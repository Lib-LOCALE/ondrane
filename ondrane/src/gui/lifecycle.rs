//! libVLC lifecycle: creating the player, restarting the engine when an
//! instance option changes, reopening the current media when a per-media
//! option changes, player events (audio cork, errors, video size) and the
//! interactive dialogs libVLC raises (logins, certificate questions, errors).

use super::*;
use crate::engine::{DialogEvent, DialogId, PlayerEvent};
use crate::options::{self, Apply};

/// A libVLC dialog waiting for an answer, shown by the "VLC dialog" popup.
#[derive(Debug, Clone)]
pub struct PendingDialog {
    pub id: Option<DialogId>,
    pub event: DialogEvent,
}

impl Controller {
    /// Pick up the background libVLC load once it has finished: create the
    /// player, then open what was given on the command line.
    pub(super) fn poll_engine_load(self: &Rc<Self>) {
        let result = {
            let rx = self.engine_rx.borrow();
            let Some(rx) = rx.as_ref() else { return };
            match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
            }
        };
        *self.engine_rx.borrow_mut() = None;
        let w = self.ui();
        let a = w.global::<AppState>();
        {
            if let Some((path, engine)) = result {
                let _ = self.libvlc.set(path);
                *self.engine.borrow_mut() = Some(engine);
                let poster_dir = self.st.borrow().thumbs.cache_dir().join("posters");
                let _ = self
                    .posters
                    .set(PosterWorker::new(poster_dir, |media, file| {
                        post(move |c| c.poster_ready(media, file));
                    }));
                self.attach_engine();
                if let Some(p) = self.player() {
                    let max = a.get_max_volume();
                    p.set_volume(a.get_volume().min(max));
                }
                self.start_cast_discovery();
            }
        }
        a.set_engine_ok(self.player().is_some());
        if self.player().is_none() {
            self.toast("libVLC not found: install VLC to enable playback");
            return;
        }
        if let Some((targets, subtitle)) = self.startup_media.borrow_mut().take() {
            if !targets.is_empty() {
                self.open_targets(targets, false);
                if let (Some(sub), Some(p)) = (subtitle, self.player()) {
                    let _ = p.add_subtitle_file(&sub);
                }
            }
        }
    }

    /// A playback action was requested but there is no player (yet).
    pub(super) fn playback_unavailable(&self) {
        if self.engine_rx.borrow().is_some() {
            self.toast(tr("Starting the playback engine…"));
        } else {
            self.show_error(
                &tr("Playback not available"),
                &tr("libVLC was not found on your system. Please install VLC media player or place libvlc.dll in the application folder."),
            );
        }
    }

    /// Create the player of the current engine and connect its frames, events
    /// and dialogs to the UI.
    pub(super) fn attach_engine(self: &Rc<Self>) {
        let engine = self.engine.borrow();
        let Some(e) = engine.as_ref() else {
            *self.player.borrow_mut() = None;
            return;
        };
        e.set_dialog_handler(|ev| post(move |c| c.on_vlc_dialog(ev)));
        let embedded = self.st.borrow().settings.app.interface.embedded_video;
        let player = match e.create_player_with(embedded) {
            Ok(p) => Rc::new(p),
            Err(err) => {
                eprintln!("cannot create the libVLC player: {err}");
                return;
            }
        };
        drop(engine);
        if embedded {
            install_frame_sink(&player);
        }
        *self.player.borrow_mut() = Some(player);
        self.apply_output_device();
    }

    /// Audio device and speaker layout ("Output device", "Speaker setup").
    pub(super) fn apply_output_device(&self) {
        let Some(p) = self.player() else { return };
        let st = self.st.borrow();
        let dev = st.settings.os("audio.device");
        if !dev.is_empty() {
            p.set_audio_device(&dev);
        }
        let ch = st.settings.oi("audio.speakers");
        if ch > 0 {
            p.set_audio_channel(ch as i32);
        }
    }

    /// Apply a changed option once the user stops dragging (debounced).
    pub(super) fn schedule_apply(&self, apply: Apply) {
        let mut x = self.x.borrow_mut();
        let rank = |a: Apply| match a {
            Apply::App => 0,
            Apply::Media => 1,
            Apply::Engine => 2,
        };
        let keep = x.pending_apply.map(|(a, _)| a).unwrap_or(Apply::App);
        let next = if rank(apply) >= rank(keep) {
            apply
        } else {
            keep
        };
        x.pending_apply = Some((next, Instant::now() + Duration::from_millis(700)));
    }

    pub(super) fn run_pending_apply(self: &Rc<Self>) {
        let due = {
            let x = self.x.borrow();
            match x.pending_apply {
                Some((a, at)) if Instant::now() >= at => Some(a),
                _ => None,
            }
        };
        let Some(apply) = due else { return };
        self.x.borrow_mut().pending_apply = None;
        match apply {
            Apply::Engine => self.restart_engine(),
            Apply::Media => self.reload_media(),
            Apply::App => {}
        }
    }

    /// Current media, time and play state, to restore after a reopen.
    pub(super) fn playback_snapshot(&self) -> Option<(usize, i64, bool)> {
        let p = self.player()?;
        let st = self.st.borrow();
        let idx = st.index?;
        st.current.as_ref()?;
        if st.ended {
            return None;
        }
        Some((idx, p.time_ms(), p.state() != PlaybackState::Paused))
    }

    /// Reopen the current media at the same time (per-media options,
    /// recording, streaming and casting all apply when an input starts).
    pub fn reload_media(self: &Rc<Self>) {
        let Some((idx, time, playing)) = self.playback_snapshot() else {
            return;
        };
        self.x.borrow_mut().start_override = Some((time, !playing));
        self.play_index(idx);
    }

    /// Recreate the libVLC instance with the current instance options
    /// (audio filters, subtitle rendering, logging…) and resume playback.
    pub fn restart_engine(self: &Rc<Self>) {
        let Some(path) = self.libvlc.get().cloned() else {
            return;
        };
        let snap = self.playback_snapshot();
        self.save_progress();
        {
            let mut x = self.x.borrow_mut();
            x.discovery = None;
            x.cast_target = None;
            x.convert.clear();
        }
        let volume = self.player().map(|p| p.volume());
        *self.player.borrow_mut() = None;
        *self.engine.borrow_mut() = None;
        let args = options::engine_args(&self.st.borrow().settings);
        let (args, reset_cache) = {
            let mut a = args;
            let reset = std::mem::take(&mut self.x.borrow_mut().reset_plugins);
            if reset {
                a.push("--reset-plugins-cache".into());
            }
            (a, reset)
        };
        match VlcEngine::load_with_args(&path, &args) {
            Ok(e) => *self.engine.borrow_mut() = Some(e),
            Err(err) => {
                self.toast(tr_fmt("libVLC could not restart: {}", &[&err.to_string()]));
                return;
            }
        }
        self.attach_engine();
        if let (Some(p), Some(v)) = (self.player(), volume) {
            p.set_volume(v);
        }
        self.start_cast_discovery();
        if let Some((idx, time, playing)) = snap {
            self.x.borrow_mut().start_override = Some((time, !playing));
            self.play_index(idx);
        }
        if reset_cache {
            self.toast(tr("Plugins rescanned"));
        }
    }

    /// Player notifications, polled by the UI timer.
    pub(super) fn poll_player_events(self: &Rc<Self>) {
        let Some(p) = self.player() else { return };
        while let Some(ev) = p.take_event() {
            match ev {
                PlayerEvent::Corked => {
                    let pause = self.st.borrow().settings.ob("play.cork");
                    if pause && p.state() == PlaybackState::Playing {
                        p.pause();
                        self.x.borrow_mut().cork_paused = true;
                        self.toast(tr("Paused: another application is using the audio"));
                    }
                }
                PlayerEvent::Uncorked => {
                    if std::mem::take(&mut self.x.borrow_mut().cork_paused) {
                        p.play();
                    }
                }
                PlayerEvent::Error if self.cast_name().is_some() => {
                    let name = self.cast_name().unwrap_or_default();
                    let resume = {
                        let mut x = self.x.borrow_mut();
                        x.cast_target = None;
                        x.cast_ip = None;
                        x.cast_from.take()
                    };
                    self.show_error(
                        &tr("Cast failed"),
                        &tr_fmt(
                            "Could not reach {}. Playback continues on this computer.",
                            &[&name],
                        ),
                    );
                    if let Some((idx, time)) = resume {
                        self.x.borrow_mut().start_override = Some((time, false));
                        self.play_index(idx);
                    }
                    self.refresh_toolbar();
                }
                PlayerEvent::Error => {
                    let name = self
                        .st
                        .borrow()
                        .current
                        .as_deref()
                        .map(library::display_name)
                        .unwrap_or_default();
                    self.show_error(
                        &tr("Playback error"),
                        &tr_fmt("libVLC could not open or decode \"{}\".", &[&name]),
                    );
                }
                PlayerEvent::VideoStarted => self.on_video_started(),
            }
        }
    }

    // --------------------------------------------------------- dialogs

    /// An error for the user: a dialog when "Show confirmation and error
    /// dialogs" is on, a toast otherwise.
    pub fn show_error(&self, title: &str, text: &str) {
        if self.st.borrow().settings.app.global.show_error_dialogs {
            self.queue_dialog(PendingDialog {
                id: None,
                event: DialogEvent::Error {
                    title: title.to_string(),
                    text: text.to_string(),
                },
            });
        } else {
            self.toast(format!("{title}: {text}"));
        }
    }

    fn on_vlc_dialog(&self, ev: DialogEvent) {
        let engine = self.engine.borrow();
        let Some(e) = engine.as_ref() else { return };
        match &ev {
            DialogEvent::Error { title, text } => {
                drop(engine);
                self.show_error(title, text);
            }
            DialogEvent::Question { id, title, .. }
                if !self.st.borrow().settings.ob("net.verify")
                    && title.to_lowercase().contains("insecure") =>
            {
                // "Verify certificates" is off: view, then accept for 24 hours.
                e.dialog_action(*id, 1);
            }
            DialogEvent::Login { id, .. }
            | DialogEvent::Question { id, .. }
            | DialogEvent::Progress { id, .. } => {
                let id = *id;
                drop(engine);
                self.queue_dialog(PendingDialog {
                    id: Some(id),
                    event: ev,
                });
            }
            DialogEvent::Update { id, position, text } => {
                let mut x = self.x.borrow_mut();
                if let Some(d) = x.dialogs.iter_mut().find(|d| d.id == Some(*id)) {
                    if let DialogEvent::Progress {
                        position: p,
                        text: t,
                        ..
                    } = &mut d.event
                    {
                        *p = *position;
                        if !text.is_empty() {
                            *t = text.clone();
                        }
                    }
                }
                drop(x);
                drop(engine);
                self.show_next_dialog();
            }
            DialogEvent::Cancel { id } => {
                e.dialog_dismiss(*id);
                drop(engine);
                self.x.borrow_mut().dialogs.retain(|d| d.id != Some(*id));
                self.show_next_dialog();
            }
        }
    }

    fn queue_dialog(&self, d: PendingDialog) {
        {
            let mut x = self.x.borrow_mut();
            if let DialogEvent::Error { title, text } = &d.event {
                // libVLC repeats some errors (e.g. a missing audio device)
                let key = format!("{title}\n{text}");
                let repeated = x.dialogs.iter().any(|q| {
                    matches!(&q.event, DialogEvent::Error { title: t, text: m } if format!("{t}\n{m}") == key)
                }) || x
                    .last_error
                    .as_ref()
                    .is_some_and(|(k, at)| *k == key && at.elapsed() < Duration::from_secs(60));
                x.last_error = Some((key, Instant::now()));
                if repeated {
                    return;
                }
            }
            x.dialogs.push(d);
        }
        self.show_next_dialog();
    }

    /// Show the oldest pending libVLC dialog (or close the popup).
    pub(super) fn show_next_dialog(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let v = w.global::<VlcDialog>();
        let first = self.x.borrow().dialogs.first().cloned();
        let Some(d) = first else {
            if a.get_dialog() == 8 {
                a.set_dialog(0);
            }
            return;
        };
        let (kind, title, text) = match &d.event {
            DialogEvent::Error { title, text } => (0, title, text),
            DialogEvent::Login { title, text, .. } => (1, title, text),
            DialogEvent::Question { title, text, .. } => (2, title, text),
            DialogEvent::Progress { title, text, .. } => (3, title, text),
            _ => return,
        };
        v.set_kind(kind);
        v.set_title(ss(title));
        v.set_text(ss(text));
        match &d.event {
            DialogEvent::Login {
                username,
                ask_store,
                ..
            } => {
                v.set_username(ss(username));
                v.set_password("".into());
                v.set_ask_store(*ask_store);
                v.set_cancel(ss(tr("Cancel")));
                v.set_action1(ss(tr("Log In")));
                v.set_action2("".into());
            }
            DialogEvent::Question {
                cancel,
                action1,
                action2,
                severity,
                ..
            } => {
                v.set_cancel(ss(cancel));
                v.set_action1(ss(action1));
                v.set_action2(ss(action2));
                v.set_critical(*severity > 0);
            }
            DialogEvent::Progress {
                position, cancel, ..
            } => {
                v.set_progress(*position);
                v.set_cancel(ss(cancel));
                v.set_action1("".into());
                v.set_action2("".into());
            }
            _ => {
                v.set_cancel("".into());
                v.set_action1(ss(tr("OK")));
                v.set_action2("".into());
                v.set_critical(true);
            }
        }
        if a.get_dialog() == 0 || a.get_dialog() == 8 {
            a.set_dialog(8);
        }
    }

    /// Answer of the VLC dialog popup: 0 cancel, 1/2 actions.
    pub(super) fn answer_vlc_dialog(&self, action: i32) {
        let d = {
            let mut x = self.x.borrow_mut();
            if x.dialogs.is_empty() {
                None
            } else {
                Some(x.dialogs.remove(0))
            }
        };
        if let Some(d) = d {
            let engine = self.engine.borrow();
            if let (Some(e), Some(id)) = (engine.as_ref(), d.id) {
                match (&d.event, action) {
                    (DialogEvent::Login { .. }, 1) => {
                        let w = self.ui();
                        let v = w.global::<VlcDialog>();
                        e.dialog_login(
                            id,
                            v.get_username().as_str(),
                            v.get_password().as_str(),
                            v.get_store(),
                        );
                    }
                    (DialogEvent::Question { .. }, 1 | 2) => e.dialog_action(id, action),
                    _ => e.dialog_dismiss(id),
                }
            }
        }
        self.ui().global::<AppState>().set_dialog(0);
        self.show_next_dialog();
    }
}

/// Copy decoded frames into the `frame` image of the UI (at most one pending
/// UI update at a time; libVLC overwrites the slot meanwhile).
fn install_frame_sink(p: &VlcPlayer) {
    type FrameSlot = Arc<Mutex<(Option<SharedPixelBuffer<Rgba8Pixel>>, bool)>>;
    let slot: FrameSlot = Arc::new(Mutex::new((None, false)));
    let epoch = p.frame_epoch.clone();
    p.set_video_callback(move |w, h, bytes| {
        if w == 0 || h == 0 || bytes.len() != w as usize * h as usize * 4 {
            return;
        }
        let Ok(mut s) = slot.lock() else { return };
        // One copy from VLC, then transfer ownership to the UI; replace stale frames.
        s.0 = Some(SharedPixelBuffer::clone_from_slice(bytes, w, h));
        if !s.1 {
            s.1 = true;
            let slot3 = slot.clone();
            let epoch2 = epoch.clone();
            let serial = epoch.load(std::sync::atomic::Ordering::SeqCst);
            if slint::invoke_from_event_loop(move || {
                let frame = {
                    let Ok(mut s) = slot3.lock() else { return };
                    s.1 = false;
                    s.0.take()
                };
                let Some(frame) = frame else { return };
                with_app(|c| {
                    let Some(player) = c.player() else { return };
                    if !Arc::ptr_eq(&player.frame_epoch, &epoch2)
                        || epoch2.load(std::sync::atomic::Ordering::SeqCst) != serial
                    {
                        return;
                    }
                    let w = c.ui();
                    let a = w.global::<AppState>();
                    if a.get_has_media() && (!a.get_audio_mode() || a.get_visualizer()) {
                        a.set_frame(Image::from_rgba8(frame));
                        a.set_has_frame(true);
                    }
                });
            })
            .is_err()
            {
                s.1 = false;
            }
        }
    });
}
