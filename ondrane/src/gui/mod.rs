//! Controller binding the Slint UI (ui/*.slint) to libVLC, the media library
//! and the settings. All UI state lives in the `AppState` global; every user
//! action arrives through a handful of generic callbacks and is dispatched by
//! [`Controller::command`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use crate::customization::{Profile, Tool, ToolbarConfig};
use crate::engine::{PlaybackState, VlcEngine, VlcPlayer};
use crate::i18n::{tr, tr_fmt};
use crate::library::{self, LibraryStore, MediaKind, SortKey};
use crate::pickers;
use crate::settings::{OndraneSettings, ResumeMode};
use crate::subtitles;
use crate::thumbs::{PosterWorker, ThumbnailCache, ThumbnailMaker};
use crate::tokens::ThemeMode;

slint::include_modules!();
include!(concat!(env!("OUT_DIR"), "/icon_lookup.rs"));

mod extras;
mod keys;
mod lifecycle;
mod menus;
mod pages;
mod prefs;
#[cfg(all(test, target_os = "linux"))]
mod smoke;
mod window;

pub use keys::ACTIONS as SHORTCUT_ACTIONS;

/// "00:02:10" style clock used by the Figma control bar.
pub fn fmt_clock(ms: i64) -> String {
    let s = (ms.max(0) / 1000) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// "23:06" / "1:02:03" style durations for lists.
pub fn fmt_short(ms: i64) -> String {
    let s = (ms.max(0) / 1000) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "2h 3m" summary used by the queue header.
pub fn fmt_total(ms: i64) -> String {
    let m = (ms.max(0) / 60_000) as u64;
    if m >= 60 {
        format!("{}h {}m", m / 60, m % 60)
    } else {
        format!("{}m", m)
    }
}

pub fn parse_hex_color(hex: &str) -> Option<slint::Color> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(slint::Color::from_rgb_u8(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ))
}

pub const ACCENTS: [&str; 9] = [
    "#78E3CB", "#E15032", "#C49E23", "#19A234", "#18AAD3", "#185DD3", "#8E42FF", "#F052B3",
    "#AFAFAF",
];
pub const SPEEDS: [f32; 8] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
pub const ASPECTS: [(&str, &str); 7] = [
    ("Automatic", ""),
    ("16:9", "16:9"),
    ("4:3", "4:3"),
    ("16:10", "16:10"),
    ("21:9", "21:9"),
    ("1:1", "1:1"),
    ("5:4", "5:4"),
];
pub const CROPS: [(&str, &str); 5] = [
    ("None", ""),
    ("16:9", "16:9"),
    ("4:3", "4:3"),
    ("21:9", "21:9"),
    ("1:1", "1:1"),
];
pub const EQ_PRESETS: [&str; 18] = [
    "None",
    "Flat",
    "Classical",
    "Club",
    "Dance",
    "Full bass",
    "Full bass & treble",
    "Full treble",
    "Headphones",
    "Large hall",
    "Live",
    "Party",
    "Pop",
    "Reggae",
    "Rock",
    "Ska",
    "Soft",
    "Soft rock",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Videos,
    Music,
}

impl Section {
    fn root(self) -> PathBuf {
        match self {
            Section::Videos => library::videos_dir(),
            Section::Music => library::music_dir(),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Section::Videos => "Videos",
            Section::Music => "Music",
        }
    }
    fn filter(self) -> MediaKind {
        match self {
            Section::Videos => MediaKind::Video,
            Section::Music => MediaKind::Audio,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    Home,
    Folder { section: Section, path: PathBuf },
    Playlists,
    Playlist(usize),
    Network,
    Recent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    Media,
    Folder(PathBuf),
    Playlist(usize),
    Stream,
    Action(&'static str),
}

#[derive(Debug, Clone)]
pub struct Row {
    pub target: String,
    pub title: String,
    pub kind: RowKind,
}

pub struct State {
    pub settings: OndraneSettings,
    pub toolbar: ToolbarConfig,
    pub store: LibraryStore,

    pub queue: Vec<String>,
    pub index: Option<usize>,
    pub current: Option<String>,
    pub current_playlist: String,
    pub repeat: i32,
    pub shuffle: bool,
    pub ended: bool,
    pub length_ms: i64,
    pub show_remaining: bool,
    pub meta_loaded: bool,
    pub aspect_idx: usize,
    pub crop_idx: usize,
    pub eq_preset: usize,

    pub loc: Location,
    pub back: Vec<Location>,
    pub fwd: Vec<Location>,
    pub rows: Vec<Row>,
    pub selected: Option<usize>,
    pub sort: SortKey,
    pub search: String,

    pub posters: HashMap<String, Image>,
    pub thumbs: ThumbnailCache,
    pub thumb_maker: ThumbnailMaker,

    pub menu_item: Option<usize>,
    pub toast_until: Option<Instant>,
    pub last_save: Instant,
    pub last_pointer: Instant,

    pub sub_pairs: Vec<(PathBuf, Option<PathBuf>)>,
    pub sub_dir: Option<PathBuf>,
    pub sub_scope: i32,
    pub new_playlist_files: Vec<String>,
    pub toolbar_backup: Option<ToolbarConfig>,
    pub shortcut_edits: std::collections::BTreeMap<String, String>,
    pub settings_page: i32,
    pub settings_search: String,
    pub settings_advanced: bool,
    pub quick_tab: i32,
    pub restore_size: Option<slint::PhysicalSize>,
    /// Files of a drag & drop being collected.
    pub dropped: Vec<String>,
}

pub struct Controller {
    window: slint::Weak<MainWindow>,
    /// libVLC path, kept to restart the engine with new instance options.
    /// Set once the background engine load has finished.
    libvlc: std::cell::OnceCell<PathBuf>,
    engine: RefCell<Option<VlcEngine>>,
    /// libVLC is loaded off the UI thread so the window shows immediately;
    /// the result arrives here and is picked up by the UI timer.
    engine_rx: RefCell<Option<EngineReceiver>>,
    /// Files / subtitle given on the command line, opened once the engine is up.
    startup_media: RefCell<Option<(Vec<String>, Option<String>)>>,
    player: RefCell<Option<Rc<VlcPlayer>>>,
    st: RefCell<State>,
    items: Rc<VecModel<MediaItem>>,
    posters: std::cell::OnceCell<PosterWorker>,
    x: RefCell<extras::Extras>,
}

/// Result of the background libVLC load: its path and the initialised engine.
pub type EngineReceiver = std::sync::mpsc::Receiver<Option<(PathBuf, VlcEngine)>>;

thread_local! {
    static APP: RefCell<Option<Rc<Controller>>> = const { RefCell::new(None) };
}

fn with_app(f: impl FnOnce(&Rc<Controller>)) {
    let app = APP.with(|a| a.borrow().clone());
    if let Some(c) = app {
        f(&c);
    }
}

/// Run `f` on the UI thread with the controller (callable from any thread).
fn post(f: impl FnOnce(&Rc<Controller>) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || with_app(f));
}

/// Run a blocking job (file picker...) on a worker thread, then `done` on the UI thread.
fn background<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&Rc<Controller>, T) + Send + 'static,
) {
    std::thread::spawn(move || {
        let v = job();
        post(move |c| done(c, v));
    });
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

impl Controller {
    fn ui(&self) -> MainWindow {
        self.window.upgrade().expect("main window alive")
    }

    /// Poster generation, off when "Use the media library" is off.
    fn poster_worker(&self) -> Option<&PosterWorker> {
        if self.st.borrow().settings.ob("lib.use-ml") {
            self.posters.get()
        } else {
            None
        }
    }

    /// The current libVLC player (None without libVLC).
    pub fn player(&self) -> Option<Rc<VlcPlayer>> {
        self.player.borrow().clone()
    }

    pub fn icon(&self, name: &str) -> IconData {
        let w = self.ui();
        icon_by_name(&w.global::<Icons>(), name)
    }

    pub fn toast(&self, text: impl AsRef<str>) {
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_toast(ss(tr(text.as_ref())));
        a.set_toast_visible(true);
        self.st.borrow_mut().toast_until = Some(Instant::now() + Duration::from_millis(2200));
    }

    fn save_settings(&self) {
        let st = self.st.borrow();
        let _ = st.settings.save_to_file(&OndraneSettings::config_path());
    }

    fn save_store(&self) {
        self.st.borrow().store.save();
    }

    // ------------------------------------------------------------ theme
    pub fn apply_theme(&self) {
        let w = self.ui();
        let theme = w.global::<Theme>();
        let st = self.st.borrow();
        let dark = match st.settings.app.global.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            _ => system_prefers_dark(),
        };
        let palette = st
            .settings
            .app
            .global
            .color_scheme
            .palette(st.settings.app.global.catppuccin_flavor);
        theme.set_dark(palette.as_ref().map_or(dark, |p| p.dark));
        if let Some(c) = parse_hex_color(
            palette
                .as_ref()
                .map_or(st.settings.app.global.accent_color.as_str(), |p| p.accent),
        ) {
            theme.set_accent(c);
        }
        drop(st);
        self.apply_skin();
    }

    // ------------------------------------------------------- playback
    fn media_options(&self, target: &str) -> Vec<String> {
        let st = self.st.borrow();
        let s = &st.settings;
        let mut o = crate::options::media_options(s, library::is_url(target));
        if target.starts_with("fd://") {
            o.extend([":demux=avformat".into(), ":file-caching=100".into()]);
        }
        let resume =
            s.app.privacy.remember_position && s.media.playback.resume_mode != ResumeMode::Never;
        if resume {
            if let Some(pos) = st.store.resume_position(target) {
                o.push(format!(":start-time={:.3}", pos as f64 / 1000.0));
            }
        }
        o
    }

    pub fn play_index(self: &Rc<Self>, idx: usize) {
        #[cfg(target_os = "linux")]
        {
            let mut x = self.x.borrow_mut();
            x.capture_generation = x.capture_generation.wrapping_add(1);
            let target = self.st.borrow().queue.get(idx).cloned();
            if let Some(capture) = x.screen_capture.as_mut() {
                if Some(capture.mrl()) != target {
                    x.screen_capture = None;
                } else {
                    if let Some(p) = self.player() {
                        p.stop();
                    }
                    match capture.restart() {
                        Ok(mrl) => self.st.borrow_mut().queue[idx] = mrl,
                        Err(e) => {
                            x.screen_capture = None;
                            drop(x);
                            self.show_error(&tr("Screen capture"), &e);
                            return;
                        }
                    }
                }
            }
        }
        self.save_progress();
        let Some(target) = self.st.borrow().queue.get(idx).cloned() else {
            return;
        };
        let w = self.ui();
        let a = w.global::<AppState>();
        let path = PathBuf::from(&target);
        let is_url = library::is_url(&target);
        let kind = if is_url {
            MediaKind::Video
        } else {
            library::media_kind(&path)
        };
        let audio = kind == MediaKind::Audio;
        let mut options = self.media_options(&target);
        let restart = self.x.borrow_mut().start_override.take();
        if let Some((time, _)) = restart.filter(|_| !target.starts_with("fd://")) {
            options.retain(|o| !o.starts_with(":start-time"));
            options.push(format!(":start-time={:.3}", time as f64 / 1000.0));
        }
        options.extend(self.session_options(&target));
        let resumed = restart.is_none() && options.iter().any(|o| o.starts_with(":start-time"));
        {
            let mut st = self.st.borrow_mut();
            st.index = Some(idx);
            st.current = Some(target.clone());
            st.ended = false;
            st.length_ms = 0;
            st.meta_loaded = false;
            if remembers(&st.settings, &target) {
                st.store.record_open(&target);
            }
        }
        let Some(p) = self.player() else {
            self.playback_unavailable();
            return;
        };
        p.set_renderer(self.x.borrow().cast_target.as_deref());
        if p.open_with_options(&target, &options).is_err() {
            {
                let mut st = self.st.borrow_mut();
                st.ended = true;
                st.current = None;
            }
            a.set_playing(false);
            a.set_has_media(false);
            self.show_error(&tr("Cannot open"), &library::display_name(&target));
            return;
        }
        p.play();
        if restart.is_some_and(|(_, paused)| paused) {
            p.pause();
        }
        let st = self.st.borrow();
        p.set_rate(st.settings.media.playback.default_speed);
        p.set_subtitle_delay_ms(st.settings.media.subtitles.delay_ms as i64);
        p.set_audio_delay_ms(st.settings.media.audio.audio_delay_ms as i64);
        drop(st);
        if !is_url && self.st.borrow().settings.media.subtitles.auto_load {
            if let Some(sub) = self.matched_subtitle(&path) {
                let _ = p.add_subtitle_file(&sub.to_string_lossy());
            }
        }
        self.apply_audio_effects();
        self.apply_picture();
        self.apply_deinterlace();
        let name = library::display_name(&target);
        let folder = if is_url {
            tr("Network")
        } else {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        let show_name = self
            .st
            .borrow()
            .settings
            .app
            .interface
            .show_filename_in_title;
        a.set_has_media(true);
        a.set_ended(false);
        a.set_audio_mode(audio);
        a.set_has_frame(false);
        a.set_media_name(ss(&name));
        a.set_media_folder(ss(&folder));
        a.set_media_playlist(ss(&self.st.borrow().current_playlist));
        a.set_folder_label(ss(if folder.is_empty() {
            tr("Videos")
        } else {
            folder.clone()
        }));
        a.set_window_title(ss(if show_name {
            library::pretty_title(&target)
        } else {
            tr("Ondrane")
        }));
        a.set_media_line(ss(library::pretty_title(&target)));
        a.set_favorite(self.st.borrow().store.is_favorite(&target));
        a.set_position(0.0);
        a.set_time_text("00:00:00".into());
        a.set_duration_text("00:00:00".into());
        a.set_waveform(model(if audio {
            library::waveform_for(&target, 220)
        } else {
            vec![]
        }));
        a.set_has_art(false);
        if audio {
            if let Some(cover) = find_cover(&path) {
                if let Ok(img) = Image::load_from_path(&cover) {
                    a.set_art(img);
                    a.set_has_art(true);
                }
            }
        }
        // seek-bar previews
        if !is_url && !audio && self.st.borrow().settings.media.playback.seek_thumbnails {
            let mut st = self.st.borrow_mut();
            st.thumb_maker.cancel();
            if !st.thumbs.load_for_media(&path) && path.is_file() {
                let dir = st.thumbs.cache_dir().to_path_buf();
                let p2 = path.clone();
                st.thumb_maker.start(path.clone(), dir, move || {
                    let p3 = p2.clone();
                    post(move |c| {
                        let mut st = c.st.borrow_mut();
                        if st.current.as_deref() == Some(p3.to_string_lossy().as_ref()) {
                            st.thumbs.reload_for_media(&p3);
                        }
                    });
                });
            }
        }
        if resumed {
            self.toast("Resumed where you left off · Home to restart");
        } else if !audio && restart.is_none() {
            let (on, secs) = {
                let s = &self.st.borrow().settings;
                (s.ob("osd.filename"), s.of("osd.filename-dur"))
            };
            if on {
                self.toast_for(&name, secs);
            }
        }
        self.refresh_queue();
        self.refresh_browse_marks();
    }

    /// Point the browser at the folder of the playing file (Figma side panel
    /// lists the current folder with the playing item highlighted).
    fn follow_current_folder(self: &Rc<Self>) {
        let cur = self.st.borrow().current.clone();
        let Some(cur) = cur.filter(|c| !library::is_url(c)) else {
            return;
        };
        let path = PathBuf::from(&cur);
        let Some(parent) = path.parent() else { return };
        let section = if library::media_kind(&path) == MediaKind::Audio {
            Section::Music
        } else {
            Section::Videos
        };
        let loc = Location::Folder {
            section,
            path: parent.to_path_buf(),
        };
        if self.st.borrow().loc != loc {
            self.navigate(loc, true);
        }
    }

    fn matched_subtitle(&self, media: &Path) -> Option<PathBuf> {
        let st = self.st.borrow();
        if let Some((_, Some(s))) = st.sub_pairs.iter().find(|(m, _)| m == media) {
            return Some(s.clone());
        }
        drop(st);
        subtitles::find_for_media(media)
    }

    pub fn open_targets(self: &Rc<Self>, targets: Vec<String>, append: bool) {
        let mut expanded = Vec::new();
        for t in targets {
            let p = PathBuf::from(&t);
            if !library::is_url(&t) && p.is_dir() {
                // "When opening a folder": include subfolders or not
                let (recursive, hidden) = {
                    let s = &self.st.borrow().settings;
                    (s.oi("lib.folder") == 0, s.media.library.show_hidden)
                };
                let mut stack = vec![p.clone()];
                while let Some(dir) = stack.pop() {
                    let mut subdirs = Vec::new();
                    for e in library::list_dir(&dir, None, hidden) {
                        if e.is_dir {
                            subdirs.push(e.path);
                        } else {
                            expanded.push(e.path.to_string_lossy().to_string());
                        }
                    }
                    if recursive {
                        subdirs.reverse();
                        stack.extend(subdirs);
                    }
                }
            } else if !library::is_url(&t) && library::media_kind(&p) == MediaKind::Playlist {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    expanded.extend(library::parse_m3u(&text, p.parent()));
                }
            } else if library::media_kind(&p) == MediaKind::Subtitle {
                if let Some(pl) = self.player() {
                    let _ = pl.add_subtitle_file(&t);
                    self.toast(tr_fmt("Subtitles: {}", &[&library::display_name(&t)]));
                }
            } else {
                expanded.push(t);
            }
        }
        if expanded.is_empty() {
            return;
        }
        let start = {
            let mut st = self.st.borrow_mut();
            if append && !st.queue.is_empty() {
                let start = st.queue.len();
                st.queue.extend(expanded);
                if st.current.is_none() {
                    Some(start)
                } else {
                    None
                }
            } else {
                st.queue = expanded;
                st.current_playlist.clear();
                Some(0)
            }
        };
        if let Some(i) = start {
            self.play_index(i);
            self.follow_current_folder();
        } else {
            self.refresh_queue();
            self.toast("Added to queue");
        }
    }

    fn play_rows_from(self: &Rc<Self>, idx: usize, playlist_name: &str) {
        let (queue, start) = {
            let st = self.st.borrow();
            let media: Vec<(usize, String)> = st
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r.kind, RowKind::Media | RowKind::Stream))
                .map(|(i, r)| (i, r.target.clone()))
                .collect();
            let start = media.iter().position(|(i, _)| *i == idx).unwrap_or(0);
            (media.into_iter().map(|(_, t)| t).collect::<Vec<_>>(), start)
        };
        if queue.is_empty() {
            return;
        }
        {
            let mut st = self.st.borrow_mut();
            st.queue = queue;
            st.current_playlist = playlist_name.to_string();
        }
        self.play_index(start);
    }

    pub fn toggle_play(self: &Rc<Self>) {
        let Some(p) = self.player() else {
            self.playback_unavailable();
            return;
        };
        let (has, ended, idx) = {
            let st = self.st.borrow();
            (st.current.is_some(), st.ended, st.index)
        };
        if !has {
            let resume_idx = idx.unwrap_or(0);
            if resume_idx < self.st.borrow().queue.len() {
                self.play_index(resume_idx);
            } else if !self.st.borrow().queue.is_empty() {
                self.play_index(0);
            } else {
                self.command("open-files");
            }
            return;
        }
        if ended || p.state() == PlaybackState::Stopped {
            if let Some(i) = idx {
                self.play_index(i);
            }
            return;
        }
        p.toggle_play_pause();
    }

    pub fn stop(&self) {
        #[cfg(target_os = "linux")]
        {
            let mut x = self.x.borrow_mut();
            x.capture_generation = x.capture_generation.wrapping_add(1);
            x.screen_capture = None;
        }
        self.save_progress();
        if let Some(p) = self.player() {
            p.stop();
        }
        let w = self.ui();
        let a = w.global::<AppState>();
        {
            let mut st = self.st.borrow_mut();
            st.current = None;
            st.ended = false;
            st.thumb_maker.cancel();
        }
        a.set_has_media(false);
        a.set_playing(false);
        a.set_has_frame(false);
        a.set_audio_mode(false);
        a.set_position(0.0);
        a.set_time_text("00:00:00".into());
        a.set_duration_text("00:00:00".into());
        a.set_window_title(ss(tr("Ondrane")));
        self.refresh_queue();
        self.refresh_browse_marks();
    }

    pub fn next(self: &Rc<Self>, auto: bool) {
        let (len, idx, shuffle, repeat) = {
            let st = self.st.borrow();
            (st.queue.len(), st.index, st.shuffle, st.repeat)
        };
        if len == 0 {
            self.ui().global::<AppState>().set_playing(false);
            return;
        }
        let next = if shuffle && len > 1 {
            let cur = idx.unwrap_or(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            let r = (nanos ^ cur.wrapping_mul(1664525)).wrapping_mul(2654435761) % (len - 1);
            Some(if r >= cur { r + 1 } else { r })
        } else {
            match idx {
                Some(i) if i + 1 < len => Some(i + 1),
                _ if repeat == 1 => Some(0),
                Some(i) if auto => {
                    // binge mode: continue with the next file of the folder
                    let st = self.st.borrow();
                    let cur = PathBuf::from(&st.queue[i]);
                    let follow =
                        st.settings.media.playback.auto_play_next && !library::is_url(&st.queue[i]);
                    drop(st);
                    if follow {
                        let sibs = library::folder_siblings(&cur);
                        if let Some(pos) = sibs.iter().position(|p| *p == cur) {
                            if let Some(n) = sibs.get(pos + 1) {
                                self.st
                                    .borrow_mut()
                                    .queue
                                    .push(n.to_string_lossy().to_string());
                                Some(i + 1)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                None => Some(0),
                _ => None,
            }
        };
        match next {
            Some(n) => self.play_index(n),
            None if auto => {
                let w = self.ui();
                w.global::<AppState>().set_playing(false);
                match self.st.borrow().settings.oi("play.finish") {
                    1 => self.command("open-library"),
                    2 => self.command("quit"),
                    _ => {}
                }
            }
            None => self.toast("End of the queue"),
        }
    }

    pub fn prev(self: &Rc<Self>) {
        let Some(p) = self.player() else { return };
        let (idx, len, repeat) = {
            let st = self.st.borrow();
            (st.index, st.queue.len(), st.repeat)
        };
        let cur_time = p.time_ms();
        if cur_time > 3000 || idx.unwrap_or(0) == 0 {
            if idx.unwrap_or(0) == 0 && repeat == 1 && len > 1 && cur_time <= 3000 {
                self.play_index(len - 1);
                return;
            }
            p.seek_ms(0);
            if let Some(i) = idx {
                if self.st.borrow().ended {
                    self.play_index(i);
                }
            }
            return;
        }
        if let Some(i) = idx {
            self.play_index(i - 1);
        }
    }

    fn on_ended(self: &Rc<Self>) {
        let (cur, len, repeat, stop_after) = {
            let st = self.st.borrow();
            (
                st.current.clone(),
                st.length_ms,
                st.repeat,
                st.settings.media.playback.stop_after_each,
            )
        };
        if let Some(t) = &cur {
            let mut st = self.st.borrow_mut();
            if remembers(&st.settings, t) {
                st.store.record_progress(t, len, len);
                st.store.save();
            }
        }
        let w = self.ui();
        w.global::<AppState>().set_ended(true);
        if repeat == 2 {
            let i = self.st.borrow().index.unwrap_or(0);
            self.x.borrow_mut().start_override = Some((0, false));
            self.play_index(i);
        } else if !stop_after {
            self.next(true);
        } else {
            w.global::<AppState>().set_playing(false);
        }
        self.refresh_browse_marks();
    }

    fn save_progress(&self) {
        let Some(p) = self.player() else { return };
        let mut st = self.st.borrow_mut();
        if st.ended {
            return;
        }
        if let Some(t) = st.current.clone().filter(|t| remembers(&st.settings, t)) {
            let (time, len) = (p.time_ms(), p.length_ms());
            if time > 0 {
                st.store.record_progress(&t, time, len);
                st.store.save();
            }
        }
        st.last_save = Instant::now();
    }

    pub fn jump(&self, secs: i64) {
        if let Some(p) = self.player() {
            p.seek_relative_ms(secs * 1000);
            self.osd(format!("{}{} s", if secs > 0 { "+" } else { "" }, secs));
        }
    }

    pub fn set_volume(&self, v: i32) {
        let max = self.st.borrow().settings.media.audio.max_volume as i32;
        let v = v.clamp(0, max);
        if let Some(p) = self.player() {
            p.set_volume(v);
            if p.is_muted() && v > 0 {
                p.set_muted(false);
            }
        }
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_volume(v);
        a.set_muted(false);
        self.st.borrow_mut().settings.media.audio.last_volume = v as u32;
    }

    pub fn set_rate(&self, r: f32) {
        let r = r.clamp(0.25, 4.0);
        if let Some(p) = self.player() {
            p.set_rate(r);
        }
        self.ui().global::<AppState>().set_rate(r);
        self.osd(tr_fmt("Speed {}", &[&format!("{:.2}x", r)]));
    }

    pub fn apply_audio_effects(&self) {
        let Some(p) = self.player() else { return };
        let st = self.st.borrow();
        let a = &st.settings.media.audio;
        if a.equalizer_enabled && a.equalizer_db.len() == 11 {
            let mut bands = [0.5f32; 10];
            for (i, b) in bands.iter_mut().enumerate() {
                *b = ((a.equalizer_db[i + 1] + 20.0) / 40.0).clamp(0.0, 1.0);
            }
            p.set_equalizer_bands(&bands, a.equalizer_db[0]);
        } else {
            // flat, 0 dB preamp
            p.set_equalizer_bands(&[0.5; 10], 0.0);
        }
    }

    pub fn apply_picture(&self) {
        let Some(p) = self.player() else { return };
        let st = self.st.borrow();
        let pa = &st.settings.media.video.picture_adjust;
        let neutral = (pa.brightness - 1.0).abs() < 0.01
            && (pa.contrast - 1.0).abs() < 0.01
            && (pa.saturation - 1.0).abs() < 0.01
            && (pa.gamma - 1.0).abs() < 0.01
            && pa.hue.abs() < 0.5;
        p.set_picture(
            !neutral,
            pa.brightness,
            pa.contrast,
            pa.saturation,
            pa.gamma,
            pa.hue,
        );
        let _ = p.set_aspect_ratio(ASPECTS[st.aspect_idx.min(ASPECTS.len() - 1)].1);
        // "Video - Crop" panel (pixels) wins over the ratio crop
        let px: Vec<i64> = ["crop.left", "crop.top", "crop.right", "crop.bottom"]
            .iter()
            .map(|k| st.settings.oi(k).max(0))
            .collect();
        if px.iter().any(|v| *v > 0) {
            p.set_crop(&format!("{}+{}+{}+{}", px[0], px[1], px[2], px[3]));
        } else {
            p.set_crop(CROPS[st.crop_idx.min(CROPS.len() - 1)].1);
        }
    }

    pub fn sync_eq_ui(&self) {
        let w = self.ui();
        let eq = w.global::<EqState>();
        let st = self.st.borrow();
        let db = &st.settings.media.audio.equalizer_db;
        let bands: Vec<f32> = db
            .iter()
            .map(|d| ((d + 20.0) / 40.0).clamp(0.0, 1.0))
            .collect();
        let values: Vec<SharedString> = db.iter().map(|d| ss(format!("{:.1} dB", d))).collect();
        eq.set_bands(model(bands));
        eq.set_values(model(values));
    }

    fn snapshot(&self) {
        let Some(p) = self.player() else { return };
        let (dir, ext, prefix, sequential, width, preview) = {
            let st = self.st.borrow();
            let s = &st.settings;
            (
                s.system.recording.snapshot_dir.clone(),
                match s.system.recording.snapshot_format {
                    crate::settings::SnapshotFormat::Jpeg => "jpg",
                    _ => "png",
                },
                s.os("snap.prefix"),
                s.ob("snap.sequential"),
                crate::options::SNAP_WIDTHS[(s.oi("snap.size").clamp(0, 4)) as usize].1,
                s.ob("snap.preview"),
            )
        };
        let _ = std::fs::create_dir_all(&dir);
        let stem = if prefix.trim().is_empty() {
            self.st
                .borrow()
                .current
                .as_deref()
                .map(library::pretty_title)
                .unwrap_or_else(|| "vlc".into())
                .replace(' ', "_")
                + "-"
        } else {
            prefix.trim().to_string()
        };
        let file = if sequential {
            (1..100_000)
                .map(|n| dir.join(format!("{stem}{n:05}.{ext}")))
                .find(|f| !f.exists())
                .unwrap_or_else(|| dir.join(format!("{stem}{}.{ext}", library::now_secs())))
        } else {
            dir.join(format!("{stem}{}.{ext}", library::now_secs()))
        };
        match p.take_snapshot(&file, width, 0) {
            Ok(()) => {
                self.toast(tr_fmt("Snapshot saved to {}", &[&library::tilde(&file)]));
                if preview {
                    if let Ok(img) = Image::load_from_path(&file) {
                        let w = self.ui();
                        let a = w.global::<AppState>();
                        a.set_snap_image(img);
                        a.set_snap_visible(true);
                        let weak = w.as_weak();
                        slint::Timer::single_shot(Duration::from_millis(2500), move || {
                            if let Some(w) = weak.upgrade() {
                                w.global::<AppState>().set_snap_visible(false);
                            }
                        });
                    }
                }
            }
            Err(e) => self.show_error(&tr("Snapshot failed"), &e.to_string()),
        }
    }

    fn cycle_audio_track(&self) {
        let Some(p) = self.player() else { return };
        let tracks = p.audio_tracks();
        if tracks.is_empty() {
            return self.osd(tr("No audio track"));
        }
        let cur = p.current_audio_track();
        let pos = tracks
            .iter()
            .position(|t| t.id == cur)
            .map(|i| (i + 1) % tracks.len())
            .unwrap_or(0);
        p.set_audio_track(tracks[pos].id);
        self.osd(tr_fmt("Audio: {}", &[&tracks[pos].name]));
    }

    fn cycle_subtitle_track(&self) {
        let Some(p) = self.player() else { return };
        let tracks = p.subtitle_tracks();
        if tracks.is_empty() {
            return self.osd(tr("No subtitles"));
        }
        let cur = p.current_subtitle_track();
        let pos = tracks
            .iter()
            .position(|t| t.id == cur)
            .map(|i| (i + 1) % tracks.len())
            .unwrap_or(0);
        p.set_subtitle_track(tracks[pos].id);
        self.osd(tr_fmt("Subtitles: {}", &[&tracks[pos].name]));
    }

    // --------------------------------------------------------- polling
    fn tick(self: &Rc<Self>) {
        #[cfg(target_os = "linux")]
        if self
            .x
            .borrow_mut()
            .screen_capture
            .as_mut()
            .is_some_and(|c| c.has_exited())
        {
            self.stop();
            self.toast(tr("Screen sharing ended"));
        }
        self.poll_engine_load();
        let w = self.ui();
        let a = w.global::<AppState>();
        if let Some(until) = self.st.borrow().toast_until {
            if Instant::now() > until {
                a.set_toast_visible(false);
            }
        }
        // fullscreen auto-hide
        let hide_mode = self.st.borrow().settings.app.interface.auto_hide_controls;
        let delay = self
            .st
            .borrow()
            .settings
            .app
            .interface
            .controls_hide_delay_ms
            .max(500) as u64;
        let idle = self.st.borrow().last_pointer.elapsed() > Duration::from_millis(delay);
        let should_hide = a.get_playing()
            && idle
            && !a.get_menu_open()
            && !a.get_quick_open()
            && match hide_mode {
                0 => false,
                1 => a.get_fullscreen(),
                _ => true,
            };
        let fs_hide = a.get_fullscreen() && !a.get_fs_controls();
        w.global::<Chrome>()
            .set_controls_visible(!should_hide && !fs_hide);

        self.run_pending_apply();
        self.poll_player_events();
        self.poll_convert();
        let playing_now = self
            .player()
            .map(|p| p.state() == PlaybackState::Playing)
            .unwrap_or(false);
        self.tick_services(playing_now);

        let Some(p) = self.player() else { return };
        if self.st.borrow().current.is_none() {
            return;
        }
        let state = p.state();
        let time = p.time_ms();
        let len = p.length_ms();
        let playing = state == PlaybackState::Playing;
        a.set_playing(playing);
        if len > 0 {
            self.st.borrow_mut().length_ms = len;
            a.set_position((time as f32 / len as f32).clamp(0.0, 1.0));
        }
        a.set_time_text(ss(fmt_clock(time)));
        let remaining = self.st.borrow().show_remaining;
        a.set_duration_text(ss(if remaining && len > 0 {
            format!("-{}", fmt_clock((len - time).max(0)))
        } else {
            fmt_clock(len)
        }));
        a.set_volume(p.volume());
        a.set_muted(p.is_muted());

        if state == PlaybackState::Ended && !self.st.borrow().ended {
            self.st.borrow_mut().ended = true;
            self.on_ended();
            return;
        }
        if state == PlaybackState::Error && !self.st.borrow().ended {
            self.st.borrow_mut().ended = true;
            self.toast(tr("Playback error: this media cannot be played"));
            a.set_playing(false);
        }
        // metadata (tags & artwork) once the input is running
        if playing && time > 0 && !self.st.borrow().meta_loaded {
            self.st.borrow_mut().meta_loaded = true;
            let title = p.media_meta(0);
            let artist = p.media_meta(1);
            if a.get_audio_mode() {
                let t = title
                    .clone()
                    .unwrap_or_else(|| a.get_media_line().to_string());
                a.set_media_line(ss(match &artist {
                    Some(ar) => format!("{} - {}", t, ar),
                    None => t,
                }));
                if let Some(art) = p.media_meta(15) {
                    if let Ok(url) = url::Url::parse(&art) {
                        if let Ok(path) = url.to_file_path() {
                            if let Ok(img) = Image::load_from_path(&path) {
                                a.set_art(img);
                                a.set_has_art(true);
                            }
                        }
                    }
                }
            }
            if a.get_audio_mode() {
                let album = p.media_meta(4).unwrap_or_default();
                self.scrobble_start(
                    artist.as_deref().unwrap_or(""),
                    title.as_deref().unwrap_or(""),
                    &album,
                    len,
                );
            }
            // libVLC falls back to the file name when there is no title tag.
            let file_name = self
                .st
                .borrow()
                .current
                .as_deref()
                .and_then(|c| std::path::Path::new(c).file_name())
                .map(|f| f.to_string_lossy().to_string());
            let popup_title = title
                .clone()
                .filter(|t| !t.contains("://") && Some(t) != file_name.as_ref())
                .unwrap_or_else(|| a.get_media_name().to_string());
            self.track_popup(&popup_title, &a.get_media_line());
            if a.get_audio_mode()
                || library::is_url(self.st.borrow().current.as_deref().unwrap_or(""))
            {
                if let Some(t) = title.filter(|t| !t.contains("://")) {
                    a.set_window_title(ss(&t));
                }
            }
        }
        if self.st.borrow().last_save.elapsed() > Duration::from_secs(5) && playing {
            self.save_progress();
        }
    }

    // ---------------------------------------------------------- browsing
    pub fn navigate(self: &Rc<Self>, loc: Location, push: bool) {
        {
            let mut st = self.st.borrow_mut();
            if push && st.loc != loc {
                let prev = st.loc.clone();
                st.back.push(prev);
                st.fwd.clear();
            }
            st.loc = loc;
            st.selected = None;
            st.search.clear();
        }
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_search("".into());
        a.set_nav(match self.st.borrow().loc {
            Location::Home => 0,
            Location::Folder {
                section: Section::Videos,
                ..
            } => 1,
            Location::Folder {
                section: Section::Music,
                ..
            } => 2,
            Location::Network => 3,
            Location::Playlists | Location::Playlist(_) => 4,
            Location::Recent => 5,
        });
        self.refresh_browse();
    }

    fn nav_section(self: &Rc<Self>, n: i32) {
        // Re-selecting a section keeps the folder being browsed in it, or
        // jumps to the folder of the playing file (Figma side panel).
        let keep = {
            let st = self.st.borrow();
            let want = match n {
                1 => Some(Section::Videos),
                2 => Some(Section::Music),
                _ => None,
            };
            match (&st.loc, want) {
                (Location::Folder { section, .. }, Some(w)) if *section == w => {
                    Some(st.loc.clone())
                }
                (_, Some(w)) => st
                    .current
                    .as_ref()
                    .filter(|c| !library::is_url(c))
                    .map(PathBuf::from)
                    .filter(|p| {
                        let k = library::media_kind(p);
                        (w == Section::Music) == (k == MediaKind::Audio)
                    })
                    .and_then(|p| {
                        p.parent().map(|d| Location::Folder {
                            section: w,
                            path: d.to_path_buf(),
                        })
                    }),
                _ => None,
            }
        };
        let loc = keep.unwrap_or(match n {
            0 => Location::Home,
            1 => Location::Folder {
                section: Section::Videos,
                path: Section::Videos.root(),
            },
            2 => Location::Folder {
                section: Section::Music,
                path: Section::Music.root(),
            },
            3 => Location::Network,
            4 => Location::Playlists,
            _ => Location::Recent,
        });
        let w = self.ui();
        let a = w.global::<AppState>();
        if a.get_view() == 0 {
            // player mode: the rail toggles the browsing side panel
            if a.get_side_panel() && a.get_nav() == n {
                a.set_side_panel(false);
                return;
            }
            if n == 0 {
                // Home lives in the library
                a.set_view(1);
            } else {
                a.set_side_panel(true);
            }
        }
        self.navigate(loc, true);
    }

    fn crumbs_for(&self, loc: &Location) -> Vec<Crumb> {
        let st = self.st.borrow();
        let c = |label: &str, index: i32| Crumb {
            label: ss(tr(label)),
            index,
        };
        match loc {
            Location::Home => vec![c("Home", 0)],
            Location::Network => vec![c("Network", 0)],
            Location::Recent => vec![c("Recent", 0)],
            Location::Playlists => vec![c("Playlists", 0)],
            Location::Playlist(i) => vec![
                c("Playlists", 0),
                c(
                    st.store
                        .playlists
                        .get(*i)
                        .map(|p| p.name.as_str())
                        .unwrap_or("?"),
                    1,
                ),
            ],
            Location::Folder { section, path } => {
                let root = section.root();
                let mut out = Vec::new();
                if let Ok(rel) = path.strip_prefix(&root) {
                    out.push(c(section.label(), 0));
                    for (i, comp) in rel.components().enumerate() {
                        out.push(c(&comp.as_os_str().to_string_lossy(), i as i32 + 1));
                    }
                } else {
                    for (i, comp) in path.components().enumerate() {
                        let s = comp.as_os_str().to_string_lossy().to_string();
                        out.push(c(if s.is_empty() { "/" } else { &s }, i as i32 + 1000));
                    }
                }
                out
            }
        }
    }

    fn crumb(self: &Rc<Self>, index: i32) {
        let loc = self.st.borrow().loc.clone();
        let target = match loc {
            Location::Playlist(_) if index == 0 => Some(Location::Playlists),
            Location::Folder { section, path } if index >= 0 => {
                if index >= 1000 {
                    let p: PathBuf = path
                        .components()
                        .take((index - 1000 + 1) as usize)
                        .collect();
                    Some(Location::Folder { section, path: p })
                } else {
                    let root = section.root();
                    let mut p = root.clone();
                    if let Ok(rel) = path.strip_prefix(&root) {
                        for comp in rel.components().take(index as usize) {
                            p.push(comp);
                        }
                    }
                    Some(Location::Folder { section, path: p })
                }
            }
            _ => None,
        };
        if let Some(t) = target {
            self.navigate(t, true);
        }
    }

    fn media_item(&self, row: &Row, st: &State) -> MediaItem {
        let current = st.current.as_deref() == Some(row.target.as_str()) && st.current.is_some();
        let (kind, subtitle) = match &row.kind {
            RowKind::Media => {
                let k = library::media_kind(Path::new(&row.target));
                (if k == MediaKind::Audio { 1 } else { 0 }, String::new())
            }
            RowKind::Folder(_) => (2, String::new()),
            RowKind::Playlist(i) => (
                3,
                format!(
                    "{} items",
                    st.store
                        .playlists
                        .get(*i)
                        .map(|p| p.items.len())
                        .unwrap_or(0)
                ),
            ),
            RowKind::Stream => (4, String::new()),
            RowKind::Action(_) => (5, String::new()),
        };
        let entry = st.store.entry(&row.target);
        let poster = st.posters.get(&row.target).cloned();
        MediaItem {
            title: ss(&row.title),
            subtitle: ss(subtitle),
            path: ss(&row.target),
            kind,
            progress: if matches!(row.kind, RowKind::Media) {
                entry.map(|e| e.progress()).unwrap_or(0.0)
            } else {
                0.0
            },
            watched: entry.map(|e| e.watched).unwrap_or(false),
            current,
            selected: false,
            favorite: st.store.is_favorite(&row.target),
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
            duration: ss(entry
                .filter(|e| e.length_ms > 0)
                .map(|e| fmt_short(e.length_ms))
                .unwrap_or_default()),
        }
    }

    pub fn refresh_browse(self: &Rc<Self>) {
        let loc = self.st.borrow().loc.clone();
        let mut rows: Vec<Row> = Vec::new();
        let mut empty = String::new();
        {
            let st = self.st.borrow();
            let row = |target: String, title: String, kind: RowKind| Row {
                target,
                title,
                kind,
            };
            match &loc {
                Location::Home => {}
                Location::Folder { section, path } => {
                    let mut entries = library::list_dir(
                        path,
                        Some(section.filter()),
                        st.settings.media.library.show_hidden,
                    );
                    library::sort_entries(&mut entries, st.sort, &st.store);
                    for e in entries {
                        let t = e.path.to_string_lossy().to_string();
                        rows.push(if e.is_dir {
                            row(t, e.name, RowKind::Folder(e.path))
                        } else {
                            row(t, e.name, RowKind::Media)
                        });
                    }
                    if rows.is_empty() {
                        empty = tr(match section {
                            Section::Videos => {
                                "No videos here. Use + to add files or pick another folder."
                            }
                            Section::Music => {
                                "No music here. Use + to add files or pick another folder."
                            }
                        });
                    }
                }
                Location::Playlists => {
                    rows.push(row(
                        String::new(),
                        tr("Create playlist"),
                        RowKind::Action("new-playlist"),
                    ));
                    for (i, p) in st.store.playlists.iter().enumerate() {
                        rows.push(row(
                            format!("playlist:{}", i),
                            p.name.clone(),
                            RowKind::Playlist(i),
                        ));
                    }
                }
                Location::Playlist(i) => {
                    rows.push(row(
                        String::new(),
                        tr("Add"),
                        RowKind::Action("playlist-add"),
                    ));
                    if let Some(p) = st.store.playlists.get(*i) {
                        for t in &p.items {
                            let kind = if library::is_url(t) {
                                RowKind::Stream
                            } else {
                                RowKind::Media
                            };
                            rows.push(row(t.clone(), library::display_name(t), kind));
                        }
                    }
                }
                Location::Network => {
                    rows.push(row(
                        String::new(),
                        tr("Open network stream"),
                        RowKind::Action("open-network"),
                    ));
                    for h in st
                        .store
                        .history
                        .iter()
                        .filter(|h| library::is_url(&h.target))
                    {
                        rows.push(row(
                            h.target.clone(),
                            library::display_name(&h.target),
                            RowKind::Stream,
                        ));
                    }
                }
                Location::Recent => {
                    for h in &st.store.history {
                        if library::is_url(&h.target) {
                            rows.push(row(
                                h.target.clone(),
                                library::display_name(&h.target),
                                RowKind::Stream,
                            ));
                        } else if Path::new(&h.target).exists() {
                            rows.push(row(
                                h.target.clone(),
                                library::display_name(&h.target),
                                RowKind::Media,
                            ));
                        }
                    }
                    if rows.is_empty() {
                        empty = tr("Nothing played recently.");
                    }
                }
            }
            let q = st.search.to_lowercase();
            if !q.is_empty() {
                rows.retain(|r| {
                    matches!(r.kind, RowKind::Action(_)) || r.title.to_lowercase().contains(&q)
                });
            }
        }
        // cover art for audio (cover.jpg / folder.jpg next to the files)
        {
            let mut st = self.st.borrow_mut();
            let mut covers: HashMap<PathBuf, Option<Image>> = HashMap::new();
            for r in rows.iter().filter(|r| matches!(r.kind, RowKind::Media)) {
                let p = Path::new(&r.target);
                if st.posters.contains_key(&r.target) || library::media_kind(p) != MediaKind::Audio
                {
                    continue;
                }
                let Some(dir) = p.parent() else { continue };
                let cover = covers
                    .entry(dir.to_path_buf())
                    .or_insert_with(|| find_cover(p).and_then(|c| Image::load_from_path(&c).ok()))
                    .clone();
                if let Some(img) = cover {
                    st.posters.insert(r.target.clone(), img);
                }
            }
        }
        // posters for videos
        if let Some(pw) = self.poster_worker() {
            let mut st = self.st.borrow_mut();
            for r in rows.iter().filter(|r| matches!(r.kind, RowKind::Media)) {
                if st.posters.contains_key(&r.target)
                    || library::media_kind(Path::new(&r.target)) != MediaKind::Video
                {
                    continue;
                }
                if let Some(file) = pw.request(Path::new(&r.target)) {
                    if let Ok(img) = Image::load_from_path(&file) {
                        st.posters.insert(r.target.clone(), img);
                    }
                }
            }
        }
        let count = rows
            .iter()
            .filter(|r| !matches!(r.kind, RowKind::Action(_)))
            .count();
        let crumbs = self.crumbs_for(&loc);
        let items: Vec<MediaItem> = {
            let st = self.st.borrow();
            rows.iter().map(|r| self.media_item(r, &st)).collect()
        };
        self.st.borrow_mut().rows = rows;
        self.items.set_vec(items);
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_crumbs(model(crumbs));
        a.set_count_text(ss(if loc == Location::Home {
            String::new()
        } else {
            if count == 1 {
                tr("1 item")
            } else {
                tr_fmt("{} items", &[&count.to_string()])
            }
        }));
        a.set_empty_text(ss(empty));
        a.set_can_back(!self.st.borrow().back.is_empty());
        a.set_can_forward(!self.st.borrow().fwd.is_empty());
        a.set_sort_index(
            SortKey::ALL
                .iter()
                .position(|k| *k == self.st.borrow().sort)
                .unwrap_or(0) as i32,
        );
        if loc == Location::Home {
            self.refresh_home();
        }
    }

    /// Update "current"/progress marks without re-reading the folder.
    fn refresh_browse_marks(&self) {
        let st = self.st.borrow();
        for (i, r) in st.rows.iter().enumerate() {
            if i < self.items.row_count() {
                let mut it = self.items.row_data(i).unwrap();
                let fresh = self.media_item(r, &st);
                it.current = fresh.current;
                it.progress = fresh.progress;
                it.watched = fresh.watched;
                it.duration = fresh.duration;
                it.favorite = fresh.favorite;
                self.items.set_row_data(i, it);
            }
        }
    }

    fn poster_ready(&self, media: PathBuf, file: PathBuf) {
        let Ok(img) = Image::load_from_path(&file) else {
            return;
        };
        let target = media.to_string_lossy().to_string();
        let mut st = self.st.borrow_mut();
        st.posters.insert(target.clone(), img.clone());
        for (i, r) in st.rows.iter().enumerate() {
            if r.target == target {
                if let Some(mut it) = self.items.row_data(i) {
                    it.poster = img.clone();
                    it.has_poster = true;
                    self.items.set_row_data(i, it);
                }
            }
        }
        drop(st);
        if self.st.borrow().loc == Location::Home {
            self.refresh_home();
        }
    }

    fn refresh_home(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let st = self.st.borrow();
        let mk = |target: &str, title: String, subtitle: String, kind: i32, progress: f32| {
            let poster = st.posters.get(target).cloned();
            MediaItem {
                title: ss(title),
                subtitle: ss(subtitle),
                path: ss(target),
                kind,
                progress,
                has_poster: poster.is_some(),
                poster: poster.unwrap_or_default(),
                ..Default::default()
            }
        };
        let cont: Vec<MediaItem> = st
            .store
            .continue_watching(8)
            .into_iter()
            .filter(|h| Path::new(&h.target).exists())
            .map(|h| {
                let mut item = mk(
                    &h.target,
                    library::display_name(&h.target),
                    String::new(),
                    0,
                    h.progress(),
                );
                // a poster already on disk is returned at once (and never
                // reported through poster_ready), so load it here
                if !item.has_poster {
                    let cached = self
                        .poster_worker()
                        .and_then(|pw| pw.request(Path::new(&h.target)))
                        .and_then(|file| Image::load_from_path(&file).ok());
                    if let Some(img) = cached {
                        item.poster = img;
                        item.has_poster = true;
                    }
                }
                item
            })
            .collect();
        let listen: Vec<MediaItem> = st
            .store
            .listen_again(8)
            .into_iter()
            .map(|h| {
                let t = library::pretty_title(&h.target);
                let (title, artist) = match t.split_once(" - ") {
                    Some((a, b)) => (a.to_string(), b.to_string()),
                    None => (
                        t,
                        Path::new(&h.target)
                            .parent()
                            .and_then(|p| p.file_name())
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    ),
                };
                mk(&h.target, title, artist, 1, 0.0)
            })
            .collect();
        let folders: Vec<MediaItem> = st
            .store
            .frequent_folders(8)
            .into_iter()
            .map(|p| {
                let mut item = mk(
                    &p.to_string_lossy(),
                    p.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    String::new(),
                    2,
                    0.0,
                );
                // folder cards show the poster of their first video
                let first = library::list_dir(&p, Some(MediaKind::Video), false)
                    .into_iter()
                    .find(|e| !e.is_dir)
                    .map(|e| e.path);
                let poster = first.as_ref().and_then(|f| {
                    st.posters.get(&*f.to_string_lossy()).cloned().or_else(|| {
                        self.poster_worker()
                            .and_then(|pw| pw.request(f))
                            .and_then(|file| Image::load_from_path(&file).ok())
                    })
                });
                if let Some(img) = poster {
                    item.poster = img;
                    item.has_poster = true;
                }
                item
            })
            .collect();
        a.set_continue_items(model(cont));
        a.set_listen_items(model(listen));
        a.set_folder_items(model(folders));
    }

    fn item_activate(self: &Rc<Self>, i: usize) {
        let Some(row) = self.st.borrow().rows.get(i).cloned() else {
            return;
        };
        match row.kind {
            RowKind::Folder(p) => {
                let section = match &self.st.borrow().loc {
                    Location::Folder { section, .. } => *section,
                    _ => Section::Videos,
                };
                self.navigate(Location::Folder { section, path: p }, true);
            }
            RowKind::Playlist(pi) => self.navigate(Location::Playlist(pi), true),
            RowKind::Media | RowKind::Stream => {
                let name = match &self.st.borrow().loc {
                    Location::Playlist(pi) => self
                        .st
                        .borrow()
                        .store
                        .playlists
                        .get(*pi)
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                self.play_rows_from(i, &name);
            }
            RowKind::Action(cmd) => self.command(cmd),
        }
    }

    fn select_item(&self, i: Option<usize>) {
        let prev = std::mem::replace(&mut self.st.borrow_mut().selected, i);
        for idx in [prev, i].into_iter().flatten() {
            if let Some(mut it) = self.items.row_data(idx) {
                it.selected = Some(idx) == i;
                self.items.set_row_data(idx, it);
            }
        }
    }

    pub fn refresh_queue(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let st = self.st.borrow();
        let tree = st.settings.ob("ui.tree");
        let mut total = 0;
        let items: Vec<MediaItem> = st
            .queue
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let len = st.store.entry(t).map(|e| e.length_ms).unwrap_or(0);
                total += len;
                MediaItem {
                    title: ss(library::display_name(t)),
                    subtitle: ss(if tree {
                        if library::is_url(t) {
                            tr("Network")
                        } else {
                            Path::new(t)
                                .parent()
                                .and_then(|p| p.file_name())
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default()
                        }
                    } else {
                        String::new()
                    }),
                    path: ss(t),
                    current: st.index == Some(i) && st.current.is_some(),
                    duration: ss(if len > 0 {
                        fmt_short(len)
                    } else {
                        String::new()
                    }),
                    ..Default::default()
                }
            })
            .collect();
        let n = items.len();
        a.set_queue_summary(ss(format!(
            "{}{}",
            if n == 1 {
                tr("1 item")
            } else {
                tr_fmt("{} items", &[&n.to_string()])
            },
            if total > 0 {
                format!(" - {}", fmt_total(total))
            } else {
                String::new()
            }
        )));
        a.set_queue(model(items));
        a.set_shuffle(st.shuffle);
        a.set_repeat(st.repeat);
    }

    pub fn refresh_toolbar(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let icons = w.global::<Icons>();
        let st = self.st.borrow();
        let active = |t: Tool| match t {
            Tool::Repeat => st.repeat != 0,
            Tool::Shuffle => st.shuffle,
            Tool::Favorite => a.get_favorite(),
            Tool::RightSidebar => a.get_side_panel(),
            Tool::Record => self.x.borrow().recording.is_some(),
            Tool::Stream => self.x.borrow().streaming.is_some(),
            Tool::Cast => self.cast_name().is_some(),
            _ => false,
        };
        let icon_name = |t: Tool| {
            if t == Tool::Record && active(t) {
                "record-active"
            } else if t == Tool::Stream && active(t) {
                "streaming-active"
            } else if t == Tool::Cast && active(t) {
                "tv-active"
            } else {
                t.icon_name()
            }
        };
        let bar: Vec<ToolItem> = st
            .toolbar
            .tools
            .iter()
            .map(|t| ToolItem {
                id: ss(t.id()),
                label: ss(tr(t.label())),
                icon: icon_by_name(&icons, icon_name(*t)),
                active: active(*t),
            })
            .collect();
        let palette: Vec<ToolItem> = Tool::ALL
            .iter()
            .map(|t| ToolItem {
                id: ss(t.id()),
                label: ss(tr(t.label())),
                icon: icon_by_name(&icons, t.icon_name()),
                active: st.toolbar.contains(*t),
            })
            .collect();
        a.set_toolbar(model(bar));
        a.set_tool_palette(model(palette));
        a.set_cast_name(ss(self.cast_name().unwrap_or_default()));
        let x = self.x.borrow();
        a.set_recording(x.recording.is_some());
        a.set_streaming_url(ss(x.streaming.clone().unwrap_or_default()));
    }

    fn refresh_shortcuts(&self) {
        let st = self.st.borrow();
        let mut overrides = st.settings.app.controls.shortcuts.clone();
        overrides.extend(st.shortcut_edits.clone());
        let mut rows = Vec::new();
        let mut section = "";
        for (sec, id, label, _) in keys::ACTIONS {
            if *sec != section {
                section = sec;
                rows.push(ShortcutRow {
                    section: ss(tr(sec)),
                    is_header: true,
                    ..Default::default()
                });
            }
            rows.push(ShortcutRow {
                section: ss(tr(sec)),
                action: ss(tr(label)),
                key: ss(keys::key_of(id, &overrides)),
                is_header: false,
            });
        }
        let w = self.ui();
        w.global::<AppState>().set_shortcuts(model(rows));
    }

    fn refresh_subtitle_dialog(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let st = self.st.borrow();
        let pairs: Vec<SubtitlePair> = st
            .sub_pairs
            .iter()
            .map(|(m, s)| SubtitlePair {
                media: ss(m
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()),
                subtitle: ss(s
                    .as_ref()
                    .and_then(|s| s.file_name())
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()),
                matched: s.is_some(),
            })
            .collect();
        a.set_subtitle_pairs(model(pairs));
        a.set_subtitle_scope_label(ss(st
            .sub_dir
            .as_deref()
            .map(library::tilde)
            .unwrap_or_else(|| tr("Choose subtitle files…"))));
    }

    fn open_subtitles_dialog(self: &Rc<Self>) {
        let (dir, current) = {
            let st = self.st.borrow();
            let cur = st
                .current
                .clone()
                .filter(|c| !library::is_url(c))
                .map(PathBuf::from);
            let dir = cur
                .as_ref()
                .and_then(|c| c.parent().map(Path::to_path_buf))
                .or_else(|| match &st.loc {
                    Location::Folder { path, .. } => Some(path.clone()),
                    _ => None,
                });
            (dir, cur)
        };
        let scope = self.st.borrow().sub_scope;
        let media: Vec<PathBuf> = match (&dir, scope, &current) {
            (_, 1, Some(c)) => vec![c.clone()],
            (Some(d), _, _) => library::list_dir(d, Some(MediaKind::Video), false)
                .into_iter()
                .filter(|e| !e.is_dir)
                .map(|e| e.path)
                .collect(),
            _ => current.into_iter().collect(),
        };
        let subs = dir
            .as_deref()
            .map(subtitles::subtitles_in)
            .unwrap_or_default();
        {
            let mut st = self.st.borrow_mut();
            st.sub_pairs = subtitles::pair_all(&media, &subs);
            st.sub_dir = dir;
        }
        self.refresh_subtitle_dialog();
        self.ui().global::<AppState>().set_dialog(4);
    }

    // ---------------------------------------------------------- menus
    fn show_menu(self: &Rc<Self>, id: &str, x: f32, y: f32) {
        let (entries, width) = menus::build(self, id);
        if entries.is_empty() {
            return;
        }
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_menu_entries(model(entries));
        a.set_menu_width(width);
        a.set_menu_x(x);
        a.set_menu_y(y);
        a.set_submenu_open(false);
        a.set_menu_open(true);
    }

    fn show_submenu(self: &Rc<Self>, id: &str, x: f32, y: f32) {
        let entries = menus::build_sub(self, id);
        let w = self.ui();
        let a = w.global::<AppState>();
        if entries.is_empty() {
            a.set_submenu_open(false);
            return;
        }
        a.set_submenu_entries(model(entries));
        a.set_submenu_x(x);
        a.set_submenu_y(y);
        a.set_submenu_open(true);
    }

    // ------------------------------------------------------ settings
    pub fn refresh_settings(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let st = self.st.borrow();
        drop(st);
        let ctx = self.settings_ctx();
        let st = self.st.borrow();
        let (title, rows) = if st.settings_search.trim().is_empty() {
            (
                tr(pages::PAGE_TITLES[st.settings_page as usize]),
                pages::page(st.settings_page, &st, st.settings_advanced, &ctx),
            )
        } else {
            (
                tr("Search Results"),
                pages::search(&st.settings_search, &st, &ctx),
            )
        };
        a.set_settings_page(st.settings_page);
        a.set_settings_title(ss(title));
        a.set_settings_advanced(st.settings_advanced);
        a.set_settings_rows(model(rows));
    }

    pub fn refresh_quick(&self) {
        let presets = self.preset_names();
        let rows = {
            let st = self.st.borrow();
            pages::quick(st.quick_tab, &st, self.player().as_deref(), &presets)
        };
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_quick_tab(self.st.borrow().quick_tab);
        a.set_quick_rows(model(rows));
    }

    fn after_setting_change(self: &Rc<Self>, effect: pages::Effect) {
        use pages::Effect::*;
        match effect {
            Nothing => {}
            Theme => self.apply_theme(),
            Audio => {
                self.apply_audio_effects();
                self.sync_eq_ui();
            }
            Picture => self.apply_picture(),
            Browse => self.refresh_browse(),
            Rate(r) => self.set_rate(r),
            Volume => {
                let w = self.ui();
                let max = self.st.borrow().settings.media.audio.max_volume as i32;
                w.global::<AppState>().set_max_volume(max);
                let v = w.global::<AppState>().get_volume().min(max);
                self.set_volume(v);
            }
            Delays => {
                if let Some(p) = self.player() {
                    let st = self.st.borrow();
                    p.set_subtitle_delay_ms(st.settings.media.subtitles.delay_ms as i64);
                    p.set_audio_delay_ms(st.settings.media.audio.audio_delay_ms as i64);
                }
            }
            Queue => {
                self.refresh_queue();
                self.refresh_toolbar();
            }
            Command(c) => self.command(&c),
            Language => self.apply_language(),
            Engine => {
                crate::addons::apply_master_switch(
                    self.st.borrow().settings.system.extensions.lua_enabled,
                );
                self.schedule_apply(crate::options::Apply::Engine);
            }
            Window => self.apply_window_prefs(),
            Opt(key) => {
                use crate::options::Apply;
                match crate::options::find(key).map(|o| o.apply) {
                    Some(Apply::Engine) => self.schedule_apply(Apply::Engine),
                    Some(Apply::Media) => self.schedule_apply(Apply::Media),
                    _ => self.app_option_changed(key),
                }
            }
        }
        self.save_settings();
        self.refresh_settings();
        self.refresh_quick();
    }

    // ------------------------------------------------------- commands
    /// Single entry point for menu items, shortcuts, toolbar tools and buttons.
    pub fn command(self: &Rc<Self>, cmd: &str) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let (head, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
        let skip = |name: &str| -> i64 {
            let c = &self.st.borrow().settings.app.controls;
            match name {
                "veryshort" => c.skip_very_short_s as i64,
                "medium" => c.skip_medium_s as i64,
                "long" => c.skip_long_s as i64,
                _ => c.skip_short_s as i64,
            }
        };
        match head {
            "play-pause" | "play" => self.toggle_play(),
            "stop" => self.stop(),
            "next" => self.next(false),
            "prev" => self.prev(),
            "restart" => {
                let (ended, idx, stopped) = {
                    let st = self.st.borrow();
                    let stopped = self
                        .player()
                        .map(|p| p.state() == PlaybackState::Stopped)
                        .unwrap_or(false);
                    (st.ended, st.index, stopped)
                };
                if let Some(i) = idx {
                    if ended || stopped {
                        self.x.borrow_mut().start_override = Some((0, false));
                        self.play_index(i);
                    } else if let Some(p) = self.player() {
                        p.seek_ms(0);
                        p.play();
                        self.ui().global::<AppState>().set_position(0.0);
                    }
                }
            }
            "jump" => {
                let (sign, name) = arg.split_at(1.min(arg.len()));
                let s = skip(name);
                self.jump(if sign == "-" { -s } else { s });
            }
            "faster" => self.set_rate(a.get_rate() + 0.25),
            "slower" => self.set_rate(a.get_rate() - 0.25),
            "normal-speed" => self.set_rate(1.0),
            "speed" => {
                if let Ok(r) = arg.parse::<f32>() {
                    self.set_rate(r);
                } else {
                    let r = a.get_rate();
                    let next = SPEEDS
                        .iter()
                        .copied()
                        .find(|s| *s > r + 0.01)
                        .unwrap_or(SPEEDS[0]);
                    self.set_rate(next);
                }
            }
            "volume-up" => self.set_volume(
                a.get_volume() + self.st.borrow().settings.media.audio.volume_step as i32,
            ),
            "volume-down" => self.set_volume(
                a.get_volume() - self.st.borrow().settings.media.audio.volume_step as i32,
            ),
            "mute" => {
                if let Some(p) = self.player() {
                    p.toggle_mute();
                    a.set_muted(p.is_muted());
                    self.osd(tr(if p.is_muted() { "Muted" } else { "Unmuted" }));
                }
            }
            "repeat" => {
                let r = {
                    let mut st = self.st.borrow_mut();
                    st.repeat = if arg.is_empty() {
                        (st.repeat + 1) % 3
                    } else {
                        arg.parse().unwrap_or(0)
                    };
                    st.repeat
                };
                self.osd(tr(["Repeat off", "Repeat all", "Repeat one"][r as usize]));
                self.refresh_queue();
                self.refresh_toolbar();
                self.refresh_quick();
            }
            "shuffle" => {
                let s = {
                    let mut st = self.st.borrow_mut();
                    st.shuffle = !st.shuffle;
                    st.settings.media.playback.shuffle = st.shuffle;
                    st.shuffle
                };
                self.osd(tr(if s { "Shuffle on" } else { "Shuffle off" }));
                self.refresh_queue();
                self.refresh_toolbar();
                self.refresh_quick();
            }
            "cycle-audio" => self.cycle_audio_track(),
            "cycle-subtitle" => self.cycle_subtitle_track(),
            "audio-track" | "subtitle-track" => {
                if let (Some(p), Ok(id)) = (self.player(), arg.parse::<i32>()) {
                    if head == "audio-track" {
                        p.set_audio_track(id);
                    } else {
                        p.set_subtitle_track(id);
                    }
                }
            }
            "sub-delay" => {
                let d = {
                    let mut st = self.st.borrow_mut();
                    st.settings.media.subtitles.delay_ms += if arg == "-" { -50 } else { 50 };
                    st.settings.media.subtitles.delay_ms
                };
                self.after_setting_change(pages::Effect::Delays);
                self.osd(tr_fmt("Subtitle delay {} ms", &[&d.to_string()]));
            }
            "aspect" => {
                let i = {
                    let mut st = self.st.borrow_mut();
                    st.aspect_idx = if arg.is_empty() {
                        (st.aspect_idx + 1) % ASPECTS.len()
                    } else {
                        arg.parse().unwrap_or(0)
                    };
                    st.aspect_idx
                };
                self.apply_picture();
                self.osd(tr_fmt("Aspect ratio: {}", &[&tr(ASPECTS[i].0)]));
            }
            "zoom" | "crop" => {
                let i = {
                    let mut st = self.st.borrow_mut();
                    st.crop_idx = if arg.is_empty() {
                        (st.crop_idx + 1) % CROPS.len()
                    } else {
                        arg.parse().unwrap_or(0)
                    };
                    st.crop_idx
                };
                self.apply_picture();
                self.osd(tr_fmt("Crop: {}", &[&tr(CROPS[i].0)]));
            }
            "eq-preset" => {
                let i: usize = arg.parse().unwrap_or(0);
                if let Some(p) = self.player() {
                    let mut st = self.st.borrow_mut();
                    st.eq_preset = i;
                    if i == 0 {
                        st.settings.media.audio.equalizer_enabled = false;
                    } else {
                        let bands = p.set_equalizer_preset((i - 1) as u32);
                        let a = &mut st.settings.media.audio;
                        a.equalizer_enabled = true;
                        a.equalizer_db =
                            std::iter::once(a.equalizer_db.first().copied().unwrap_or(0.0))
                                .chain(bands.iter().map(|b| b * 40.0 - 20.0))
                                .collect();
                        a.equalizer_preset = Some(EQ_PRESETS[i].to_string());
                    }
                }
                self.after_setting_change(pages::Effect::Audio);
                self.osd(tr_fmt("Equalizer: {}", &[&tr(EQ_PRESETS[i])]));
            }
            "snapshot" => self.snapshot(),
            "fullscreen" => {
                self.set_fullscreen(!a.get_fullscreen());
                self.st.borrow_mut().last_pointer = Instant::now();
            }
            "maximize" => {
                let m = !w.window().is_maximized();
                w.window().set_maximized(m);
                a.set_maximized(m);
            }
            "minimize" => self.minimize(),
            "escape" => {
                if a.get_menu_open() {
                    a.set_menu_open(false);
                } else if a.get_dialog() == 8 {
                    self.answer_vlc_dialog(0);
                } else if a.get_dialog() != 0 && a.get_dialog() != 6 {
                    a.set_dialog(0);
                } else if a.get_settings_open() {
                    a.set_settings_open(false);
                } else if a.get_quick_open() {
                    a.set_quick_open(false);
                } else if a.get_queue_open() {
                    a.set_queue_open(false);
                } else if a.get_hide_ui() {
                    a.set_hide_ui(false);
                } else if a.get_fullscreen() {
                    self.command("fullscreen");
                }
            }
            "hide-ui" => {
                a.set_hide_ui(!a.get_hide_ui());
                if a.get_hide_ui() {
                    self.toast("Press Ctrl+\\ or Esc to show the interface");
                }
            }
            "show-time" => {
                let st = self.st.borrow();
                let len = st.length_ms;
                drop(st);
                let t = self.player().map(|p| p.time_ms()).unwrap_or(0);
                self.osd(format!("{} / {}", fmt_clock(t), fmt_clock(len)));
            }
            "show-controls" => {
                self.st.borrow_mut().last_pointer = Instant::now();
                w.global::<Chrome>().set_controls_visible(true);
            }
            "toggle-side-panel" | "sidebar" => {
                if a.get_view() == 1 {
                    a.set_view(0);
                    a.set_side_panel(true);
                } else {
                    a.set_side_panel(!a.get_side_panel());
                }
                self.st.borrow_mut().settings.app.interface.side_panel_open = a.get_side_panel();
                self.refresh_toolbar();
            }
            "library" => {
                a.set_view(if a.get_view() == 0 { 1 } else { 0 });
                if a.get_view() == 1 && self.st.borrow().rows.is_empty() {
                    self.refresh_browse();
                }
            }
            "open-library" => {
                a.set_view(1);
            }
            "open-player" => {
                a.set_view(0);
            }
            "home" => {
                a.set_view(1);
                self.navigate(Location::Home, true);
            }
            "playlists" => {
                a.set_view(1);
                self.navigate(Location::Playlists, true);
            }
            "open-current-folder" => {
                let cur = self.st.borrow().current.clone();
                if let Some(c) = cur.filter(|c| !library::is_url(c)) {
                    if let Some(parent) = Path::new(&c).parent() {
                        let section = if library::media_kind(Path::new(&c)) == MediaKind::Audio {
                            Section::Music
                        } else {
                            Section::Videos
                        };
                        a.set_side_panel(true);
                        self.navigate(
                            Location::Folder {
                                section,
                                path: parent.to_path_buf(),
                            },
                            true,
                        );
                    }
                } else {
                    a.set_side_panel(!a.get_side_panel());
                    if a.get_side_panel() && self.st.borrow().rows.is_empty() {
                        self.refresh_browse();
                    }
                }
            }
            "back" => {
                let prev = {
                    let mut st = self.st.borrow_mut();
                    let p = st.back.pop();
                    if p.is_some() {
                        let cur = st.loc.clone();
                        st.fwd.push(cur);
                    }
                    p
                };
                if let Some(p) = prev {
                    self.navigate(p, false);
                }
            }
            "forward" => {
                let nxt = {
                    let mut st = self.st.borrow_mut();
                    let p = st.fwd.pop();
                    if p.is_some() {
                        let cur = st.loc.clone();
                        st.back.push(cur);
                    }
                    p
                };
                if let Some(p) = nxt {
                    self.navigate(p, false);
                }
            }
            "refresh" => self.refresh_browse(),
            "sort" => {
                let i: usize = arg.parse().unwrap_or(0);
                self.st.borrow_mut().sort = SortKey::ALL[i.min(SortKey::ALL.len() - 1)];
                self.refresh_browse();
            }
            "view-grid" | "view-list" => {
                let grid = head == "view-grid";
                a.set_grid(grid);
                self.st.borrow_mut().settings.app.interface.library_grid = grid;
                self.save_settings();
            }
            "open-files" => background(
                || pickers::pick_media(true),
                |c, files| {
                    c.open_targets(
                        files
                            .into_iter()
                            .map(|f| f.to_string_lossy().to_string())
                            .collect(),
                        false,
                    );
                },
            ),
            "enqueue-files" => background(
                || pickers::pick_media(true),
                |c, files| {
                    c.open_targets(
                        files
                            .into_iter()
                            .map(|f| f.to_string_lossy().to_string())
                            .collect(),
                        true,
                    );
                },
            ),
            "open-folder" => background(
                || pickers::pick_folder("Open Folder"),
                |c, dir| {
                    if let Some(d) = dir {
                        c.open_targets(vec![d.to_string_lossy().to_string()], false);
                    }
                },
            ),
            "add-folder" => background(
                || pickers::pick_folder("Add Folder"),
                |c, dir| {
                    if let Some(d) = dir {
                        let w = c.ui();
                        w.global::<AppState>().set_view(1);
                        c.navigate(
                            Location::Folder {
                                section: Section::Videos,
                                path: d,
                            },
                            true,
                        );
                    }
                },
            ),
            "open-network" => a.set_dialog(1),
            "new-playlist" => {
                self.st.borrow_mut().new_playlist_files.clear();
                a.set_new_playlist_files(model(vec![]));
                a.set_dialog(2);
            }
            "shortcuts" => {
                self.st.borrow_mut().shortcut_edits.clear();
                self.refresh_shortcuts();
                a.set_dialog(3);
            }
            "add-subtitles" => self.open_subtitles_dialog(),
            "load-subtitle" => background(pickers::pick_subtitles, |c, files| {
                if let (Some(p), Some(f)) = (c.player(), files.first()) {
                    let _ = p.add_subtitle_file(&f.to_string_lossy());
                    c.toast(tr_fmt(
                        "Subtitles: {}",
                        &[&library::display_name(&f.to_string_lossy())],
                    ));
                    c.refresh_quick();
                }
            }),
            "customize" => {
                let backup = self.st.borrow().toolbar.clone();
                self.st.borrow_mut().toolbar_backup = Some(backup);
                self.refresh_toolbar();
                a.set_dialog(5);
            }
            "profile" => {
                if let Some(p) = Profile::PRESETS.iter().find(|p| p.label() == arg) {
                    self.st.borrow_mut().toolbar = ToolbarConfig::from_profile(*p);
                    let _ = self
                        .st
                        .borrow()
                        .toolbar
                        .save_to_file(&ToolbarConfig::config_path());
                    self.refresh_toolbar();
                    self.toast(tr_fmt("Profile: {}", &[&tr(p.label())]));
                }
            }
            "settings" | "open-advanced-settings" => {
                a.set_quick_open(false);
                if head == "open-advanced-settings" {
                    let page = [4, 5, 6, 7][self.st.borrow().quick_tab.clamp(0, 3) as usize];
                    self.st.borrow_mut().settings_page = page;
                }
                self.refresh_settings();
                a.set_settings_open(true);
            }
            "settings-page" => {
                {
                    let mut st = self.st.borrow_mut();
                    st.settings_page = arg.parse().unwrap_or(0);
                    st.settings_search.clear();
                    st.settings_advanced = false;
                }
                a.set_settings_search("".into());
                self.refresh_settings();
            }
            "about" => a.set_dialog(7),
            "open-disc" => self.open_disc_dialog(),
            "disc-play" => self.play_disc(),
            "open-capture" => self.open_capture_dialog(),
            "capture-play" => self.play_capture(),
            "convert" => self.open_convert_dialog(Vec::new()),
            "convert-add" => background(pickers::pick_media_files, |c, files| {
                let w = c.ui();
                let f = w.global::<Forms>();
                let mut cur: Vec<SharedString> = f.get_convert_files().iter().collect();
                cur.extend(files.iter().map(|p| ss(p.to_string_lossy())));
                f.set_convert_files(model(cur));
            }),
            "convert-dest" => background(
                || pickers::pick_folder("Destination"),
                |c, dir| {
                    if let Some(d) = dir {
                        c.ui()
                            .global::<Forms>()
                            .set_convert_dest(ss(library::tilde(&d)));
                    }
                },
            ),
            "convert-start" => self.start_convert(),
            "convert-clear" => {
                self.x.borrow_mut().convert.retain(|j| !j.done);
                self.open_convert_dialog(Vec::new());
            }
            "record" => self.toggle_recording(),
            "stream-dialog" => self.open_stream_dialog(),
            "stream-toggle" => self.toggle_streaming(),
            "cast-menu" => {
                self.scan_lan_casts();
                let size = w.window().size().to_logical(w.window().scale_factor());
                self.show_menu("cast", size.width - 300.0, size.height - 120.0);
            }
            "cast" => self.cast_to(arg),
            "vlc-dialog" => self.answer_vlc_dialog(arg.parse().unwrap_or(0)),
            "rescan-plugins" => {
                self.x.borrow_mut().reset_plugins = true;
                self.restart_engine();
            }
            "addon-install" => background(pickers::pick_lua, |c, file| {
                if let Some(f) = file {
                    match crate::addons::install(&f) {
                        Ok(_) => {
                            c.toast(tr("Add-on installed"));
                            c.restart_engine();
                        }
                        Err(e) => c.show_error(&tr("Add-ons"), &e.to_string()),
                    }
                    c.refresh_settings();
                }
            }),
            "addon-folder" => {
                let dir = crate::addons::user_dir();
                let _ = std::fs::create_dir_all(&dir);
                pickers::reveal(&dir);
            }
            "addon-toggle" | "addon-remove" => self.addon_command(head, arg),
            "quit" => {
                self.shutdown();
                let _ = slint::quit_event_loop();
            }
            "pip" => {
                // compact picture-in-picture: small window with only the picture and controls
                if a.get_hide_ui() {
                    a.set_hide_ui(false);
                    if let Some(sz) = self.st.borrow_mut().restore_size.take() {
                        w.window().set_size(sz);
                    }
                } else {
                    self.st.borrow_mut().restore_size = Some(w.window().size());
                    a.set_view(0);
                    a.set_hide_ui(true);
                    w.window().set_size(slint::LogicalSize::new(480.0, 270.0));
                }
            }
            "favorite" => {
                let cur = self.st.borrow().current.clone();
                if let Some(t) = cur {
                    let fav = self.st.borrow_mut().store.toggle_favorite(&t);
                    self.save_store();
                    a.set_favorite(fav);
                    self.toast(if fav {
                        "Added to favorites"
                    } else {
                        "Removed from favorites"
                    });
                    self.refresh_toolbar();
                }
            }
            "properties" => {
                let cur = self.st.borrow().current.clone();
                if let (Some(t), Some(p)) = (cur, self.player()) {
                    let (vw, vh) = p.video_dimensions();
                    let len = self.st.borrow().length_ms;
                    self.toast(format!(
                        "{} · {}x{} · {}",
                        library::display_name(&t),
                        vw,
                        vh,
                        fmt_short(len)
                    ));
                }
            }
            "replay" => {
                let i = self.st.borrow().index;
                if let Some(i) = i {
                    self.play_index(i);
                }
            }
            "toggle-remaining" => {
                let mut st = self.st.borrow_mut();
                st.show_remaining = !st.show_remaining;
            }
            "jump-to-time" => self.command("show-time"),
            "clear-queue" => {
                {
                    let mut st = self.st.borrow_mut();
                    let cur = st.index.and_then(|i| st.queue.get(i).cloned());
                    st.queue = cur.into_iter().collect();
                    st.index = if st.queue.is_empty() { None } else { Some(0) };
                }
                self.refresh_queue();
            }
            "toggle-queue" => a.set_queue_open(!a.get_queue_open()),
            "save-queue" => {
                let items = self.st.borrow().queue.clone();
                let i = self.st.borrow_mut().store.create_playlist("Queue", items);
                self.save_store();
                let name = self.st.borrow().store.playlists[i].name.clone();
                self.toast(tr_fmt("Saved as playlist \"{}\"", &[&name]));
            }
            "mark-all-watched" => {
                let targets: Vec<String> = self
                    .st
                    .borrow()
                    .rows
                    .iter()
                    .filter(|r| r.kind == RowKind::Media)
                    .map(|r| r.target.clone())
                    .collect();
                {
                    let mut st = self.st.borrow_mut();
                    for t in &targets {
                        st.store.set_watched(t, true);
                    }
                }
                self.save_store();
                self.refresh_browse();
            }
            "play-all" | "shuffle-all" | "enqueue-all" => {
                let targets: Vec<String> = self
                    .st
                    .borrow()
                    .rows
                    .iter()
                    .filter(|r| matches!(r.kind, RowKind::Media | RowKind::Stream))
                    .map(|r| r.target.clone())
                    .collect();
                if head == "shuffle-all" && !self.st.borrow().shuffle {
                    self.command("shuffle");
                }
                self.open_targets(targets, head == "enqueue-all");
            }
            "reveal" => {
                let p = match &self.st.borrow().loc {
                    Location::Folder { path, .. } => Some(path.clone()),
                    _ => None,
                };
                if let Some(p) = p {
                    pickers::reveal(&p);
                }
            }
            "playlist-add" => {
                let loc = self.st.borrow().loc.clone();
                if let Location::Playlist(pi) = loc {
                    background(
                        || pickers::pick_media(true),
                        move |c, files| {
                            {
                                let mut st = c.st.borrow_mut();
                                for f in files {
                                    st.store.add_to_playlist(pi, &f.to_string_lossy());
                                }
                            }
                            c.save_store();
                            c.refresh_browse();
                        },
                    );
                }
            }
            "new-playlist-pick" => background(
                || pickers::pick_media(true),
                |c, files| {
                    let list: Vec<String> = files
                        .into_iter()
                        .map(|f| f.to_string_lossy().to_string())
                        .collect();
                    c.st.borrow_mut().new_playlist_files.extend(list);
                    let names: Vec<SharedString> =
                        c.st.borrow()
                            .new_playlist_files
                            .iter()
                            .map(|f| ss(library::display_name(f)))
                            .collect();
                    c.ui()
                        .global::<AppState>()
                        .set_new_playlist_files(model(names));
                },
            ),
            "item" => self.item_command(arg),
            "tool" => {
                let cmd = match Tool::from_id(arg) {
                    Some(Tool::PlaybackSpeed) => "speed",
                    Some(Tool::Forward10) => "jump-secs:10",
                    Some(Tool::Backward10) => "jump-secs:-10",
                    Some(Tool::Forward15) => "jump-secs:15",
                    Some(Tool::Backward15) => "jump-secs:-15",
                    Some(Tool::Stop) => "stop",
                    Some(Tool::Repeat) => "repeat",
                    Some(Tool::Shuffle) => "shuffle",
                    Some(Tool::Snapshot) => "snapshot",
                    Some(Tool::PictureInPicture) => "pip",
                    Some(Tool::Zoom) => "zoom",
                    Some(Tool::RightSidebar) => "sidebar",
                    Some(Tool::QuitVlc) => "quit",
                    Some(Tool::VlcSettings) => "settings",
                    Some(Tool::GoHome) => "home",
                    Some(Tool::Properties) => "properties",
                    Some(Tool::Replay) => "replay",
                    Some(Tool::Favorite) => "favorite",
                    Some(Tool::Fullscreen) => "fullscreen",
                    Some(Tool::Playlists) => "playlists",
                    Some(Tool::Record) => "record",
                    Some(Tool::Stream) => "stream-dialog",
                    Some(Tool::Cast) => "cast-menu",
                    None => "",
                };
                if !cmd.is_empty() {
                    self.command(cmd);
                }
            }
            "jump-secs" => self.jump(arg.parse().unwrap_or(0)),
            "quick-audio" => self.handle_act_int("quick", 2),
            "audio-track-idx" | "subtitle-track-idx" => {
                if let Some(p) = self.player() {
                    let i: usize = arg.parse().unwrap_or(0);
                    if head == "audio-track-idx" {
                        if let Some(t) = p.audio_tracks().get(i) {
                            p.set_audio_track(t.id);
                        }
                    } else if let Some(t) = p.subtitle_tracks().get(i) {
                        p.set_subtitle_track(t.id);
                    }
                }
            }
            "open-recent" => {
                if !arg.is_empty() {
                    self.open_targets(vec![arg.to_string()], false);
                }
            }
            "add-to-playlist" => {
                let sel = self
                    .st
                    .borrow()
                    .menu_item
                    .and_then(|i| self.st.borrow().rows.get(i).cloned());
                let target = sel
                    .map(|r| r.target)
                    .or_else(|| self.st.borrow().current.clone());
                if let Some(t) = target {
                    let name = if arg == "new" {
                        let i = self
                            .st
                            .borrow_mut()
                            .store
                            .create_playlist("New Playlist", vec![t]);
                        self.st.borrow().store.playlists[i].name.clone()
                    } else if let Ok(i) = arg.parse::<usize>() {
                        self.st.borrow_mut().store.add_to_playlist(i, &t);
                        self.st
                            .borrow()
                            .store
                            .playlists
                            .get(i)
                            .map(|p| p.name.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    self.save_store();
                    self.toast(tr_fmt("Added to {}", &[&name]));
                    if matches!(
                        self.st.borrow().loc,
                        Location::Playlists | Location::Playlist(_)
                    ) {
                        self.refresh_browse();
                    }
                }
            }
            "clear-history" => {
                self.st.borrow_mut().store.clear_history();
                self.save_store();
                self.refresh_browse();
                self.toast("History cleared");
            }
            other => {
                if !other.is_empty() && !self.settings_command(head, arg) {
                    eprintln!("ondrane: unhandled command {cmd}");
                }
            }
        }
    }

    /// Commands of the file / folder / playlist context menus.
    fn item_command(self: &Rc<Self>, arg: &str) {
        let Some(i) = self.st.borrow().menu_item else {
            return;
        };
        let Some(row) = self.st.borrow().rows.get(i).cloned() else {
            return;
        };
        match arg {
            "play" => self.item_activate(i),
            "watched" => {
                let w = !self.st.borrow().store.is_watched(&row.target);
                self.st.borrow_mut().store.set_watched(&row.target, w);
                self.save_store();
                self.refresh_browse_marks();
            }
            "favorite" => {
                let fav = self.st.borrow_mut().store.toggle_favorite(&row.target);
                self.save_store();
                self.toast(if fav {
                    "Added to favorites"
                } else {
                    "Removed from favorites"
                });
            }
            "queue" => {
                let targets = match &row.kind {
                    RowKind::Folder(p) => library::list_dir(p, None, false)
                        .into_iter()
                        .filter(|e| !e.is_dir)
                        .map(|e| e.path.to_string_lossy().to_string())
                        .collect(),
                    RowKind::Playlist(pi) => self
                        .st
                        .borrow()
                        .store
                        .playlists
                        .get(*pi)
                        .map(|p| p.items.clone())
                        .unwrap_or_default(),
                    _ => vec![row.target.clone()],
                };
                self.open_targets(targets, true);
            }
            "shuffle" => {
                if !self.st.borrow().shuffle {
                    self.command("shuffle");
                }
                let targets = match &row.kind {
                    RowKind::Folder(p) => library::list_dir(p, None, false)
                        .into_iter()
                        .filter(|e| !e.is_dir)
                        .map(|e| e.path.to_string_lossy().to_string())
                        .collect(),
                    RowKind::Playlist(pi) => self
                        .st
                        .borrow()
                        .store
                        .playlists
                        .get(*pi)
                        .map(|p| p.items.clone())
                        .unwrap_or_default(),
                    _ => vec![row.target.clone()],
                };
                self.open_targets(targets, false);
            }
            "reveal" => match &row.kind {
                RowKind::Folder(p) => pickers::reveal(p),
                _ => pickers::reveal(Path::new(&row.target)),
            },
            "subtitles" => {
                self.st.borrow_mut().sub_scope = 1;
                if let RowKind::Media = row.kind {
                    let media = PathBuf::from(&row.target);
                    let dir = media.parent().map(Path::to_path_buf);
                    let subs = dir
                        .as_deref()
                        .map(subtitles::subtitles_in)
                        .unwrap_or_default();
                    {
                        let mut st = self.st.borrow_mut();
                        st.sub_pairs = subtitles::pair_all(&[media], &subs);
                        st.sub_dir = dir;
                    }
                    self.refresh_subtitle_dialog();
                    self.ui().global::<AppState>().set_dialog(4);
                }
            }
            "info" => {
                let e = self.st.borrow().store.entry(&row.target).cloned();
                let size = std::fs::metadata(&row.target).map(|m| m.len()).unwrap_or(0);
                self.toast(format!(
                    "{} · {:.1} MB{}",
                    row.title,
                    size as f64 / 1_048_576.0,
                    e.filter(|e| e.length_ms > 0)
                        .map(|e| format!(" · {}", fmt_short(e.length_ms)))
                        .unwrap_or_default()
                ));
            }
            "export" => {
                if let RowKind::Playlist(pi) = row.kind {
                    let pl = self.st.borrow().store.playlists.get(pi).cloned();
                    if let Some(pl) = pl {
                        let name = format!("{}.m3u", pl.name);
                        background(
                            move || pickers::pick_save("Export Playlist", &name),
                            move |c, dest| {
                                if let Some(d) = dest {
                                    match std::fs::write(&d, library::to_m3u(&pl.items)) {
                                        Ok(()) => c.toast(tr_fmt(
                                            "Exported to {}",
                                            &[&library::tilde(&d)],
                                        )),
                                        Err(e) => {
                                            c.show_error(&tr("Export failed"), &e.to_string())
                                        }
                                    }
                                }
                            },
                        );
                    }
                }
            }
            "delete-playlist" => {
                if let RowKind::Playlist(pi) = row.kind {
                    self.st.borrow_mut().store.delete_playlist(pi);
                    self.save_store();
                    self.refresh_browse();
                }
            }
            "remove-from-playlist" => {
                if let Location::Playlist(pi) = self.st.borrow().loc.clone() {
                    let mut st = self.st.borrow_mut();
                    if let Some(p) = st.store.playlists.get_mut(pi) {
                        p.items.retain(|t| *t != row.target);
                    }
                    drop(st);
                    self.save_store();
                    self.refresh_browse();
                }
            }
            _ => {}
        }
    }

    fn handle_key(self: &Rc<Self>, raw: &str) {
        if std::env::var_os("ONDRANE_DEBUG_KEYS").is_some() {
            eprintln!("key {:?} -> {:?}", raw, keys::normalize(raw));
        }
        let key = keys::normalize(raw);
        let w = self.ui();
        let a = w.global::<AppState>();
        // Enter / arrows inside the library act on the selection
        if a.get_dialog() == 0 && !a.get_settings_open() && !a.get_menu_open() {
            if key == "Enter" {
                if let Some(i) = self.st.borrow().selected {
                    return self.item_activate(i);
                }
            }
            if key == "Backspace" && (a.get_view() == 1 || a.get_side_panel()) {
                return self.command("back");
            }
        }
        let overrides = self.st.borrow().settings.app.controls.shortcuts.clone();
        if let Some(cmd) = keys::command_for(&key, &overrides) {
            // modal surfaces only accept Escape
            if (a.get_dialog() != 0 || a.get_settings_open()) && cmd != "escape" {
                return;
            }
            self.command(&cmd);
        }
    }

    fn shutdown(&self) {
        #[cfg(target_os = "linux")]
        {
            let mut x = self.x.borrow_mut();
            x.capture_generation = x.capture_generation.wrapping_add(1);
            x.screen_capture = None;
            x.services = None;
        }
        self.save_progress();
        {
            let mut st = self.st.borrow_mut();
            if st.settings.app.privacy.clear_history_on_exit {
                st.store.clear_history();
            }
            st.thumb_maker.cancel();
        }
        self.save_store();
        self.save_settings();
        let _ = self
            .st
            .borrow()
            .toolbar
            .save_to_file(&ToolbarConfig::config_path());
    }

    pub fn first_run_onboarding(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_onboarding_step(0);
        a.set_onboarding_folder(ss(library::tilde(&library::videos_dir())));
        a.set_dialog(6);
    }
}

/// History is on and the item does not match "Never remember items matching".
fn remembers(s: &OndraneSettings, target: &str) -> bool {
    if [
        "fd://",
        "v4l2://",
        "screen://",
        "dshow://",
        "avfoundation://",
    ]
    .iter()
    .any(|s| target.starts_with(s))
    {
        return false;
    }
    if !s.app.privacy.remember_history {
        return false;
    }
    let pattern = s.os("privacy.exclude");
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return true;
    }
    match regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
    {
        Ok(re) => !re.is_match(target),
        Err(_) => !target.to_lowercase().contains(&pattern.to_lowercase()),
    }
}

/// Best-effort desktop colour-scheme detection for the "System" theme.
fn system_prefers_dark() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
        {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).contains("dark");
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(light) = crate::system::registry::get_dword(
            crate::system::registry::HKCU,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "AppsUseLightTheme",
        ) {
            return light == 0;
        }
    }
    true
}

fn find_cover(media: &Path) -> Option<PathBuf> {
    let dir = media.parent()?;
    for name in [
        "cover", "folder", "front", "album", "Cover", "Folder", "Front", "AlbumArt",
    ] {
        for ext in ["jpg", "jpeg", "png"] {
            let p = dir.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

// ======================================================================
// Application wiring
// ======================================================================

pub struct GuiApp {
    window: MainWindow,
    controller: Rc<Controller>,
    _timer: slint::Timer,
}

use slint::winit_030::WinitWindowAccessor as _;

/// Run `f` once the winit window behind `w` exists (it is created after the
/// event loop starts, so settings touching it must wait).
fn when_native_window(w: slint::Weak<MainWindow>, tries: u32, f: impl FnOnce() + 'static) {
    slint::Timer::single_shot(
        Duration::from_millis(if tries == 0 { 0 } else { 25 }),
        move || {
            let ready = w
                .upgrade()
                .and_then(|w| w.window().with_winit_window(|_| ()))
                .is_some();
            if ready || tries >= 200 {
                f();
            } else {
                when_native_window(w, tries + 1, f);
            }
        },
    );
}

impl GuiApp {
    pub fn new(
        engine: EngineReceiver,
        settings: OndraneSettings,
        toolbar: ToolbarConfig,
        initial: Vec<String>,
        initial_subtitle: Option<String>,
        show_onboarding: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let backend = slint::BackendSelector::new().backend_name("winit".into());
        #[cfg(target_os = "linux")]
        let backend = {
            use slint::winit_030::winit::platform::x11::EventLoopBuilderExtX11;
            let mut event_loop = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
            #[cfg(test)]
            {
                use slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland;
                EventLoopBuilderExtWayland::with_any_thread(&mut event_loop, true);
                EventLoopBuilderExtX11::with_any_thread(&mut event_loop, true);
            }
            if settings.ob("ui.xwayland") && std::env::var_os("DISPLAY").is_some() {
                event_loop.with_x11();
            }
            backend.with_winit_event_loop_builder(event_loop)
        };
        backend.select()?;
        slint::set_xdg_app_id(if std::env::var_os("FLATPAK_ID").is_some() {
            "io.github.lib_locale.ondrane"
        } else {
            "ondrane"
        })?;
        let window = MainWindow::new()?;
        crate::i18n::set_language(&settings.app.global.language);
        let thumbs = ThumbnailCache::default();
        let start_grid = settings.app.interface.library_grid;
        let side_panel =
            settings.app.interface.side_panel_open && !settings.app.interface.start_compact;
        let shuffle = settings.media.playback.shuffle;
        let repeat = match settings.media.playback.loop_mode {
            crate::settings::LoopMode::None => 0,
            crate::settings::LoopMode::All => 1,
            crate::settings::LoopMode::Single => 2,
        };
        let state = State {
            toolbar,
            store: LibraryStore::load_default(),
            queue: Vec::new(),
            index: None,
            current: None,
            current_playlist: String::new(),
            repeat,
            shuffle,
            ended: false,
            length_ms: 0,
            show_remaining: false,
            meta_loaded: false,
            aspect_idx: 0,
            crop_idx: 0,
            eq_preset: 0,
            loc: Location::Folder {
                section: Section::Videos,
                path: library::videos_dir(),
            },
            back: Vec::new(),
            fwd: Vec::new(),
            rows: Vec::new(),
            selected: None,
            sort: SortKey::UnwatchedFirst,
            search: String::new(),
            posters: HashMap::new(),
            thumbs,
            thumb_maker: ThumbnailMaker::default(),
            menu_item: None,
            toast_until: None,
            last_save: Instant::now(),
            last_pointer: Instant::now(),
            sub_pairs: Vec::new(),
            sub_dir: None,
            sub_scope: 0,
            new_playlist_files: Vec::new(),
            toolbar_backup: None,
            shortcut_edits: Default::default(),
            settings_page: 0,
            settings_search: String::new(),
            settings_advanced: false,
            quick_tab: 0,
            restore_size: None,
            dropped: Vec::new(),
            settings,
        };
        let items = Rc::new(VecModel::<MediaItem>::default());
        let controller = Rc::new(Controller {
            window: window.as_weak(),
            libvlc: std::cell::OnceCell::new(),
            engine: RefCell::new(None),
            engine_rx: RefCell::new(Some(engine)),
            startup_media: RefCell::new(Some((initial, initial_subtitle))),
            player: RefCell::new(None),
            st: RefCell::new(state),
            items: items.clone(),
            posters: std::cell::OnceCell::new(),
            x: RefCell::new(extras::Extras::default()),
        });
        APP.with(|a| *a.borrow_mut() = Some(controller.clone()));

        let a = window.global::<AppState>();
        a.set_items(ModelRc::from(items));

        a.set_grid(start_grid);
        a.set_side_panel(side_panel);
        a.set_sort_options(model(
            SortKey::ALL.iter().map(|k| ss(tr(k.label()))).collect(),
        ));
        controller.apply_theme();
        controller.apply_framing();
        controller.sync_eq_ui();

        {
            let st = controller.st.borrow();
            let vol = if st.settings.media.audio.remember_last_volume {
                st.settings.media.audio.last_volume
            } else {
                st.settings.media.audio.default_volume
            };
            let max = st.settings.media.audio.max_volume as i32;
            drop(st);
            a.set_volume((vol as i32).min(max));
            a.set_max_volume(max);
        }

        Self::wire_callbacks(&window, &controller);

        controller.refresh_toolbar();
        controller.refresh_queue();
        controller.refresh_browse();
        controller.refresh_quick();

        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(200),
            || with_app(|c| c.tick()),
        );

        if controller
            .st
            .borrow()
            .settings
            .media
            .video
            .fullscreen_on_launch
        {
            controller.command("fullscreen");
        }
        if show_onboarding
            || !controller
                .st
                .borrow()
                .settings
                .app
                .global
                .onboarding_completed
        {
            controller.first_run_onboarding();
        }
        // an engine that is already loaded is picked up right away
        controller.poll_engine_load();
        Ok(Self {
            window,
            controller,
            _timer: timer,
        })
    }

    fn wire_callbacks(window: &MainWindow, c: &Rc<Controller>) {
        let a = window.global::<AppState>();
        macro_rules! cb {
            ($method:ident, |$ctl:ident $(, $arg:ident)*| $body:expr) => {{
                a.$method(move |$($arg),*| {
                    with_app(|$ctl| $body);
                });
            }};
        }
        let _ = c;
        cb!(on_act, |c, id| {
            let id = id.to_string();
            c.handle_act(&id)
        });
        cb!(on_act_int, |c, id, v| c.handle_act_int(id.as_str(), v));
        cb!(on_act_float, |c, id, v| c.handle_act_float(id.as_str(), v));
        cb!(on_act_text, |c, id, v| c
            .handle_act_text(id.as_str(), v.as_str()));
        cb!(on_menu_request, |c, id, x, y| {
            c.st.borrow_mut().menu_item = None;
            c.show_menu(id.as_str(), x, y)
        });
        cb!(on_submenu_request, |c, id, x, y| c.show_submenu(
            id.as_str(),
            x,
            y
        ));
        cb!(on_menu_pick, |c, id| {
            c.command(id.as_str());
            c.refocus()
        });
        cb!(on_setting_bool, |c, key, v| {
            let e = pages::set_bool(&mut c.st.borrow_mut(), key.as_str(), v);
            c.after_setting_change(e)
        });
        cb!(on_setting_int, |c, key, v| {
            let e = pages::set_int(&mut c.st.borrow_mut(), key.as_str(), v);
            c.after_setting_change(e)
        });
        cb!(on_setting_float, |c, key, v| {
            let e = pages::set_float(&mut c.st.borrow_mut(), key.as_str(), v);
            c.after_setting_change(e)
        });
        cb!(on_setting_text, |c, key, v| c
            .handle_setting_text(key.as_str(), v.as_str()));
        cb!(on_seek_hover, |c, pos, x| c.seek_hover(pos, x));
        cb!(on_item_activate, |c, i| c.item_activate(i as usize));
        cb!(on_item_select, |c, i| c.select_item(Some(i as usize)));
        cb!(on_item_menu, |c, i, x, y| {
            c.select_item(Some(i as usize));
            c.st.borrow_mut().menu_item = Some(i as usize);
            let kind = c.st.borrow().rows.get(i as usize).map(|r| r.kind.clone());
            let id = match kind {
                Some(RowKind::Folder(_)) => "item-folder",
                Some(RowKind::Playlist(_)) => "item-playlist",
                Some(RowKind::Action(_)) | None => return,
                _ => "item-file",
            };
            let (entries, width) = menus::build(c, id);
            let w = c.ui();
            let a = w.global::<AppState>();
            a.set_menu_entries(model(entries));
            a.set_menu_width(width);
            a.set_menu_x(x);
            a.set_menu_y(y);
            a.set_submenu_open(false);
            a.set_menu_open(true)
        });
        cb!(on_home_activate, |c, section, i| c
            .home_activate(section.as_str(), i as usize));
        cb!(on_queue_activate, |c, i| c.play_index(i as usize));
    }

    pub fn run(self) -> Result<(), slint::PlatformError> {
        // Release D-Bus services before their executor's thread-local destructor,
        // including when a renderer or callback unwinds.
        struct Cleanup(Rc<Controller>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.0.shutdown();
                APP.with(|a| {
                    a.borrow_mut().take();
                });
            }
        }
        let _cleanup = Cleanup(self.controller.clone());
        let c = self.controller.clone();
        if !c.restore_geometry() {
            self.window
                .window()
                .set_size(slint::LogicalSize::new(1440.0, 860.0));
        }
        self.window.show()?;
        // winit-dependent setup once the native window exists
        let c2 = c.clone();
        when_native_window(c.window.clone(), 0, move || {
            c2.fit_to_screen();
            c2.install_window_hooks();
            c2.start_services();
            if c2.start_minimized() {
                c2.minimize();
            }
            c2.check_updates();
        });
        if c.st.borrow().settings.ob("lib.single") {
            window::listen_single_instance();
        }
        // the window may be hidden to the tray: quit explicitly
        let r = slint::run_event_loop_until_quit();
        self.controller.save_geometry();
        r
    }
}

impl Controller {
    fn refocus(&self) {
        self.ui().invoke_refocus();
    }

    fn handle_act(self: &Rc<Self>, id: &str) {
        let w = self.ui();
        let a = w.global::<AppState>();
        match id {
            "pointer-moved" => {
                self.st.borrow_mut().last_pointer = Instant::now();
                if !w.global::<Chrome>().get_controls_visible() {
                    w.global::<Chrome>().set_controls_visible(true);
                }
            }
            "canvas-click" => {
                self.refocus();
                if a.get_quick_open() || a.get_queue_open() {
                    a.set_quick_open(false);
                    a.set_queue_open(false);
                } else if self
                    .st
                    .borrow()
                    .settings
                    .app
                    .controls
                    .single_click_play_pause
                    && a.get_has_media()
                {
                    self.toggle_play();
                }
            }
            "canvas-double-click" => {
                use crate::settings::DoubleClickAction::*;
                let action = self.st.borrow().settings.app.controls.double_click_action;
                match action {
                    ToggleFullscreen => {
                        // undo the play/pause of the first click
                        if self
                            .st
                            .borrow()
                            .settings
                            .app
                            .controls
                            .single_click_play_pause
                            && a.get_has_media()
                        {
                            self.toggle_play();
                        }
                        self.command("fullscreen")
                    }
                    TogglePlayPause => self.toggle_play(),
                    None => {}
                }
            }
            "canvas-middle-click" => {
                use crate::settings::MiddleClickAction::*;
                let action = self.st.borrow().settings.app.controls.middle_click_action;
                match action {
                    ToggleMute => self.command("mute"),
                    PictureInPicture => self.command("pip"),
                    None => {}
                }
            }
            "settings-advanced" => {
                let adv = !self.st.borrow().settings_advanced;
                self.st.borrow_mut().settings_advanced = adv;
                self.refresh_settings();
            }
            "open-network" => a.set_dialog(1),
            "shortcuts-save" => {
                {
                    let mut st = self.st.borrow_mut();
                    let edits = std::mem::take(&mut st.shortcut_edits);
                    st.settings.app.controls.shortcuts.extend(edits);
                }
                self.save_settings();
                a.set_dialog(0);
                self.refocus();
            }
            "shortcuts-cancel" => {
                self.st.borrow_mut().shortcut_edits.clear();
                a.set_dialog(0);
                self.refocus();
            }
            "subtitles-apply" => {
                let pairs = self.st.borrow().sub_pairs.clone();
                let cur = self.st.borrow().current.clone();
                let mut n = 0;
                for (m, s) in &pairs {
                    if let Some(s) = s {
                        n += 1;
                        if cur.as_deref() == Some(m.to_string_lossy().as_ref()) {
                            if let Some(p) = self.player() {
                                let _ = p.add_subtitle_file(&s.to_string_lossy());
                            }
                        }
                    }
                }
                a.set_dialog(0);
                self.toast(if n == 1 {
                    tr("1 subtitle file attached")
                } else {
                    tr_fmt("{} subtitle files attached", &[&n.to_string()])
                });
                self.refocus();
            }
            "subtitles-auto" => self.open_subtitles_dialog(),
            "subtitles-browse" => background(pickers::pick_subtitles, |c, files| {
                if files.is_empty() {
                    return;
                }
                {
                    let mut st = c.st.borrow_mut();
                    let media: Vec<PathBuf> = st.sub_pairs.iter().map(|(m, _)| m.clone()).collect();
                    let mut paired = subtitles::pair_all(&media, &files);
                    // a single file for a single video always pairs
                    if paired.len() == 1 && files.len() == 1 && paired[0].1.is_none() {
                        paired[0].1 = Some(files[0].clone());
                    }
                    for (i, (_, s)) in paired.into_iter().enumerate() {
                        if s.is_some() {
                            st.sub_pairs[i].1 = s;
                        }
                    }
                    st.sub_dir = files[0].parent().map(Path::to_path_buf);
                }
                c.refresh_subtitle_dialog();
            }),
            "customize-save" => {
                self.st.borrow_mut().toolbar_backup = None;
                let _ = self
                    .st
                    .borrow()
                    .toolbar
                    .save_to_file(&ToolbarConfig::config_path());
                a.set_dialog(0);
                self.refocus();
            }
            "customize-cancel" => {
                if let Some(b) = self.st.borrow_mut().toolbar_backup.take() {
                    self.st.borrow_mut().toolbar = b;
                }
                self.refresh_toolbar();
                a.set_dialog(0);
                self.refocus();
            }
            "onboarding-browse" => background(
                || pickers::pick_folder("Choose a Media Folder"),
                |c, d| {
                    if let Some(d) = d {
                        c.ui()
                            .global::<AppState>()
                            .set_onboarding_folder(ss(library::tilde(&d)));
                    }
                },
            ),
            "onboarding-finish" => {
                let folder = library::expand_tilde(a.get_onboarding_folder().as_str());
                {
                    let mut st = self.st.borrow_mut();
                    st.settings.app.global.onboarding_completed = true;
                    if folder.is_dir()
                        && !st.settings.media.library.watched_folders.contains(&folder)
                    {
                        st.settings
                            .media
                            .library
                            .watched_folders
                            .insert(0, folder.clone());
                    }
                }
                self.save_settings();
                a.set_dialog(0);
                if folder.is_dir() {
                    a.set_view(1);
                    self.navigate(
                        Location::Folder {
                            section: Section::Videos,
                            path: folder,
                        },
                        true,
                    );
                }
                self.refocus();
            }
            other => self.command(other),
        }
    }

    fn handle_act_int(self: &Rc<Self>, id: &str, v: i32) {
        let w = self.ui();
        let a = w.global::<AppState>();
        match id {
            "nav" => self.nav_section(v),
            "crumb" => self.crumb(v),
            "volume" => self.set_volume(v),
            "volume-step" => {
                let reverse = self.st.borrow().settings.app.controls.reverse_scroll;
                let step = self.st.borrow().settings.media.audio.volume_step as i32;
                let dir = if reverse { -v } else { v };
                self.set_volume(a.get_volume() + dir * step);
                self.osd(tr_fmt("Volume {}%", &[&a.get_volume().to_string()]));
            }
            "jump" => self.jump(v as i64),
            "wheel-v" => self.wheel(v, false),
            "wheel-h" => self.wheel(v, true),
            "quick" => {
                let tab = v;
                if a.get_quick_open()
                    && (self.st.borrow().quick_tab == tab
                        || (tab == 0 && self.st.borrow().quick_tab < 2))
                {
                    a.set_quick_open(false);
                } else {
                    self.st.borrow_mut().quick_tab = tab;
                    self.refresh_quick();
                    a.set_quick_open(true);
                    a.set_queue_open(false);
                }
            }
            "quick-tab" => {
                self.st.borrow_mut().quick_tab = v;
                self.refresh_quick();
            }
            "settings-page" => self.command(&format!("settings-page:{v}")),
            "queue-remove" => {
                {
                    let mut st = self.st.borrow_mut();
                    let i = v as usize;
                    if i < st.queue.len() && st.index != Some(i) {
                        st.queue.remove(i);
                        if let Some(cur) = st.index {
                            if i < cur {
                                st.index = Some(cur - 1);
                            }
                        }
                    }
                }
                self.refresh_queue();
            }
            "customize-toggle" => {
                if let Some(t) = Tool::ALL.get(v as usize) {
                    self.st.borrow_mut().toolbar.toggle(*t);
                    self.refresh_toolbar();
                }
            }
            "subtitles-scope" => {
                self.st.borrow_mut().sub_scope = v;
                self.open_subtitles_dialog();
            }
            _ => {}
        }
    }

    fn handle_act_float(self: &Rc<Self>, id: &str, v: f32) {
        if id == "seek" {
            if let Some(p) = self.player() {
                let len = p.length_ms();
                if len > 0 {
                    p.seek_ms((v.clamp(0.0, 1.0) as f64 * len as f64) as i64);
                } else {
                    p.seek_position(v);
                }
                let w = self.ui();
                w.global::<AppState>().set_position(v);
                if self.st.borrow().ended {
                    let i = self.st.borrow().index;
                    if let Some(i) = i {
                        let target_ms = if len > 0 {
                            (v.clamp(0.0, 1.0) as f64 * len as f64) as i64
                        } else {
                            0
                        };
                        self.x.borrow_mut().start_override = Some((target_ms, false));
                        self.play_index(i);
                    }
                }
            }
        }
    }

    fn handle_act_text(self: &Rc<Self>, id: &str, v: &str) {
        let w = self.ui();
        let a = w.global::<AppState>();
        if id == "key" {
            return self.handle_key(v);
        }
        if id == "resize" {
            use slint::winit_030::{winit::window::ResizeDirection as D, WinitWindowAccessor};
            let dir = match v {
                "n" => D::North,
                "s" => D::South,
                "e" => D::East,
                "w" => D::West,
                "ne" => D::NorthEast,
                "nw" => D::NorthWest,
                "se" => D::SouthEast,
                _ => D::SouthWest,
            };
            w.window().with_winit_window(|ww| {
                let _ = ww.drag_resize_window(dir);
            });
            return;
        }
        if let Some(idx) = id.strip_prefix("shortcut-set:") {
            // index into the displayed rows (headers included)
            let i: usize = idx.parse().unwrap_or(usize::MAX);
            let mut n = 0usize;
            let mut section = "";
            for (sec, cmd, _, _) in keys::ACTIONS {
                if *sec != section {
                    section = sec;
                    n += 1;
                }
                if n == i {
                    let key = keys::normalize(v);
                    if !key.is_empty() && key != "Esc" {
                        self.st
                            .borrow_mut()
                            .shortcut_edits
                            .insert(cmd.to_string(), key);
                    }
                    break;
                }
                n += 1;
            }
            self.refresh_shortcuts();
            return;
        }
        match id {
            "search" => {
                self.st.borrow_mut().search = v.to_string();
                a.set_search(ss(v));
                self.refresh_browse();
            }
            "settings-search" => {
                self.st.borrow_mut().settings_search = v.to_string();
                self.refresh_settings();
            }
            "open-network" => {
                let (urls, name) = v.split_once("\n#name=").unwrap_or((v, ""));
                let list: Vec<String> = urls
                    .split(|c: char| c.is_whitespace())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
                if list.is_empty() {
                    return;
                }
                if !name.trim().is_empty() {
                    self.st
                        .borrow_mut()
                        .store
                        .create_playlist(name, list.clone());
                    self.save_store();
                }
                a.set_dialog(0);
                self.open_targets(list, false);
                if !name.trim().is_empty() {
                    self.st.borrow_mut().current_playlist = name.trim().to_string();
                    a.set_media_playlist(ss(name.trim()));
                }
                self.refocus();
            }
            "create-playlist" => {
                let files = std::mem::take(&mut self.st.borrow_mut().new_playlist_files);
                let i = self.st.borrow_mut().store.create_playlist(v, files);
                self.save_store();
                a.set_dialog(0);
                let name = self.st.borrow().store.playlists[i].name.clone();
                self.toast(tr_fmt("Playlist \"{}\" created", &[&name]));
                if matches!(self.st.borrow().loc, Location::Playlists) {
                    self.refresh_browse();
                }
                self.refocus();
            }
            _ => {}
        }
    }

    fn handle_setting_text(self: &Rc<Self>, key: &str, v: &str) {
        if let Some(base) = key.strip_suffix(".browse") {
            let base = base.to_string();
            let file = base == "ui.skin-file" || base == "log.file";
            let save = base == "log.file";
            background(
                move || {
                    if save {
                        pickers::pick_save("Log file", "ondrane.log")
                    } else if file {
                        pickers::pick_any_file("Choose a file")
                    } else {
                        pickers::pick_folder("Choose a folder")
                    }
                },
                move |c, d| {
                    if let Some(d) = d {
                        let e =
                            pages::set_text(&mut c.st.borrow_mut(), &base, &d.to_string_lossy());
                        c.after_setting_change(e);
                    }
                },
            );
            return;
        }
        let e = pages::set_text(&mut self.st.borrow_mut(), key, v);
        self.after_setting_change(e);
    }

    fn seek_hover(&self, pos: f32, x: f32) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let len = self.st.borrow().length_ms;
        if len <= 0 {
            return;
        }
        let ms = (pos as f64 * len as f64) as i64;
        a.set_preview_time(ss(fmt_clock(ms)));
        a.set_preview_x(x);
        let frame = {
            let st = self.st.borrow();
            if st.settings.media.playback.seek_thumbnails {
                st.thumbs.frame_at(ms as u64)
            } else {
                None
            }
        };
        if let Some((fw, fh, data)) = frame {
            a.set_preview(Image::from_rgba8(
                SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&data, fw, fh),
            ));
            a.set_has_preview(true);
        } else {
            a.set_has_preview(false);
        }
        a.set_preview_visible(true);
    }

    fn home_activate(self: &Rc<Self>, section: &str, i: usize) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let list = match section {
            "continue" => a.get_continue_items(),
            "listen" => a.get_listen_items(),
            _ => a.get_folder_items(),
        };
        let Some(item) = list.row_data(i) else { return };
        let target = item.path.to_string();
        if section == "folders" {
            let path = PathBuf::from(&target);
            let section = if path.starts_with(library::music_dir())
                && !path.starts_with(library::videos_dir())
            {
                Section::Music
            } else {
                Section::Videos
            };
            self.navigate(Location::Folder { section, path }, true);
        } else {
            self.open_targets(vec![target], false);
        }
    }
}
