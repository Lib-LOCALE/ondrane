//! Window and desktop behaviour: drag & drop of files, window level,
//! decorations, geometry, fullscreen screen, video wallpaper, auto-resize to
//! the video, system tray, notifications, screen-saver inhibition, the web
//! remote, Last.fm scrobbling, single instance and the update check.

use super::*;
use crate::remote::RemoteServer;
use crate::scrobbler::{Scrobbler, Track};
use crate::system::{self, Inhibitor, Tray, TrayCmd, TrayLabels};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};

/// Background services owned by the controller.
#[derive(Default)]
pub struct Services {
    pub remote: Option<RemoteServer>,
    pub scrobbler: Option<Scrobbler>,
    /// Track being listened to, milliseconds actually played, submitted?
    pub scrobble: Option<(Track, i64, bool)>,
    pub inhibitor: Option<Inhibitor>,
    pub tray: Option<Tray>,
    pub hidden_to_tray: bool,
    pub popup_for: Option<String>,
}

impl Controller {
    fn services(&self) -> std::cell::RefMut<'_, Services> {
        std::cell::RefMut::map(self.x.borrow_mut(), |x| {
            x.services.get_or_insert_with(Services::default)
        })
    }

    /// Start everything that depends on settings (called once at startup and
    /// again when one of these settings changes).
    pub(super) fn start_services(self: &Rc<Self>) {
        self.apply_window_prefs();
        self.apply_remote();
        self.apply_scrobbler();
        self.apply_tray();
        self.start_cast_discovery();
    }

    pub(super) fn install_window_hooks(self: &Rc<Self>) {
        let w = self.ui();
        // Files dragged from the file manager (winit DroppedFile events).
        w.window().on_winit_window_event(|_, ev| {
            match ev {
                winit::event::WindowEvent::HoveredFile(_) => {
                    with_app(|c| c.ui().global::<AppState>().set_drop_hover(true));
                }
                winit::event::WindowEvent::HoveredFileCancelled => {
                    with_app(|c| c.ui().global::<AppState>().set_drop_hover(false));
                }
                winit::event::WindowEvent::DroppedFile(path) => {
                    let p = path.to_string_lossy().to_string();
                    with_app(|c| c.on_file_dropped(p));
                }
                winit::event::WindowEvent::Focused(true) => {
                    with_app(|c| c.services().popup_for = None);
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    /// Dropped files are collected for a moment so a multi-file drop becomes
    /// one queue (several DroppedFile events arrive in a row).
    fn on_file_dropped(self: &Rc<Self>, path: String) {
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_drop_hover(false);
        let first = {
            let mut st = self.st.borrow_mut();
            st.dropped.push(path);
            st.dropped.len() == 1
        };
        if first {
            let me = self.clone();
            slint::Timer::single_shot(Duration::from_millis(150), move || {
                let files = std::mem::take(&mut me.st.borrow_mut().dropped);
                // shift-drop or "add to queue" preference appends
                let append =
                    me.st.borrow().settings.ob("lib.enqueue") && me.st.borrow().current.is_some();
                if a_queue_open(&me) || append {
                    me.open_targets(files, true);
                } else {
                    me.open_targets(files, false);
                }
                me.ui().global::<AppState>().set_view(0);
            });
        }
    }

    /// Always on top, native decorations, video wallpaper.
    pub(super) fn apply_window_prefs(&self) {
        let w = self.ui();
        let st = self.st.borrow();
        let s = &st.settings;
        let on_top = s.app.interface.always_on_top;
        let wallpaper = s.media.video.video_wallpaper_mode;
        let decorations = s.ob("ui.decorations");
        w.global::<AppState>().set_native_decorations(decorations);
        w.window().with_winit_window(|ww| {
            use winit::window::WindowLevel;
            ww.set_window_level(if wallpaper {
                WindowLevel::AlwaysOnBottom
            } else if on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
            ww.set_decorations(decorations && !wallpaper);
        });
        if wallpaper {
            let a = w.global::<AppState>();
            a.set_hide_ui(true);
            if !a.get_fullscreen() {
                w.window().set_fullscreen(true);
                a.set_fullscreen(true);
            }
        }
    }

    /// Fullscreen on the screen chosen in "Open fullscreen on".
    pub(super) fn set_fullscreen(&self, fs: bool) {
        let w = self.ui();
        let screen = self.st.borrow().settings.oi("ui.fs-screen");
        let mut handled = false;
        if fs && screen > 0 {
            handled = w
                .window()
                .with_winit_window(|ww| {
                    let m = ww.available_monitors().nth(screen as usize - 1)?;
                    ww.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(m))));
                    Some(())
                })
                .flatten()
                .is_some();
        }
        if !handled {
            w.window().set_fullscreen(fs);
        } else {
            w.window().set_fullscreen(true);
        }
        let a = w.global::<AppState>();
        a.set_fullscreen(fs);
        a.set_fs_controls(self.st.borrow().settings.ob("ui.fs-controls"));
    }

    /// Names of the connected screens for "Open fullscreen on".
    pub(super) fn screen_names(&self) -> Vec<String> {
        let mut v = vec![tr("Current screen")];
        let names = self
            .ui()
            .window()
            .with_winit_window(|ww| {
                ww.available_monitors()
                    .enumerate()
                    .map(|(i, m)| {
                        let size = m.size();
                        format!(
                            "{} {} — {} ({}×{})",
                            tr("Screen"),
                            i + 1,
                            m.name().unwrap_or_default(),
                            size.width,
                            size.height
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        v.extend(names);
        v
    }

    /// Restore the saved window geometry ("Remember window size and position").
    pub(super) fn restore_geometry(&self) -> bool {
        let g = {
            let st = self.st.borrow();
            if !st.settings.ob("ui.remember-geometry") {
                return false;
            }
            st.settings.os("ui.geometry")
        };
        let v: Vec<i32> = g.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        let [x, y, wdt, hgt] = v[..] else {
            return false;
        };
        if wdt < 400 || hgt < 300 {
            return false;
        }
        let w = self.ui();
        w.window()
            .set_size(slint::PhysicalSize::new(wdt as u32, hgt as u32));
        w.window().set_position(slint::PhysicalPosition::new(x, y));
        true
    }

    /// Keep the window inside its screen (first start on a small display,
    /// or a geometry saved on a screen that is gone).
    pub(super) fn fit_to_screen(&self) {
        let w = self.ui();
        let fit = w.window().with_winit_window(|ww| {
            let m = ww
                .current_monitor()
                .or_else(|| ww.primary_monitor())
                .or_else(|| ww.available_monitors().next())?;
            let (ms, mp, ws) = (m.size(), m.position(), ww.outer_size());
            if ws.width <= ms.width && ws.height <= ms.height {
                return None;
            }
            let wdt = ws.width.min(ms.width * 92 / 100);
            let hgt = ws.height.min(ms.height * 92 / 100);
            let x = mp.x + (ms.width - wdt) as i32 / 2;
            let y = mp.y + (ms.height - hgt) as i32 / 2;
            Some((x, y, wdt, hgt))
        });
        if let Some(Some((x, y, wdt, hgt))) = fit {
            w.window().set_size(slint::PhysicalSize::new(wdt, hgt));
            w.window().set_position(slint::PhysicalPosition::new(x, y));
        }
    }

    pub(super) fn save_geometry(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        if a.get_fullscreen() || w.window().is_maximized() {
            return;
        }
        let (pos, size) = (w.window().position(), w.window().size());
        let mut st = self.st.borrow_mut();
        if st.settings.ob("ui.remember-geometry") {
            st.settings.set_extra(
                "ui.geometry",
                format!("{},{},{},{}", pos.x, pos.y, size.width, size.height),
            );
        }
    }

    /// "Resize window to fit video": Never / When the first video starts / Always.
    pub(super) fn on_video_started(&self) {
        let mode = self.st.borrow().settings.oi("ui.resize");
        let first = !std::mem::replace(&mut self.x.borrow_mut().resized_once, true);
        if mode == 0 || (mode == 1 && !first) {
            return;
        }
        let w = self.ui();
        if w.global::<AppState>().get_fullscreen() || w.window().is_maximized() {
            return;
        }
        let Some((vw, vh)) = self.player().map(|p| p.video_dimensions()) else {
            return;
        };
        if vw == 0 || vh == 0 {
            return;
        }
        let scale = w.window().scale_factor();
        let (mut lw, mut lh) = (vw as f32 / scale, vh as f32 / scale);
        let max = w
            .window()
            .with_winit_window(|ww| {
                ww.current_monitor().map(|m| {
                    let s = m.size();
                    (s.width as f32 / scale * 0.9, s.height as f32 / scale * 0.85)
                })
            })
            .flatten()
            .unwrap_or((1600.0, 900.0));
        let k = (max.0 / lw).min(max.1 / lh).min(1.0);
        lw *= k;
        lh *= k;
        // title bar + control bar of the Figma player
        w.window()
            .set_size(slint::LogicalSize::new(lw.max(640.0), lh + 136.0));
    }

    // -------------------------------------------------------------- tray

    pub(super) fn apply_tray(self: &Rc<Self>) {
        let want = self.st.borrow().settings.ob("ui.tray");
        let has = self.services().tray.is_some();
        if want == has {
            return;
        }
        if !want {
            self.services().tray = None;
            if std::mem::take(&mut self.services().hidden_to_tray) {
                let _ = self.ui().window().show();
            }
            return;
        }
        let labels = TrayLabels {
            show: tr("Show / Hide VLC"),
            play_pause: tr("Play / Pause"),
            next: tr("Next"),
            previous: tr("Previous"),
            quit: tr("Quit Ondrane"),
        };
        let tray = Tray::new(labels, |cmd| post(move |c| c.on_tray(cmd)));
        if tray.is_none() {
            self.toast(tr("The system tray is not available on this desktop"));
        }
        self.services().tray = tray;
    }

    fn on_tray(self: &Rc<Self>, cmd: TrayCmd) {
        match cmd {
            TrayCmd::Toggle => self.toggle_tray_window(),
            TrayCmd::PlayPause => self.toggle_play(),
            TrayCmd::Next => self.next(false),
            TrayCmd::Previous => self.prev(),
            TrayCmd::Quit => self.command("quit"),
        }
    }

    fn toggle_tray_window(&self) {
        let w = self.ui();
        let hidden = self.services().hidden_to_tray;
        if hidden {
            let _ = w.window().show();
            w.window().set_minimized(false);
            self.services().hidden_to_tray = false;
        } else {
            let _ = w.window().hide();
            self.services().hidden_to_tray = true;
        }
    }

    /// Minimize: hide to the tray when the tray icon is shown.
    pub(super) fn minimize(&self) {
        if self.services().tray.is_some() {
            let _ = self.ui().window().hide();
            self.services().hidden_to_tray = true;
        } else {
            self.ui().window().set_minimized(true);
        }
    }

    pub(super) fn start_minimized(&self) -> bool {
        let s = &self.st.borrow().settings;
        s.ob("ui.tray") && s.ob("ui.tray-minimized") && self.services().tray.is_some()
    }

    // ------------------------------------------------------------ remote

    pub(super) fn apply_remote(&self) {
        let (on, port, pw) = {
            let s = &self.st.borrow().settings;
            (
                s.ob("remote.web"),
                s.oi("remote.port").clamp(1024, 65535) as u16,
                s.os("remote.password"),
            )
        };
        let running = self.services().remote.as_ref().map(|r| r.port);
        if !on {
            self.services().remote = None;
            return;
        }
        if running == Some(port) {
            return;
        }
        self.services().remote = None;
        match RemoteServer::start(port, pw, |cmd| post(move |c| c.remote_command(&cmd))) {
            Ok(r) => {
                self.services().remote = Some(r);
                self.toast(tr_fmt(
                    "Web remote: http://{}:{}/",
                    &[&extras::local_ip(), &port.to_string()],
                ));
            }
            Err(e) => self.show_error(
                &tr("Web remote control"),
                &tr_fmt(
                    "Port {} is not available ({})",
                    &[&port.to_string(), &e.to_string()],
                ),
            ),
        }
    }

    fn remote_command(self: &Rc<Self>, cmd: &str) {
        if let Some(v) = cmd.strip_prefix("seek:") {
            if let (Some(p), Ok(f)) = (self.player(), v.parse::<f32>()) {
                p.seek_position(f);
            }
        } else if let Some(v) = cmd.strip_prefix("volume:") {
            self.set_volume(v.parse().unwrap_or(80));
        } else {
            self.command(cmd);
        }
    }

    // ---------------------------------------------------------- last.fm

    pub(super) fn apply_scrobbler(&self) {
        let (on, url, user, pass) = {
            let s = &self.st.borrow().settings;
            (
                s.ob("lastfm.enable"),
                s.os("lastfm.url"),
                s.os("lastfm.user"),
                s.os("lastfm.pass"),
            )
        };
        let url = if url.trim().is_empty() {
            "post.audioscrobbler.com".to_string()
        } else {
            url
        };
        self.services().scrobbler = (on && !user.is_empty()).then(|| {
            Scrobbler::start(
                format!("http://{}", url.trim_start_matches("http://")),
                user,
                pass,
            )
        });
    }

    /// Called once the tags of an audio track are known.
    pub(super) fn scrobble_start(&self, artist: &str, title: &str, album: &str, length_ms: i64) {
        let mut sv = self.services();
        if sv.scrobbler.is_none() || artist.is_empty() || title.is_empty() {
            sv.scrobble = None;
            return;
        }
        let t = Track {
            artist: artist.to_string(),
            title: title.to_string(),
            album: album.to_string(),
            length_s: (length_ms / 1000).max(0) as u64,
            started_unix: library::now_secs(),
        };
        if let Some(s) = &sv.scrobbler {
            s.now_playing(t.clone());
        }
        sv.scrobble = Some((t, 0, false));
    }

    // ------------------------------------------------------------- tick

    /// Services polled by the UI timer (every 200 ms).
    pub(super) fn tick_services(self: &Rc<Self>, playing: bool) {
        let w = self.ui();
        let a = w.global::<AppState>();
        // tray (Windows / macOS poll their menu events)
        let cmd = self.services().tray.as_ref().and_then(|t| t.poll());
        if let Some(cmd) = cmd {
            self.on_tray(cmd);
        }
        // keep the screen awake while a video plays
        let want_awake = playing
            && !a.get_audio_mode()
            && self.st.borrow().settings.media.video.disable_screensaver;
        let awake = self.services().inhibitor.is_some();
        if want_awake != awake {
            self.services().inhibitor = want_awake.then(Inhibitor::new);
        }
        // scrobbling: half the track or 4 minutes
        if playing {
            let mut sv = self.services();
            let mut submit = None;
            if let Some((t, played, done)) = sv.scrobble.as_mut() {
                *played += 200;
                let need = ((t.length_s as i64 * 1000) / 2).min(240_000);
                if !*done && t.length_s >= 30 && *played >= need {
                    *done = true;
                    submit = Some(t.clone());
                }
            }
            if let (Some(t), Some(s)) = (submit, &sv.scrobbler) {
                s.submit(t);
            }
        }
        // web remote status
        let status = self.services().remote.as_ref().map(|r| r.status.clone());
        if let Some(status) = status {
            if let Ok(mut s) = status.lock() {
                s.title = a.get_media_name().to_string();
                s.playing = playing;
                s.volume = a.get_volume();
                if let Some(p) = self.player() {
                    s.time_ms = p.time_ms().max(0);
                    s.length_ms = p.length_ms().max(0);
                }
            }
        }
    }

    /// "Show a popup when the track changes".
    pub(super) fn track_popup(&self, title: &str, line: &str) {
        let (mode, secs) = {
            let s = &self.st.borrow().settings;
            (s.oi("ui.popup"), s.of("ui.popup-duration"))
        };
        let w = self.ui();
        let away = self.services().hidden_to_tray || w.window().is_minimized();
        if mode == 2 || (mode == 1 && away) {
            if self.services().popup_for.as_deref() == Some(title) {
                return;
            }
            self.services().popup_for = Some(title.to_string());
            let body = if line.is_empty() || line == title {
                tr("Now playing")
            } else {
                line.to_string()
            };
            system::notify(title, &body, (secs * 1000.0) as u32);
        }
    }

    /// Background update check ("Check for updates automatically").
    pub(super) fn check_updates(&self) {
        if !self.st.borrow().settings.app.global.check_updates {
            return;
        }
        background(system::latest_release, |c, tag| {
            if let Some(tag) = tag {
                if system::is_newer(&tag, env!("CARGO_PKG_VERSION")) {
                    c.toast(tr_fmt("Ondrane {} is available", &[&tag]));
                    system::notify("Ondrane", &tr_fmt("Ondrane {} is available", &[&tag]), 8000);
                }
            }
        });
    }
}

fn a_queue_open(c: &Controller) -> bool {
    c.ui().global::<AppState>().get_queue_open()
}

/// Second launches hand their files to this window.
pub fn listen_single_instance() {
    crate::instance::listen(|req| {
        post(move |c| {
            let w = c.ui();
            let _ = w.window().show();
            w.window().set_minimized(false);
            c.services().hidden_to_tray = false;
            if !req.targets.is_empty() {
                c.open_targets(req.targets, req.enqueue);
                w.global::<AppState>().set_view(0);
            }
        })
    });
}
