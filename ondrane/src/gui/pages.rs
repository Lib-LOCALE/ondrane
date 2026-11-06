//! Settings pages ("Settings - *" frames) and the quick-settings drawer tabs.
//! Every row of the Figma pages is bound to a real setting: typed settings,
//! or the `extra` options of `crate::options` (libVLC instance arguments,
//! per-media options or application behaviour). `set_*` apply a change and
//! report which subsystem must be refreshed.

use super::*;
use crate::options::{self as opts, Def};
use crate::settings::{
    DeinterlaceMode, DoubleClickAction, MiddleClickAction, SkipLoopFilter, SnapshotFormat,
};

pub const PAGE_TITLES: [&str; 13] = [
    "Global Settings",
    "Interface Settings",
    "Privacy Settings",
    "Mouse, Keyboard & Remote Controls",
    "Playback Preferences",
    "Video Defaults",
    "Audio Defaults",
    "Subtitle Defaults",
    "Playlists, Folders & Files",
    "Network Connections & Security",
    "Performance Settings",
    "Recording, Casting & Streaming Out",
    "Add-ons & Scripts",
];

pub enum Effect {
    Nothing,
    Theme,
    Audio,
    Picture,
    Browse,
    Rate(f32),
    Volume,
    Delays,
    Queue,
    Command(String),
    /// Interface language changed.
    Language,
    /// An `extra` option changed (applied according to its `Apply`).
    Opt(&'static str),
    /// A typed setting that is a libVLC instance option changed.
    Engine,
    /// Window level / decorations / wallpaper changed.
    Window,
}

/// Lists computed by the controller (devices, screens, add-ons…).
#[derive(Default)]
pub struct Ctx {
    pub audio_devices: Vec<(String, String)>,
    pub screens: Vec<String>,
    pub drives: Vec<String>,
    pub addons: Vec<crate::addons::Addon>,
    pub remote_url: Option<String>,
}

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| ss(tr(s))).collect())
}

fn base(kind: i32, key: &str, label: &str) -> SettingRow {
    SettingRow {
        kind,
        key: ss(key),
        label: ss(tr(label)),
        ..Default::default()
    }
}

fn header(label: &str) -> SettingRow {
    base(0, "", label)
}

fn toggle(key: &str, label: &str, on: bool) -> SettingRow {
    SettingRow {
        on,
        ..base(1, key, label)
    }
}

fn toggle_info(key: &str, label: &str, on: bool, info: &str) -> SettingRow {
    SettingRow {
        info: ss(tr(info)),
        ..toggle(key, label, on)
    }
}

fn dropdown(key: &str, label: &str, options: &[&str], index: usize) -> SettingRow {
    SettingRow {
        options: strings(options),
        index: index as i32,
        ..base(2, key, label)
    }
}

fn dropdown_owned(key: &str, label: &str, options: Vec<String>, index: usize) -> SettingRow {
    SettingRow {
        options: model(options.iter().map(ss).collect()),
        index: index as i32,
        ..base(2, key, label)
    }
}

fn segmented(key: &str, label: &str, options: &[&str], index: usize) -> SettingRow {
    SettingRow {
        options: strings(options),
        index: index as i32,
        ..base(3, key, label)
    }
}

fn slider(
    key: &str,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    ticks: &[&str],
    text: String,
) -> SettingRow {
    SettingRow {
        value,
        minimum: min,
        maximum: max,
        ticks: strings(ticks),
        text: ss(text),
        ..base(4, key, label)
    }
}

fn text(key: &str, label: &str, value: &str, placeholder: &str) -> SettingRow {
    SettingRow {
        text: ss(value),
        button: ss(tr(placeholder)),
        ..base(5, key, label)
    }
}

fn button(key: &str, label: &str, caption: &str) -> SettingRow {
    SettingRow {
        button: ss(tr(caption)),
        ..base(6, key, label)
    }
}

fn swatches(key: &str, label: &str, index: usize) -> SettingRow {
    SettingRow {
        index: index as i32,
        ..base(7, key, label)
    }
}

fn path_row(key: &str, label: &str, value: &Path) -> SettingRow {
    SettingRow {
        text: ss(library::tilde(value)),
        button: ss(tr("Browse")),
        ..base(8, key, label)
    }
}

fn info(label: &str) -> SettingRow {
    base(10, "", label)
}

fn info_owned(label: String) -> SettingRow {
    SettingRow {
        kind: 10,
        label: ss(label),
        ..Default::default()
    }
}

fn equalizer() -> SettingRow {
    base(9, "eq", "")
}

// ------------------------------------------- rows bound to `extra` options

fn ot(s: &OndraneSettings, key: &str, label: &str) -> SettingRow {
    toggle(key, label, s.ob(key))
}

fn od(s: &OndraneSettings, key: &str, label: &str, options: &[&str]) -> SettingRow {
    dropdown(key, label, options, s.oi(key).max(0) as usize)
}

fn odl<T>(s: &OndraneSettings, key: &str, label: &str, list: &[(&str, T)]) -> SettingRow {
    let labels: Vec<&str> = list.iter().map(|x| x.0).collect();
    od(s, key, label, &labels)
}

fn otext(s: &OndraneSettings, key: &str, label: &str, placeholder: &str) -> SettingRow {
    let v = match opts::find(key).map(|o| o.def) {
        Some(Def::I(_)) => s.oi(key).to_string(),
        _ => s.os(key),
    };
    text(key, label, &v, placeholder)
}

fn ocolor(s: &OndraneSettings, key: &str, label: &str) -> SettingRow {
    let v = s.os(key);
    SettingRow {
        kind: 11,
        color: parse_hex_color(&v).unwrap_or_default(),
        ..text(key, label, &v, "#FFFFFF")
    }
}

fn opass(s: &OndraneSettings, key: &str, label: &str) -> SettingRow {
    SettingRow {
        kind: 12,
        ..text(key, label, &s.os(key), "")
    }
}

fn secs_slider(s: &OndraneSettings, key: &str, label: &str, min: f32, max: f32) -> SettingRow {
    let v = s.of(key) as f32;
    let lo = format!("{min} s");
    let hi = format!("{max} s");
    slider(key, label, v, min, max, &[&lo, &hi], format!("{v:.1} s"))
}

fn theme_index(st: &State) -> usize {
    match st.settings.app.global.theme_mode {
        ThemeMode::Dark => 0,
        ThemeMode::Light => 1,
        _ => 2,
    }
}

fn eq_preset_index(st: &State) -> usize {
    st.settings
        .media
        .audio
        .equalizer_preset
        .as_ref()
        .and_then(|n| EQ_PRESETS.iter().position(|p| p == n))
        .filter(|_| st.settings.media.audio.equalizer_enabled)
        .unwrap_or(0)
}

fn picture_rows(st: &State, out: &mut Vec<SettingRow>) {
    let p = &st.settings.media.video.picture_adjust;
    out.push(slider(
        "pic.hue",
        "Hue",
        p.hue,
        -180.0,
        180.0,
        &[],
        format!("{:.0}°", p.hue),
    ));
    out.push(slider(
        "pic.brightness",
        "Brightness",
        p.brightness,
        0.0,
        2.0,
        &[],
        format!("{:.2}", p.brightness),
    ));
    out.push(slider(
        "pic.contrast",
        "Contrast",
        p.contrast,
        0.0,
        2.0,
        &[],
        format!("{:.2}", p.contrast),
    ));
    out.push(slider(
        "pic.saturation",
        "Saturation",
        p.saturation,
        0.0,
        3.0,
        &[],
        format!("{:.2}", p.saturation),
    ));
    out.push(slider(
        "pic.gamma",
        "Gamma",
        p.gamma,
        0.01,
        10.0,
        &[],
        format!("{:.2}", p.gamma),
    ));
    out.push(button("pic.reset", "Reset picture", "Reset"));
}

pub fn page(n: i32, st: &State, advanced: bool, ctx: &Ctx) -> Vec<SettingRow> {
    let s = &st.settings;
    let mut r = Vec::new();
    match n {
        0 => {
            let g = &s.app.global;
            r.push(header("Appearance"));
            r.push(dropdown_owned(
                "app.language",
                "Interface language",
                crate::i18n::choices(),
                crate::i18n::choice_index(&g.language),
            ));
            use crate::themes::{CatppuccinFlavor, ColorScheme};
            r.push(dropdown(
                "app.palette",
                "Color scheme",
                &ColorScheme::NAMES,
                ColorScheme::ALL
                    .iter()
                    .position(|p| *p == g.color_scheme)
                    .unwrap_or(0),
            ));
            if g.color_scheme == ColorScheme::Catppuccin {
                r.push(dropdown(
                    "app.catppuccin",
                    "Catppuccin flavor",
                    &CatppuccinFlavor::NAMES,
                    CatppuccinFlavor::ALL
                        .iter()
                        .position(|p| *p == g.catppuccin_flavor)
                        .unwrap_or(0),
                ));
            }
            r.push(info(
                "Changes apply immediately. Your choice is saved automatically.",
            ));
            if g.color_scheme == ColorScheme::Ondrane {
                r.push(segmented(
                    "app.theme",
                    "Theme",
                    &["Dark", "Light", "System"],
                    theme_index(st),
                ));
                let accent = ACCENTS
                    .iter()
                    .position(|a| a.eq_ignore_ascii_case(&g.accent_color))
                    .unwrap_or(usize::MAX);
                r.push(swatches("app.accent", "Accent color", accent));
            }
            r.push(header("Startup"));
            r.push(toggle(
                "app.dialogs",
                "Show confirmation and error dialogs",
                g.show_error_dialogs,
            ));
            r.push(toggle(
                "app.updates",
                "Check for updates automatically",
                g.check_updates,
            ));
            r.push(toggle_info(
                "app.welcome",
                "Show the welcome screen on next start",
                !g.onboarding_completed,
                "Shows the onboarding (media folder, theme and colour) the next time VLC starts.",
            ));
            r.push(header("Diagnostics"));
            r.push(toggle_info(
                "app.devmode",
                "Developer mode",
                g.developer_mode,
                "Verbose libVLC messages on the console and the advanced options shown by default.",
            ));
            r.push(toggle("app.savelog", "Save a log", g.save_log));
            if g.save_log {
                r.push(path_row("log.file", "Log file", &opts::log_file(s)));
                r.push(dropdown(
                    "app.logdetail",
                    "Log details",
                    &["Errors only", "Errors and warnings", "Debug"],
                    g.log_detail.min(2) as usize,
                ));
            }
            r.push(info_owned(tr_fmt(
                "Settings file: {}",
                &[&library::tilde(&OndraneSettings::config_path())],
            )));
            if advanced {
                r.push(button("app.reset", "Reset all settings", "Reset"));
            }
        }
        1 => {
            let i = &s.app.interface;
            r.push(header("Skin"));
            r.push(toggle_info(
                "ui.skin",
                "Use a custom skin",
                s.ob("ui.skin"),
                "A JSON colour file; export the current theme to start one.",
            ));
            if s.ob("ui.skin") {
                r.push(SettingRow {
                    text: ss(s.os("ui.skin-file")),
                    button: ss(tr("Browse")),
                    ..base(8, "ui.skin-file", "Skin file")
                });
            }
            r.push(button(
                "ui.skin-export",
                "Export the current theme as a skin",
                "Export",
            ));
            r.push(header("Window"));
            r.push(toggle_info(
                "ui.embedded",
                "Play video inside main window",
                i.embedded_video,
                "Off: libVLC shows the video in its own window.",
            ));
            r.push(od(
                s,
                "ui.resize",
                "Resize window to fit video",
                &["Never", "When the first video starts", "Always"],
            ));
            r.push(ot(
                s,
                "ui.remember-geometry",
                "Remember window size and position",
            ));
            r.push(toggle(
                "ui.ontop",
                "Keep window above other windows",
                i.always_on_top,
            ));
            r.push(ot(s, "ui.decorations", "Show window decorations"));
            if cfg!(target_os = "linux") {
                r.push(toggle_info(
                    "ui.xwayland",
                    "Use XWayland on Wayland desktops",
                    s.ob("ui.xwayland"),
                    "Needed to drop files on the window under Wayland. Applies on next start.",
                ));
            }
            r.push(toggle(
                "ui.title",
                "Show the file name in the title bar",
                i.show_filename_in_title,
            ));
            r.push(dropdown_owned(
                "ui.fs-screen",
                "Open fullscreen on",
                ctx.screens.clone(),
                (s.oi("ui.fs-screen").max(0) as usize).min(ctx.screens.len().saturating_sub(1)),
            ));
            r.push(header("Playback Controls"));
            r.push(toggle(
                "ui.compact",
                "Start in compact mode",
                i.start_compact,
            ));
            r.push(dropdown(
                "ui.autohide",
                "Auto-hide the controls",
                &["Never", "In fullscreen only", "Always"],
                i.auto_hide_controls as usize,
            ));
            let secs = i.controls_hide_delay_ms as f32 / 1000.0;
            r.push(slider(
                "ui.hide-after",
                "Hide after",
                secs,
                0.5,
                10.0,
                &["0 s", "5 s", "10s"],
                format!("{:.1} s", secs),
            ));
            r.push(ot(s, "ui.fs-controls", "Show controls in fullscreen"));
            r.push(header("Notifications and System Tray"));
            r.push(ot(s, "ui.tray", "Show a system tray icon"));
            if s.ob("ui.tray") {
                r.push(ot(s, "ui.tray-minimized", "Start minimized to the tray"));
            }
            r.push(od(
                s,
                "ui.popup",
                "Show a popup when the track changes",
                &["Never", "When minimized", "Always"],
            ));
            if s.oi("ui.popup") > 0 {
                let v = s.of("ui.popup-duration") as f32;
                r.push(slider(
                    "ui.popup-duration",
                    "Popup duration",
                    v,
                    1.0,
                    10.0,
                    &["0 s", "5 s", "10s"],
                    format!("{v:.0} s"),
                ));
            }
            r.push(header("Playlist"));
            r.push(ot(s, "ui.tree", "Show the playlist as a tree"));
            r.push(segmented(
                "ui.view",
                "Library view",
                &["Grid", "List"],
                if i.library_grid { 0 } else { 1 },
            ));
        }
        2 => {
            let p = &s.app.privacy;
            r.push(header("History"));
            r.push(toggle(
                "privacy.history",
                "Save recently played items",
                p.remember_history,
            ));
            r.push(toggle(
                "privacy.position",
                "Remember the playback position",
                p.remember_position,
            ));
            r.push(otext(
                s,
                "privacy.exclude",
                "Never remember items matching",
                "e.g. private|/Downloads/",
            ));
            r.push(toggle(
                "privacy.clear-exit",
                "Clear history when VLC closes",
                p.clear_history_on_exit,
            ));
            r.push(button("privacy.clear", "Clear history now", "Clear"));
            r.push(header("Network"));
            r.push(toggle(
                "privacy.fetch",
                "Fetch titles and cover art from the internet",
                p.fetch_remote_metadata,
            ));
            r.push(ot(s, "lastfm.enable", "Submit played tracks to last.fm"));
            if s.ob("lastfm.enable") {
                r.push(otext(
                    s,
                    "lastfm.user",
                    "Last.fm username",
                    "Last.fm username",
                ));
                r.push(opass(s, "lastfm.pass", "Last.fm password"));
                r.push(otext(
                    s,
                    "lastfm.url",
                    "Last.fm submission URL",
                    "post.audioscrobbler.com",
                ));
            }
        }
        3 => {
            let c = &s.app.controls;
            r.push(header("Keyboard Shortcuts"));
            r.push(button(
                "controls.edit",
                "Defaults",
                "Edit Keyboard Shortcuts",
            ));
            r.push(button("controls.reset", "Reset all shortcuts", "Reset"));
            r.push(header("Skip Amounts"));
            for (key, label, v, ph) in [
                (
                    "skip.veryshort",
                    "Very short skip",
                    c.skip_very_short_s,
                    "3 s",
                ),
                ("skip.short", "Short skip", c.skip_short_s, "10 s"),
                ("skip.medium", "Medium skip", c.skip_medium_s, "60 s"),
                ("skip.long", "Long skip", c.skip_long_s, "300 s"),
            ] {
                r.push(text(key, label, &format!("{v} s"), ph));
            }
            r.push(header("Mouse"));
            r.push(dropdown(
                "mouse.single",
                "Single click on the video",
                &["Play / Pause", "Nothing"],
                if c.single_click_play_pause { 0 } else { 1 },
            ));
            let dbl = match c.double_click_action {
                DoubleClickAction::ToggleFullscreen => 0,
                DoubleClickAction::TogglePlayPause => 1,
                DoubleClickAction::None => 2,
            };
            r.push(dropdown(
                "mouse.double",
                "Double click on the video",
                &["Toggle fullscreen", "Play / Pause", "Nothing"],
                dbl,
            ));
            let mid = match c.middle_click_action {
                MiddleClickAction::ToggleMute => 0,
                MiddleClickAction::PictureInPicture => 1,
                MiddleClickAction::None => 2,
            };
            r.push(dropdown(
                "mouse.middle",
                "Middle click on the video",
                &["Mute", "Picture in picture", "Nothing"],
                mid,
            ));
            r.push(od(
                s,
                "mouse.wheel",
                "Scroll wheel",
                &["Volume", "Seek", "Nothing"],
            ));
            r.push(od(
                s,
                "mouse.hwheel",
                "Horizontal scroll",
                &["Seek", "Volume", "Nothing"],
            ));
            r.push(toggle(
                "mouse.reverse",
                "Reverse scroll direction",
                c.reverse_scroll,
            ));
            r.push(header("Remotes"));
            r.push(ot(s, "remote.web", "Web remote control"));
            if s.ob("remote.web") {
                r.push(otext(s, "remote.port", "Port", "8080"));
                r.push(opass(s, "remote.password", "Password (user: vlc)"));
                if let Some(url) = &ctx.remote_url {
                    r.push(info_owned(tr_fmt(
                        "Open {} on a phone or computer of the same network.",
                        &[url],
                    )));
                }
            }
        }
        4 => {
            let p = &s.media.playback;
            r.push(header("Startup"));
            let resume = match p.resume_mode {
                ResumeMode::Always => 0,
                ResumeMode::Ask => 1,
                ResumeMode::Never => 2,
            };
            r.push(dropdown(
                "play.resume",
                "Continue where you left off",
                &["Always", "Ask", "Never"],
                resume,
            ));
            r.push(toggle(
                "play.autostart",
                "Start playing as soon as a file opens",
                p.start_on_open,
            ));
            r.push(slider(
                "play.speed",
                "Default playback speed",
                p.default_speed,
                0.25,
                2.0,
                &["0.25", "0.5", "0.75", "1", "1.25", "1.5", "1.75", "2"],
                format!("{:.2}x", p.default_speed),
            ));
            r.push(ot(s, "play.cork", "Pause when another app uses audio"));
            r.push(header("Tracks"));
            r.push(odl(
                s,
                "play.quality",
                "Preferred video quality",
                &opts::QUALITIES,
            ));
            r.push(odl(
                s,
                "play.menu-lang",
                "Preferred menu language",
                &opts::LANGUAGES,
            ));
            let tracks = ["Automatic", "1", "2", "3", "4", "5", "6", "7", "8"];
            r.push(od(s, "play.audio-track", "Default audio track", &tracks));
            r.push(od(s, "play.sub-track", "Default subtitle track", &tracks));
            r.push(header("Order"));
            r.push(dropdown(
                "play.repeat",
                "Repeat",
                &["Off", "All", "One"],
                st.repeat as usize,
            ));
            r.push(toggle("play.shuffle", "Shuffle", st.shuffle));
            r.push(toggle(
                "play.stop-each",
                "Stop after each item",
                p.stop_after_each,
            ));
            r.push(od(
                s,
                "play.finish",
                "When playback finishes",
                &["Stop", "Go to the library", "Quit Ondrane"],
            ));
            r.push(toggle(
                "play.next-in-folder",
                "Continue with the next file of the folder",
                p.auto_play_next,
            ));
            r.push(header("Seeking"));
            r.push(ot(s, "play.precise", "Seek precisely"));
            r.push(toggle(
                "play.thumbs",
                "Show thumbnail while scrubbing",
                p.seek_thumbnails,
            ));
            r.push(header("Files & Discs"));
            let mut drives = vec!["-".to_string()];
            drives.extend(ctx.drives.iter().cloned());
            let cur = s.os("play.drive");
            if !cur.is_empty() && !drives.contains(&cur) {
                drives.push(cur.clone());
            }
            let idx = drives.iter().position(|d| *d == cur).unwrap_or(0);
            r.push(dropdown_owned(
                "play.drive",
                "Default optical drive",
                drives,
                idx,
            ));
            r.push(od(
                s,
                "play.avi",
                "Repair damaged AVI files",
                &["Ask", "Always fix", "Never fix", "Fix when necessary"],
            ));
            r.push(ot(
                s,
                "play.mkv-preload",
                "Load matching .mkv files from the same folder",
            ));
        }
        5 => {
            let v = &s.media.video;
            r.push(header("Output"));
            r.push(toggle("video.enable", "Enable video", v.enable_video));
            r.push(toggle(
                "video.fullscreen",
                "Start videos in fullscreen",
                v.fullscreen_on_launch,
            ));
            r.push(toggle(
                "video.screensaver",
                "Stop the screensaver during playback",
                v.disable_screensaver,
            ));
            r.push(toggle_info(
                "video.wallpaper",
                "Play video as desktop wallpaper",
                v.video_wallpaper_mode,
                "Fullscreen behind every other window, without controls. Esc leaves it.",
            ));
            r.push(header("Picture"));
            picture_rows(st, &mut r);
            r.push(header("Deinterlacing"));
            let di = match v.deinterlace {
                DeinterlaceMode::Off => 0,
                DeinterlaceMode::Automatic => 1,
                DeinterlaceMode::On => 2,
            };
            r.push(dropdown(
                "video.deinterlace",
                "Deinterlacing",
                &["Off", "Automatic", "On"],
                di,
            ));
            r.push(odl(s, "video.deint-method", "Method", &opts::DEINT_METHODS));
            r.push(header("Framing"));
            r.push(dropdown_owned(
                "video.aspect",
                "Aspect ratio",
                ASPECTS.iter().map(|a| tr(a.0)).collect(),
                st.aspect_idx,
            ));
            r.push(dropdown_owned(
                "video.crop",
                "Crop",
                CROPS.iter().map(|a| tr(a.0)).collect(),
                st.crop_idx,
            ));
            r.push(ot(s, "video.scale", "Scale video to the window"));
            r.push(od(
                s,
                "video.zoom",
                "Zoom",
                &["Fit", "Fill", "1:4", "1:2", "1:1 (original size)", "2:1"],
            ));
            r.push(od(
                s,
                "video.position",
                "Position in the window",
                &["Center", "Top", "Bottom", "Left", "Right"],
            ));
            r.push(header("Snapshots"));
            r.push(path_row(
                "snap.dir",
                "Save to",
                &s.system.recording.snapshot_dir,
            ));
            r.push(otext(s, "snap.prefix", "File name prefix", "vlcsnap-"));
            r.push(dropdown(
                "snap.format",
                "Format",
                &["PNG", "JPG"],
                if s.system.recording.snapshot_format == SnapshotFormat::Jpeg {
                    1
                } else {
                    0
                },
            ));
            r.push(odl(s, "snap.size", "Size", &opts::SNAP_WIDTHS));
            r.push(ot(s, "snap.sequential", "Number files sequentially"));
            r.push(ot(s, "snap.preview", "Show a preview after capture"));
        }
        6 => {
            let a = &s.media.audio;
            r.push(header("Output"));
            r.push(toggle("audio.enable", "Enable audio", a.enable_audio));
            let mut names = vec![tr("System default")];
            names.extend(ctx.audio_devices.iter().map(|d| d.1.clone()));
            let cur = s.os("audio.device");
            let idx = ctx
                .audio_devices
                .iter()
                .position(|d| d.0 == cur)
                .map(|i| i + 1)
                .unwrap_or(0);
            r.push(dropdown_owned("audio.device", "Output device", names, idx));
            r.push(od(
                s,
                "audio.speakers",
                "Speaker setup",
                &[
                    "Automatic",
                    "Stereo",
                    "Reverse stereo",
                    "Left",
                    "Right",
                    "Dolby Surround",
                ],
            ));
            r.push(header("Volume"));
            let max_idx = match a.max_volume {
                0..=110 => 0,
                111..=150 => 1,
                _ => 2,
            };
            r.push(segmented(
                "audio.max",
                "Maximum volume",
                &["100%", "125%", "200%"],
                max_idx,
            ));
            r.push(toggle(
                "audio.remember",
                "Remember last volume",
                a.remember_last_volume,
            ));
            if !a.remember_last_volume {
                r.push(slider(
                    "audio.startup",
                    "Always starts at",
                    a.default_volume as f32,
                    0.0,
                    a.max_volume as f32,
                    &["0%", &format!("{}%", a.max_volume)],
                    format!("{}%", a.default_volume),
                ));
            }
            r.push(text(
                "audio.step",
                "Volume step",
                &format!("{}", a.volume_step),
                "5",
            ));
            r.push(header("Loudness"));
            r.push(ot(s, "audio.normvol", "Normalize loudness"));
            if s.ob("audio.normvol") {
                let v = s.of("audio.norm-level") as f32;
                r.push(slider(
                    "audio.norm-level",
                    "Target level",
                    v,
                    0.5,
                    10.0,
                    &["0.5", "10"],
                    format!("{v:.1}"),
                ));
            }
            r.push(ot(s, "audio.night", "Night mode"));
            r.push(od(
                s,
                "audio.replaygain",
                "Replay gain",
                &["Off", "Track", "Album"],
            ));
            let pre = s.of("audio.rg-preamp") as f32;
            r.push(slider(
                "audio.rg-preamp",
                "Replay gain preamp",
                pre,
                -20.0,
                20.0,
                &["-20 dB", "0 dB", "+20 dB"],
                format!("{pre:+.1} dB"),
            ));
            r.push(header("Effects"));
            r.push(toggle("audio.eq", "Equalizer", a.equalizer_enabled));
            r.push(dropdown(
                "audio.eq-preset",
                "Equalizer preset",
                &EQ_PRESETS,
                eq_preset_index(st),
            ));
            r.push(button("audio.open-eq", "Equalizer bands", "Open Equalizer"));
            r.push(ot(s, "audio.eq-2pass", "Two-pass"));
            r.push(ot(s, "audio.spatial", "Spatial audio"));
            if s.ob("audio.spatial") {
                r.push(od(
                    s,
                    "audio.spatial-mode",
                    "Spatial audio mode",
                    &["Headphone virtualization", "Room spatializer"],
                ));
            }
            r.push(ot(s, "audio.widen", "Stereo widening"));
            r.push(ot(s, "audio.compressor", "Compressor"));
            let pitch = s.of("audio.pitch") as f32;
            r.push(slider(
                "audio.pitch",
                "Pitch shift",
                pitch,
                -12.0,
                12.0,
                &["-12", "0", "+12"],
                format!("{pitch:+.1}"),
            ));
            r.push(ot(s, "audio.chorus", "Chorus/flanger"));
            r.push(ot(s, "audio.karaoke", "Karaoke"));
            r.push(header("Tracks"));
            r.push(odl(
                s,
                "audio.lang",
                "Preferred audio language",
                &opts::LANGUAGES,
            ));
            let d = a.audio_delay_ms as f32 / 1000.0;
            r.push(slider(
                "audio.delay",
                "Audio delay",
                d,
                -5.0,
                5.0,
                &["-5 s", "0 s", "+5 s"],
                format!("{:.3} s", d),
            ));
            r.push(header("Visualization"));
            r.push(odl(s, "audio.visual", "Visualizer", &opts::VISUALS));
        }
        7 => {
            let sb = &s.media.subtitles;
            r.push(header("Tracks & Languages"));
            r.push(toggle("sub.enable", "Enable subtitles", sb.enabled));
            r.push(odl(s, "sub.lang", "Preferred language", &opts::LANGUAGES));
            r.push(odl(s, "sub.encoding", "Text encoding", &opts::ENCODINGS));
            r.push(header("Loading Subtitles"));
            r.push(toggle(
                "sub.auto",
                "Load subtitle files automatically",
                sb.auto_load,
            ));
            r.push(odl(s, "sub.fuzzy", "File name matching", &opts::FUZZY));
            r.push(button(
                "sub.dialog",
                "Match subtitle files to a folder",
                "Add Subtitles",
            ));
            r.push(header("Appearance"));
            r.push(otext(s, "sub.font", "Font", "System, sans-serif"));
            r.push(segmented(
                "sub.size-mode",
                "Size values",
                &["Relative", "Pixels"],
                s.oi("sub.size-mode").clamp(0, 1) as usize,
            ));
            if s.oi("sub.size-mode") == 1 {
                let px = s.of("sub.size-px") as f32;
                r.push(slider(
                    "sub.size-px",
                    "Font size",
                    px,
                    10.0,
                    96.0,
                    &["10 px", "96 px"],
                    format!("{px:.0} px"),
                ));
            } else {
                r.push(od(
                    s,
                    "sub.size",
                    "Font size",
                    &["Automatic", "Smaller", "Small", "Normal", "Large", "Larger"],
                ));
            }
            r.push(ot(s, "sub.bold", "Bold"));
            r.push(ocolor(s, "sub.color", "Text color"));
            let op = s.of("sub.opacity") as f32;
            r.push(slider(
                "sub.opacity",
                "Text opacity",
                op,
                0.0,
                100.0,
                &["0%", "50%", "100%"],
                format!("{op:.0}%"),
            ));
            r.push(od(
                s,
                "sub.outline",
                "Outline thickness",
                &["None", "Thin", "Normal", "Thick"],
            ));
            r.push(ocolor(s, "sub.outline-color", "Outline color"));
            r.push(ot(s, "sub.shadow", "Shadow"));
            if s.ob("sub.shadow") {
                r.push(ocolor(s, "sub.shadow-color", "Shadow color"));
                r.push(od(
                    s,
                    "sub.shadow-dist",
                    "Shadow distance",
                    &["Small", "Medium", "Large"],
                ));
            }
            r.push(od(s, "sub.background", "Background", &["None", "Box"]));
            if s.oi("sub.background") == 1 {
                r.push(ocolor(s, "sub.bg-color", "Background color"));
                let bo = s.of("sub.bg-opacity") as f32;
                r.push(slider(
                    "sub.bg-opacity",
                    "Background opacity",
                    bo,
                    0.0,
                    100.0,
                    &["0%", "50%", "100%"],
                    format!("{bo:.0}%"),
                ));
            }
            r.push(od(
                s,
                "sub.position",
                "Vertical position",
                &["Bottom", "Raised", "High"],
            ));
            r.push(header("Timing"));
            let d = sb.delay_ms as f32 / 1000.0;
            r.push(slider(
                "sub.delay",
                "Subtitle delay",
                d,
                -5.0,
                5.0,
                &["-5 s", "0 s", "+5 s"],
                format!("{:.3} s", d),
            ));
            r.push(header("On-Screen Messages"));
            r.push(ot(s, "osd.feedback", "Show feedback for shortcuts"));
            if s.ob("osd.feedback") {
                r.push(secs_slider(
                    s,
                    "osd.feedback-dur",
                    "Show feedback for",
                    0.5,
                    5.0,
                ));
            }
            r.push(ot(s, "osd.filename", "Show filename when a video starts"));
            if s.ob("osd.filename") {
                r.push(secs_slider(
                    s,
                    "osd.filename-dur",
                    "Show filename for",
                    1.0,
                    10.0,
                ));
            }
        }
        8 => {
            r.push(header("Opening Files"));
            r.push(toggle_info(
                "lib.single",
                "Only allow one Ondrane window",
                s.ob("lib.single"),
                "Files opened later go to this window.",
            ));
            if s.ob("lib.single") {
                r.push(ot(
                    s,
                    "lib.fm-apply",
                    "Apply to files opened from the file manager",
                ));
            }
            r.push(ot(
                s,
                "lib.enqueue",
                "Add to the playlist instead of replacing",
            ));
            r.push(od(
                s,
                "lib.folder",
                "When opening a folder",
                &["Include subfolders", "Only this folder"],
            ));
            r.push(toggle(
                "lib.hidden",
                "Show hidden files",
                s.media.library.show_hidden,
            ));
            r.push(info_owned(tr_fmt(
                "Videos folder: {}",
                &[&library::tilde(&library::videos_dir())],
            )));
            r.push(info_owned(tr_fmt(
                "Music folder: {}",
                &[&library::tilde(&library::music_dir())],
            )));
            r.push(header("Metadata & Artwork"));
            r.push(dropdown(
                "privacy.fetch-index",
                "Fetch titles and artwork",
                &["From the file only", "From the file and the internet"],
                s.app.privacy.fetch_remote_metadata as usize,
            ));
            r.push(toggle_info(
                "lib.use-ml",
                "Use the media library",
                s.ob("lib.use-ml"),
                "Home sections, posters and video thumbnails of the library.",
            ));
            r.push(button(
                "lib.clear-thumbs",
                "Thumbnails and seek previews cache",
                "Clear",
            ));
        }
        9 => {
            let net = &s.system.network;
            r.push(header("Buffering"));
            r.push(dropdown(
                "net.preset",
                "Buffering",
                &["Low latency", "Normal", "High"],
                s.oi("net.preset").clamp(0, 2) as usize,
            ));
            r.push(secs_slider(s, "net.file-caching", "Local files", 0.1, 10.0));
            r.push(secs_slider(s, "net.disc-caching", "Discs", 0.1, 10.0));
            let secs = net.network_caching_ms as f32 / 1000.0;
            r.push(slider(
                "net.caching",
                "Network streams",
                secs,
                0.1,
                10.0,
                &["0.1 s", "5 s", "10 s"],
                format!("{:.1} s", secs),
            ));
            r.push(secs_slider(
                s,
                "net.live-caching",
                "Live capture",
                0.1,
                10.0,
            ));
            r.push(header("Proxy"));
            r.push(text(
                "net.proxy",
                "HTTP proxy",
                net.proxy_url.as_deref().unwrap_or(""),
                "System proxy",
            ));
            r.push(opass(s, "net.proxy-pwd", "HTTP proxy password"));
            r.push(header("Security"));
            r.push(toggle_info(
                "net.verify",
                "Verify certificates against the system trust store",
                s.ob("net.verify"),
                "Off: invalid certificates are accepted for 24 hours without asking.",
            ));
            r.push(od(
                s,
                "net.cipher",
                "Cipher preference",
                &["Performance", "Balanced", "Secure"],
            ));
            if advanced {
                r.push(text("net.ua", "User agent", &net.user_agent, "Ondrane"));
            }
        }
        10 => {
            let p = &s.system.performance;
            r.push(header("Decoding"));
            r.push(dropdown(
                "perf.hw",
                "Hardware-accelerated decoding",
                &["Automatic", "Disabled"],
                if s.media.video.hardware_decoding {
                    0
                } else {
                    1
                },
            ));
            if cfg!(target_os = "linux") && s.app.interface.embedded_video {
                r.push(info("On Linux, hardware decoding is available when video plays in a separate window."));
            }
            r.push(od(
                s,
                "perf.postproc",
                "Video post-processing",
                &["Off", "Low", "Medium", "High"],
            ));
            let lf = match p.skip_loop_filter {
                SkipLoopFilter::None => 0,
                SkipLoopFilter::NonRef => 1,
                SkipLoopFilter::Bidir => 2,
                SkipLoopFilter::NonKey => 3,
                SkipLoopFilter::All => 4,
            };
            r.push(dropdown(
                "perf.loop",
                "Skip the H.264 loop filter",
                &[
                    "Never",
                    "Non-reference",
                    "Bidirectional",
                    "Non-key",
                    "Always",
                ],
                lf,
            ));
            r.push(text(
                "perf.threads",
                "Decoding threads (0 = automatic)",
                &p.decoding_threads.to_string(),
                "0",
            ));
            r.push(header("Smooth Playback"));
            r.push(toggle(
                "perf.drop",
                "Drop frames when the system can't keep up",
                p.drop_late_frames,
            ));
            r.push(ot(s, "perf.rt", "Give VLC real-time priority"));
            if s.ob("perf.rt") {
                let pr = s.oi("perf.priority") as f32;
                r.push(slider(
                    "perf.priority",
                    "Process priority",
                    pr,
                    -10.0,
                    10.0,
                    &["-10", "0", "+10"],
                    format!("{pr:+.0}"),
                ));
            }
            r.push(button("perf.rescan", "Rescan plugins", "Rescan"));
            if advanced || s.app.global.developer_mode {
                r.push(header("Developer"));
                r.push(odl(s, "perf.encoder", "Encoder", &opts::ENCODERS));
                r.push(odl(s, "perf.decoder", "Decoder", &opts::DECODERS));
                r.push(od(
                    s,
                    "perf.clock-sync",
                    "Clock synchronisation",
                    &["Default", "Disabled", "Enabled"],
                ));
                r.push(otext(s, "perf.clock-jitter", "Clock jitter (ms)", "5000"));
                r.push(otext(
                    s,
                    "perf.cr-average",
                    "Clock reference averaging",
                    "40",
                ));
            }
        }
        11 => {
            let rec = &s.system.recording;
            r.push(header("Recording"));
            r.push(path_row(
                "rec.dir",
                "Save recordings to",
                &rec.recording_dir,
            ));
            r.push(ot(
                s,
                "rec.copy",
                "Record without re-encoding when possible",
            ));
            r.push(button(
                "rec.toggle",
                "Record the media that is playing",
                "Start / Stop",
            ));
            r.push(header("Casting"));
            r.push(ot(s, "cast.discover", "Discover cast devices"));
            r.push(od(
                s,
                "cast.quality",
                "Cast quality",
                &["Automatic", "Low", "Medium", "High"],
            ));
            r.push(otext(
                s,
                "cast.ip",
                "Chromecast address (if not discovered)",
                "192.168.1.20",
            ));
            r.push(header("Streaming"));
            r.push(ot(s, "stream.local", "Play locally while streaming"));
            r.push(ot(s, "stream.audio", "Include audio"));
            r.push(ot(s, "stream.video", "Include video"));
            r.push(ot(s, "stream.subs", "Include subtitles"));
            r.push(od(
                s,
                "stream.preset",
                "Encoding preset",
                &opts::X264_PRESETS,
            ));
            r.push(odl(s, "stream.tune", "Tuning", &opts::X264_TUNES));
            r.push(button(
                "stream.open",
                "Stream the media that is playing",
                "Stream Out",
            ));
            r.push(header("Snapshots"));
            r.push(path_row("snap.dir", "Save snapshots to", &rec.snapshot_dir));
        }
        _ => {
            r.push(header("Add-ons"));
            r.push(toggle_info(
                "ext.load",
                "Load add-ons on startup",
                s.system.extensions.lua_enabled,
                "Lua scripts installed for your user (built-in VLC scripts always load).",
            ));
            r.push(button("ext.install", "Install a Lua add-on", "Install"));
            r.push(button("ext.folder", "Add-ons folder", "Open"));
            let mut last = String::new();
            for a in &ctx.addons {
                if a.category != last {
                    last = a.category.clone();
                    r.push(header(crate::addons::category_label(&a.category)));
                }
                let key = format!("addon:{}", a.path.display());
                if a.user {
                    r.push(SettingRow {
                        info: ss(tr("Installed by you")),
                        ..toggle(&key, &a.name, a.enabled)
                    });
                } else {
                    r.push(SettingRow {
                        kind: 10,
                        label: ss(format!("{} · {}", a.name, tr("built into VLC"))),
                        ..Default::default()
                    });
                }
            }
            r.push(header("Engine"));
            r.push(info_owned(format!(
                "libVLC: {}",
                crate::engine::find_libvlc()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| tr("not found"))
            )));
        }
    }
    r
}

/// Rows of every page whose label matches `query`.
pub fn search(query: &str, st: &State, ctx: &Ctx) -> Vec<SettingRow> {
    let q = query.to_lowercase();
    let mut out = Vec::new();
    for (i, title) in PAGE_TITLES.iter().enumerate() {
        let rows: Vec<SettingRow> = page(i as i32, st, true, ctx)
            .into_iter()
            .filter(|r| r.kind != 0 && r.kind != 9 && r.label.to_lowercase().contains(&q))
            .collect();
        if !rows.is_empty() {
            out.push(header(title));
            out.extend(rows);
        }
    }
    if out.is_empty() {
        out.push(info("No setting matches your search."));
    }
    out
}

/// Quick-settings drawer: 0 playback, 1 video, 2 audio, 3 subtitles.
pub fn quick(
    tab: i32,
    st: &State,
    player: Option<&VlcPlayer>,
    presets: &[String],
) -> Vec<SettingRow> {
    let s = &st.settings;
    let mut r = Vec::new();
    if tab == 1 || tab == 2 {
        // Figma "Setting Preset"
        let cur = s.os("preset.current");
        let idx = presets
            .iter()
            .position(|p| *p == cur || *p == tr(&cur))
            .unwrap_or(0);
        r.push(dropdown_owned("q.preset", "Preset", presets.to_vec(), idx));
    }
    match tab {
        0 => {
            let rate = player
                .map(|p| p.rate())
                .filter(|r| *r > 0.0)
                .unwrap_or(s.media.playback.default_speed);
            r.push(header("Speed"));
            r.push(slider(
                "q.speed",
                "Playback Speed",
                rate,
                0.25,
                4.0,
                &["0.25x", "1x", "2x", "4x"],
                format!("{:.2}x", rate),
            ));
            r.push(header("Order"));
            r.push(segmented(
                "play.repeat",
                "Repeat",
                &["Off", "All", "One"],
                st.repeat as usize,
            ));
            r.push(toggle("play.shuffle", "Shuffle", st.shuffle));
            r.push(toggle(
                "play.stop-each",
                "Stop after each",
                s.media.playback.stop_after_each,
            ));
        }
        1 => {
            r.push(header("Essential"));
            r.push(dropdown_owned(
                "video.aspect",
                "Aspect Ratio",
                ASPECTS.iter().map(|a| tr(a.0)).collect(),
                st.aspect_idx,
            ));
            r.push(dropdown_owned(
                "video.crop",
                "Crop",
                CROPS.iter().map(|a| tr(a.0)).collect(),
                st.crop_idx,
            ));
            r.push(od(
                s,
                "video.zoom",
                "Zoom",
                &["Fit", "Fill", "1:4", "1:2", "1:1 (original size)", "2:1"],
            ));
            r.push(header("Colors"));
            picture_rows(st, &mut r);
            // Figma "Video - Colors"
            r.push(header("Color Filters"));
            r.push(ot(s, "vf.extract", "Color Extraction"));
            if s.ob("vf.extract") {
                r.push(ocolor(s, "vf.extract-color", "Extracted color"));
            }
            r.push(ot(s, "vf.threshold", "Color Threshold"));
            if s.ob("vf.threshold") {
                r.push(ocolor(s, "vf.threshold-color", "Kept color"));
                let sat = s.of("vf.threshold-sat") as f32;
                r.push(slider(
                    "vf.threshold-sat",
                    "Saturation",
                    sat,
                    0.0,
                    255.0,
                    &[],
                    format!("{sat:.0}"),
                ));
                let sim = s.of("vf.threshold-sim") as f32;
                r.push(slider(
                    "vf.threshold-sim",
                    "Similarity",
                    sim,
                    0.0,
                    255.0,
                    &[],
                    format!("{sim:.0}"),
                ));
            }
            r.push(ot(s, "vf.invert", "Negate Colors"));
            r.push(ot(s, "vf.posterize", "Posterize"));
            r.push(ot(s, "vf.gradient", "Gradient"));
            if s.ob("vf.gradient") {
                r.push(od(
                    s,
                    "vf.gradient-mode",
                    "Mode",
                    &["Gradient", "Edge", "Hough"],
                ));
                r.push(ot(s, "vf.gradient-color", "Color"));
                r.push(ot(s, "vf.cartoon", "Cartoon"));
            }
            let sepia = s.of("vf.sepia") as f32;
            r.push(slider(
                "vf.sepia",
                "Sepia Intensity",
                sepia,
                0.0,
                255.0,
                &[],
                format!("{sepia:.0}"),
            ));
            // Figma "Video - Crop"
            r.push(header("Crop (pixels)"));
            for (k, l) in [
                ("crop.top", "Top"),
                ("crop.bottom", "Bottom"),
                ("crop.left", "Left"),
                ("crop.right", "Right"),
            ] {
                r.push(otext(s, k, l, "0"));
            }
        }
        2 => {
            r.push(header("Tracks"));
            if let Some(p) = player {
                let tracks = p.audio_tracks();
                let cur = p.current_audio_track();
                let idx = tracks.iter().position(|t| t.id == cur).unwrap_or(0);
                let names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
                if !names.is_empty() {
                    r.push(dropdown_owned("q.audio-track", "Audio Track", names, idx));
                }
            }
            let d = s.media.audio.audio_delay_ms as f32 / 1000.0;
            r.push(slider(
                "audio.delay",
                "Audio Track Delay",
                d,
                -5.0,
                5.0,
                &["-5 s", "0 s", "+5 s"],
                format!("{:.3} s", d),
            ));
            r.push(header("Equalizer"));
            r.push(toggle(
                "audio.eq",
                "Equalizer",
                s.media.audio.equalizer_enabled,
            ));
            r.push(dropdown(
                "audio.eq-preset",
                "Preset",
                &EQ_PRESETS,
                eq_preset_index(st),
            ));
            if s.media.audio.equalizer_enabled {
                r.push(equalizer());
            }
            // Figma "Audio - Compressor / Spatializer Settings / Stereo Widener"
            r.push(header("Effects"));
            r.push(ot(s, "audio.compressor", "Compressor"));
            if s.ob("audio.compressor") {
                for (k, l, min, max, unit) in [
                    ("comp.rms", "RMS / Peak", 0.0, 1.0, ""),
                    ("comp.attack", "Attack", 1.5, 400.0, " ms"),
                    ("comp.release", "Release", 2.0, 800.0, " ms"),
                    ("comp.threshold", "Threshold", -30.0, 0.0, " dB"),
                    ("comp.ratio", "Ratio", 1.0, 20.0, " : 1"),
                    ("comp.knee", "Knee Radius", 1.0, 10.0, " dB"),
                    ("comp.makeup", "Makeup Gain", 0.0, 24.0, " dB"),
                ] {
                    let v = s.of(k) as f32;
                    r.push(slider(k, l, v, min, max, &[], format!("{v:.1}{unit}")));
                }
            }
            r.push(ot(s, "audio.spatial", "Spatializer"));
            if s.ob("audio.spatial") {
                r.push(od(
                    s,
                    "audio.spatial-mode",
                    "Mode",
                    &["Headphone virtualization", "Room spatializer"],
                ));
                if s.oi("audio.spatial-mode") == 1 {
                    for (k, l) in [
                        ("sp.size", "Size"),
                        ("sp.width", "Width"),
                        ("sp.wet", "Wet"),
                        ("sp.dry", "Dry"),
                        ("sp.damp", "Damp"),
                    ] {
                        let v = s.of(k) as f32;
                        r.push(slider(k, l, v, 0.0, 10.0, &[], format!("{v:.1}")));
                    }
                }
            }
            r.push(ot(s, "audio.widen", "Stereo Widener"));
            if s.ob("audio.widen") {
                for (k, l, min, max, unit) in [
                    ("widen.delay", "Delay Time", 1.0, 100.0, " ms"),
                    ("widen.feedback", "Feedback Gain", 0.0, 0.9, ""),
                    ("widen.crossfeed", "Crossfeed", 0.0, 0.8, ""),
                    ("widen.dry", "Dry Mix", 0.0, 1.0, ""),
                ] {
                    let v = s.of(k) as f32;
                    r.push(slider(k, l, v, min, max, &[], format!("{v:.2}{unit}")));
                }
            }
            // Figma "Audio Visualizer"
            r.push(header("Visualization"));
            r.push(odl(s, "audio.visual", "Visualizer", &opts::VISUALS));
        }
        _ => {
            r.push(header("Defaults"));
            if let Some(p) = player {
                let tracks = p.subtitle_tracks();
                let cur = p.current_subtitle_track();
                let idx = tracks.iter().position(|t| t.id == cur).unwrap_or(0);
                let names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
                if !names.is_empty() {
                    r.push(dropdown_owned("q.sub-track", "Subtitle Track", names, idx));
                }
            }
            r.push(button("q.load-sub", "Subtitle File", "Browse"));
            r.push(header("Synchronization"));
            let d = s.media.subtitles.delay_ms as f32 / 1000.0;
            r.push(slider(
                "sub.delay",
                "Subtitle Track Delay",
                d,
                -10.0,
                10.0,
                &["-10 s", "0 s", "+10 s"],
                format!("{:.3} s", d),
            ));
            r.push(header("Loading"));
            r.push(toggle(
                "sub.auto",
                "Load automatically",
                s.media.subtitles.auto_load,
            ));
        }
    }
    r
}

/// Store an `extra` option; `Opt` only when the value changed.
fn set_opt(st: &mut State, key: &str, v: serde_json::Value) -> Effect {
    let Some(o) = opts::find(key) else {
        return Effect::Nothing;
    };
    if st.settings.extra.get(key) == Some(&v) {
        return Effect::Nothing;
    }
    st.settings.extra.insert(key.to_string(), v);
    Effect::Opt(o.key)
}

pub fn set_bool(st: &mut State, key: &str, v: bool) -> Effect {
    if let Some(path) = key.strip_prefix("addon:") {
        return Effect::Command(format!("addon-toggle:{v}:{path}"));
    }
    let s = &mut st.settings;
    match key {
        "app.welcome" => s.app.global.onboarding_completed = !v,
        "app.dialogs" => s.app.global.show_error_dialogs = v,
        "app.updates" => s.app.global.check_updates = v,
        "app.devmode" => {
            s.app.global.developer_mode = v;
            st.settings_advanced = v || st.settings_advanced;
            return Effect::Engine;
        }
        "app.savelog" => {
            s.app.global.save_log = v;
            return Effect::Engine;
        }
        "ui.title" => s.app.interface.show_filename_in_title = v,
        "ui.compact" => s.app.interface.start_compact = v,
        "ui.embedded" => {
            s.app.interface.embedded_video = v;
            return Effect::Engine;
        }
        "ui.ontop" => {
            s.app.interface.always_on_top = v;
            return Effect::Window;
        }
        "privacy.history" => s.app.privacy.remember_history = v,
        "privacy.position" => s.app.privacy.remember_position = v,
        "privacy.clear-exit" => s.app.privacy.clear_history_on_exit = v,
        "privacy.fetch" => {
            s.app.privacy.fetch_remote_metadata = v;
            return Effect::Engine;
        }
        "mouse.reverse" => s.app.controls.reverse_scroll = v,
        "play.autostart" => s.media.playback.start_on_open = v,
        "play.shuffle" => {
            st.shuffle = v;
            st.settings.media.playback.shuffle = v;
            return Effect::Queue;
        }
        "play.stop-each" => s.media.playback.stop_after_each = v,
        "play.next-in-folder" => s.media.playback.auto_play_next = v,
        "play.thumbs" => s.media.playback.seek_thumbnails = v,
        "video.enable" => s.media.video.enable_video = v,
        "video.fullscreen" => s.media.video.fullscreen_on_launch = v,
        "video.screensaver" => s.media.video.disable_screensaver = v,
        "video.wallpaper" => {
            s.media.video.video_wallpaper_mode = v;
            return Effect::Window;
        }
        "audio.enable" => s.media.audio.enable_audio = v,
        "audio.remember" => s.media.audio.remember_last_volume = v,
        "audio.eq" => {
            s.media.audio.equalizer_enabled = v;
            return Effect::Audio;
        }
        "sub.enable" => s.media.subtitles.enabled = v,
        "sub.auto" => s.media.subtitles.auto_load = v,
        "lib.hidden" => {
            s.media.library.show_hidden = v;
            return Effect::Browse;
        }
        "perf.drop" => s.system.performance.drop_late_frames = v,
        "ext.load" => {
            s.system.extensions.lua_enabled = v;
            return Effect::Engine;
        }
        _ => return set_opt(st, key, v.into()),
    }
    Effect::Nothing
}

pub fn set_int(st: &mut State, key: &str, v: i32) -> Effect {
    let i = v.max(0) as usize;
    let s = &mut st.settings;
    match key {
        "app.language" => {
            s.app.global.language = crate::i18n::choice_code(i).to_string();
            return Effect::Language;
        }
        "app.palette" => {
            s.app.global.color_scheme = crate::themes::ColorScheme::ALL[i.min(6)];
            s.set_extra("ui.skin", false);
            return Effect::Theme;
        }
        "app.catppuccin" => {
            s.app.global.catppuccin_flavor = crate::themes::CatppuccinFlavor::ALL[i.min(3)];
            s.set_extra("ui.skin", false);
            return Effect::Theme;
        }
        "app.theme" => {
            s.app.global.color_scheme = crate::themes::ColorScheme::Ondrane;
            s.app.global.theme_mode =
                [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System][i.min(2)];
            return Effect::Theme;
        }
        "app.accent" => {
            s.app.global.accent_color = ACCENTS[i.min(ACCENTS.len() - 1)].to_string();
            return Effect::Theme;
        }
        "app.logdetail" => {
            s.app.global.log_detail = i.min(2) as u8;
            return Effect::Engine;
        }
        "ui.autohide" => s.app.interface.auto_hide_controls = i.min(2) as u8,
        "ui.view" => {
            s.app.interface.library_grid = i == 0;
            return Effect::Command(if i == 0 {
                "view-grid".into()
            } else {
                "view-list".into()
            });
        }
        "mouse.single" => s.app.controls.single_click_play_pause = i == 0,
        "mouse.double" => {
            s.app.controls.double_click_action = [
                DoubleClickAction::ToggleFullscreen,
                DoubleClickAction::TogglePlayPause,
                DoubleClickAction::None,
            ][i.min(2)]
        }
        "mouse.middle" => {
            s.app.controls.middle_click_action = [
                MiddleClickAction::ToggleMute,
                MiddleClickAction::PictureInPicture,
                MiddleClickAction::None,
            ][i.min(2)]
        }
        "play.resume" => {
            s.media.playback.resume_mode =
                [ResumeMode::Always, ResumeMode::Ask, ResumeMode::Never][i.min(2)]
        }
        "play.repeat" => {
            st.repeat = i.min(2) as i32;
            st.settings.media.playback.loop_mode = [
                crate::settings::LoopMode::None,
                crate::settings::LoopMode::All,
                crate::settings::LoopMode::Single,
            ][i.min(2)];
            return Effect::Queue;
        }
        "play.drive" => return Effect::Command(format!("drive-index:{i}")),
        "video.deinterlace" => {
            s.media.video.deinterlace = [
                DeinterlaceMode::Off,
                DeinterlaceMode::Automatic,
                DeinterlaceMode::On,
            ][i.min(2)];
            return Effect::Opt("video.deint-method");
        }
        "video.aspect" => {
            st.aspect_idx = i.min(ASPECTS.len() - 1);
            return Effect::Picture;
        }
        "video.crop" => {
            st.crop_idx = i.min(CROPS.len() - 1);
            return Effect::Picture;
        }
        "snap.format" => {
            s.system.recording.snapshot_format = if i == 1 {
                SnapshotFormat::Jpeg
            } else {
                SnapshotFormat::Png
            }
        }
        "audio.device" => return Effect::Command(format!("audio-device-index:{i}")),
        "audio.max" => {
            s.media.audio.max_volume = [100, 125, 200][i.min(2)];
            return Effect::Volume;
        }
        "audio.eq-preset" => return Effect::Command(format!("eq-preset:{i}")),
        "privacy.fetch-index" => {
            s.app.privacy.fetch_remote_metadata = i == 1;
            return Effect::Engine;
        }
        "net.preset" => {
            let (_, f, d, n, l) = opts::BUFFER_PRESETS[i.min(2)];
            s.set_extra("net.preset", i as i64);
            s.set_extra("net.file-caching", f);
            s.set_extra("net.disc-caching", d);
            s.set_extra("net.live-caching", l);
            s.system.network.network_caching_ms = (n * 1000.0) as u32;
            return Effect::Opt("net.file-caching");
        }
        "perf.hw" => {
            s.media.video.hardware_decoding = i == 0;
            return Effect::Engine;
        }
        "perf.loop" => {
            s.system.performance.skip_loop_filter = [
                SkipLoopFilter::None,
                SkipLoopFilter::NonRef,
                SkipLoopFilter::Bidir,
                SkipLoopFilter::NonKey,
                SkipLoopFilter::All,
            ][i.min(4)]
        }
        "q.audio-track" => return Effect::Command(format!("audio-track-idx:{i}")),
        "q.sub-track" => return Effect::Command(format!("subtitle-track-idx:{i}")),
        "q.preset" => return Effect::Command(format!("preset:{i}")),
        _ => return set_opt(st, key, (i as i64).into()),
    }
    Effect::Nothing
}

pub fn set_float(st: &mut State, key: &str, v: f32) -> Effect {
    if let Some(band) = key.strip_prefix("eq.") {
        let b: usize = band.parse().unwrap_or(0);
        let a = &mut st.settings.media.audio;
        if a.equalizer_db.len() != 11 {
            a.equalizer_db = vec![0.0; 11];
        }
        if b < 11 {
            a.equalizer_db[b] = ((v * 40.0 - 20.0) * 10.0).round() / 10.0;
        }
        a.equalizer_enabled = true;
        a.equalizer_preset = None;
        return Effect::Audio;
    }
    let s = &mut st.settings;
    match key {
        "ui.hide-after" => s.app.interface.controls_hide_delay_ms = (v * 1000.0).round() as u32,
        "play.speed" => {
            s.media.playback.default_speed = (v * 4.0).round() / 4.0;
            return Effect::Rate(s.media.playback.default_speed);
        }
        "q.speed" => return Effect::Rate((v * 20.0).round() / 20.0),
        "pic.hue" => s.media.video.picture_adjust.hue = v.round(),
        "pic.brightness" => s.media.video.picture_adjust.brightness = v,
        "pic.contrast" => s.media.video.picture_adjust.contrast = v,
        "pic.saturation" => s.media.video.picture_adjust.saturation = v,
        "pic.gamma" => s.media.video.picture_adjust.gamma = v,
        "audio.startup" => s.media.audio.default_volume = v.round() as u32,
        "audio.delay" => {
            s.media.audio.audio_delay_ms = (v * 1000.0 / 50.0).round() as i32 * 50;
            return Effect::Delays;
        }
        "sub.delay" => {
            s.media.subtitles.delay_ms = (v * 1000.0 / 50.0).round() as i32 * 50;
            return Effect::Delays;
        }
        "net.caching" => {
            s.system.network.network_caching_ms = (v * 1000.0 / 100.0).round() as u32 * 100;
            return Effect::Opt("net.file-caching");
        }
        _ => {
            return match opts::find(key).map(|o| o.def) {
                Some(Def::I(_)) => set_opt(st, key, (v.round() as i64).into()),
                Some(Def::F(_)) => {
                    let rounded = (v as f64 * 100.0).round() / 100.0;
                    set_opt(st, key, rounded.into())
                }
                _ => Effect::Nothing,
            }
        }
    }
    if key.starts_with("pic.") {
        st.settings.media.video.picture_adjust.enabled = true;
        return Effect::Picture;
    }
    Effect::Nothing
}

fn parse_secs(v: &str) -> Option<u32> {
    v.trim().trim_end_matches('s').trim().parse::<u32>().ok()
}

pub fn set_text(st: &mut State, key: &str, v: &str) -> Effect {
    if let Some(path) = key.strip_prefix("addon-remove:") {
        return Effect::Command(format!("addon-remove:x:{path}"));
    }
    let s = &mut st.settings;
    match key {
        "app.reset" => {
            let keep_onboarding = s.app.global.onboarding_completed;
            *s = OndraneSettings::default();
            s.app.global.onboarding_completed = keep_onboarding;
            return Effect::Theme;
        }
        "privacy.clear" => return Effect::Command("clear-history".into()),
        "controls.edit" => return Effect::Command("shortcuts".into()),
        "controls.reset" => s.app.controls.shortcuts.clear(),
        "skip.veryshort" => {
            s.app.controls.skip_very_short_s =
                parse_secs(v).unwrap_or(s.app.controls.skip_very_short_s)
        }
        "skip.short" => {
            s.app.controls.skip_short_s = parse_secs(v).unwrap_or(s.app.controls.skip_short_s)
        }
        "skip.medium" => {
            s.app.controls.skip_medium_s = parse_secs(v).unwrap_or(s.app.controls.skip_medium_s)
        }
        "skip.long" => {
            s.app.controls.skip_long_s = parse_secs(v).unwrap_or(s.app.controls.skip_long_s)
        }
        "pic.reset" => {
            s.media.video.picture_adjust = Default::default();
            return Effect::Picture;
        }
        "snap.dir" => {
            if !v.trim().is_empty() {
                s.system.recording.snapshot_dir = library::expand_tilde(v.trim());
            }
        }
        "rec.dir" => {
            if !v.trim().is_empty() {
                s.system.recording.recording_dir = library::expand_tilde(v.trim());
            }
        }
        "audio.step" => {
            s.media.audio.volume_step = v.trim().parse::<u32>().unwrap_or(5).clamp(1, 25)
        }
        "audio.open-eq" => return Effect::Command("open-eq".into()),
        "sub.dialog" => return Effect::Command("add-subtitles".into()),
        "lib.clear-thumbs" => {
            let dir = st.thumbs.cache_dir().to_path_buf();
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            st.posters.clear();
            return Effect::Browse;
        }
        "net.proxy" => {
            s.system.network.proxy_url = Some(v.trim().to_string()).filter(|p| !p.is_empty());
            return Effect::Opt("net.proxy-pwd");
        }
        "net.ua" => {
            if !v.trim().is_empty() {
                s.system.network.user_agent = v.trim().to_string();
            }
        }
        "perf.threads" => {
            s.system.performance.decoding_threads = v.trim().parse::<u32>().unwrap_or(0).min(64);
            return Effect::Opt("perf.decoder");
        }
        "perf.rescan" => return Effect::Command("rescan-plugins".into()),
        "rec.toggle" => return Effect::Command("record".into()),
        "stream.open" => return Effect::Command("settings-then:stream-dialog".into()),
        "ui.skin-export" => return Effect::Command("skin-export".into()),
        "ext.install" => return Effect::Command("addon-install".into()),
        "ext.folder" => return Effect::Command("addon-folder".into()),
        "q.load-sub" => return Effect::Command("load-subtitle".into()),
        _ => {
            return match opts::find(key).map(|o| o.def) {
                Some(Def::I(d)) => {
                    let n = v
                        .trim()
                        .trim_end_matches("ms")
                        .trim()
                        .parse::<i64>()
                        .unwrap_or(d);
                    set_opt(st, key, n.into())
                }
                Some(Def::S(_)) => set_opt(st, key, v.trim().to_string().into()),
                _ => Effect::Nothing,
            }
        }
    }
    Effect::Nothing
}
