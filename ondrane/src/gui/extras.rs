//! "Open Disc", "Open Capture Device", "Convert", recording, streaming out and
//! casting — all built on libVLC 3 inputs (dvd://, v4l2://, screen://…) and
//! stream-output chains (`:sout=#transcode{…}:std{…}`), plus the renderer
//! discoverer for Chromecast devices.

use super::lifecycle::PendingDialog;
use super::*;
use crate::engine::{RendererDiscovery, RendererItem};
use crate::options::{self, Apply};

/// Runtime state that is not part of the persisted settings.
#[derive(Default)]
pub struct Extras {
    pub pending_apply: Option<(Apply, Instant)>,
    /// Restart the next media at this time (ms), paused or not.
    pub start_override: Option<(i64, bool)>,
    pub cork_paused: bool,
    pub reset_plugins: bool,
    pub dialogs: Vec<PendingDialog>,
    pub last_error: Option<(String, Instant)>,
    pub discovery: Option<RendererDiscovery>,
    pub cast_target: Option<Arc<RendererItem>>,
    /// Chromecast reached by its address: found by our own mDNS search or
    /// typed in the settings (label, address, port).
    pub cast_ip: Option<(String, String, u16)>,
    /// Chromecasts found by `mdns::discover`, and when the last search started.
    pub lan_casts: Vec<crate::mdns::CastDevice>,
    pub lan_scan: Option<Instant>,
    /// Where playback was when casting started, to resume locally if the
    /// device cannot be reached.
    pub cast_from: Option<(usize, i64)>,
    pub recording: Option<PathBuf>,
    pub streaming: Option<String>,
    pub convert: Vec<ConvertJob>,
    pub disc_devices: Vec<String>,
    pub capture_devices: Vec<(String, String)>,
    #[cfg(target_os = "linux")]
    pub screen_capture: Option<crate::screencast::Capture>,
    #[cfg(target_os = "linux")]
    pub capture_generation: u64,
    pub resized_once: bool,
    pub services: Option<super::window::Services>,
}

pub struct ConvertJob {
    pub player: VlcPlayer,
    pub source: String,
    pub dest: PathBuf,
    pub done: bool,
    pub failed: bool,
    pub started: Instant,
}

/// (label, transcode chain or "" to remux, mux, extension, audio only)
pub const CONVERT_PROFILES: [(&str, &str, &str, &str, bool); 8] = [
    (
        "Video – H.264 + AAC (MP4)",
        "vcodec=h264,venc={venc},acodec=mp4a,ab=192,channels=2,samplerate=48000,scodec=none",
        "mp4",
        "mp4",
        false,
    ),
    (
        "Video – H.265 + AAC (MP4)",
        "vcodec=hevc,acodec=mp4a,ab=192,channels=2,samplerate=48000,scodec=none",
        "mp4",
        "mp4",
        false,
    ),
    (
        "Video – VP8 + Vorbis (WebM)",
        "vcodec=VP80,vb=2000,acodec=vorb,ab=160,channels=2,samplerate=48000",
        "webm",
        "webm",
        false,
    ),
    (
        "Video – MPEG-2 + MPGA (TS)",
        "vcodec=mp2v,vb=4000,acodec=mpga,ab=192,channels=2",
        "ts",
        "ts",
        false,
    ),
    (
        "Video – Remux without re-encoding (MKV)",
        "",
        "mkv",
        "mkv",
        false,
    ),
    (
        "Audio – MP3",
        "acodec=mp3,ab=192,channels=2,samplerate=44100",
        "raw",
        "mp3",
        true,
    ),
    (
        "Audio – FLAC",
        "acodec=flac,channels=2,samplerate=44100",
        "raw",
        "flac",
        true,
    ),
    (
        "Audio – OGG Vorbis",
        "acodec=vorb,ab=160,channels=2,samplerate=44100",
        "ogg",
        "ogg",
        true,
    ),
];

/// Quote a path for a stream-output chain.
fn sout_path(p: &Path) -> String {
    let s = p
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{s}'")
}

/// IP address of this computer on the local network (for stream URLs).
pub fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

/// Optical drives of this computer.
pub fn optical_drives() -> Vec<String> {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(rd) = std::fs::read_dir("/dev") {
            let mut v: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with("sr") && n[2..].chars().all(|c| c.is_ascii_digit()))
                .map(|n| format!("/dev/{n}"))
                .collect();
            v.sort();
            out.extend(v);
        }
        if out.is_empty() && Path::new("/dev/cdrom").exists() {
            out.push("/dev/cdrom".into());
        }
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        for letter in b'A'..=b'Z' {
            let root: Vec<u16> = format!("{}:\\", letter as char)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            // DRIVE_CDROM
            if unsafe { GetDriveTypeW(root.as_ptr()) } == 5 {
                out.push(format!("{}:", letter as char));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                let p = e.path();
                if p.join("VIDEO_TS").is_dir() || p.join("BDMV").is_dir() {
                    out.push(p.to_string_lossy().to_string());
                }
            }
        }
    }
    out
}

/// Video capture devices: (MRL, label).
#[allow(clippy::vec_init_then_push)] // contents depend on the target OS
pub fn capture_devices() -> Vec<(String, String)> {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(rd) = std::fs::read_dir("/sys/class/video4linux") {
            let mut v: Vec<(String, String)> = rd
                .flatten()
                .map(|e| {
                    let node = e.file_name().to_string_lossy().to_string();
                    let name = std::fs::read_to_string(e.path().join("name"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| node.clone());
                    (
                        format!("v4l2:///dev/{node}"),
                        format!("{name} (/dev/{node})"),
                    )
                })
                .collect();
            v.sort();
            out.extend(v);
        }
    }
    #[cfg(target_os = "windows")]
    out.push(("dshow://".into(), crate::i18n::tr("Default camera")));
    #[cfg(target_os = "macos")]
    out.push((
        "avfoundation://default".into(),
        crate::i18n::tr("Default camera"),
    ));
    out
}

impl Controller {
    /// Extra media options of the current session: recording, streaming.
    pub(super) fn session_options(&self, target: &str) -> Vec<String> {
        let x = self.x.borrow();
        let st = self.st.borrow();
        let s = &st.settings;
        let mut o = Vec::new();
        if let Some(dir) = &x.recording {
            let copy = s.ob("rec.copy");
            let capture_copy = copy && target.starts_with("fd://");
            let stem = if target.starts_with("fd://") {
                "screen-capture".to_string()
            } else {
                library::pretty_title(target).replace(['/', '\\', ':'], "_")
            };
            let file = dir.join(format!(
                "{}-{}.{}",
                stem,
                library::now_secs(),
                if capture_copy {
                    "mkv"
                } else if copy {
                    "ts"
                } else {
                    "mp4"
                }
            ));
            let out = if copy {
                format!(
                    "std{{access=file,mux={},dst={}}}",
                    if capture_copy { "mkv" } else { "ts" },
                    sout_path(&file)
                )
            } else {
                format!(
                    "transcode{{vcodec=h264,venc={},acodec=mp4a,ab=192,channels=2}}:std{{access=file,mux=mp4,dst={}}}",
                    options::x264_venc(s),
                    sout_path(&file)
                )
            };
            o.push(format!(
                ":sout=#duplicate{{dst=display,dst=\"{}\"}}",
                out.replace('"', "\\\"")
            ));
            o.push(":sout-keep".into());
        } else if let Some(url) = &x.streaming {
            let port = url
                .rsplit(':')
                .next()
                .and_then(|p| p.trim_end_matches('/').parse::<u16>().ok())
                .unwrap_or(8090);
            let net = format!("std{{access=http,mux=ts,dst=:{port}/}}");
            let chain = if s.ob("rec.copy") {
                net
            } else {
                format!(
                    "transcode{{vcodec=h264,venc={},acodec=mp4a,ab=192,channels=2,soverlay}}:{net}",
                    options::x264_venc(s)
                )
            };
            if s.ob("stream.local") {
                o.push(format!(
                    ":sout=#duplicate{{dst=display,dst=\"{}\"}}",
                    chain.replace('"', "\\\"")
                ));
            } else {
                o.push(format!(":sout=#{chain}"));
            }
            o.push(":sout-keep".into());
            if !s.ob("stream.audio") {
                o.push(":no-sout-audio".into());
            }
            if !s.ob("stream.video") {
                o.push(":no-sout-video".into());
            }
            if s.ob("stream.subs") {
                o.push(":sout-all".into());
            } else {
                o.push(":no-sout-spu".into());
            }
        }
        if let (Some((_, ip, port)), None, None) = (&x.cast_ip, &x.recording, &x.streaming) {
            o.push(format!(":sout=#chromecast{{ip={ip},port={port}}}"));
            o.push(":demux-filter=demux_chromecast".into());
        }
        o
    }

    pub(super) fn toggle_recording(self: &Rc<Self>) {
        if self.st.borrow().current.is_none() {
            return self.toast(tr("Play something to record it"));
        }
        let was = self.x.borrow().recording.clone();
        if let Some(dir) = was {
            self.x.borrow_mut().recording = None;
            self.reload_media();
            self.toast(tr_fmt("Recording saved in {}", &[&library::tilde(&dir)]));
        } else {
            let dir = self
                .st
                .borrow()
                .settings
                .system
                .recording
                .recording_dir
                .clone();
            let _ = std::fs::create_dir_all(&dir);
            {
                let mut x = self.x.borrow_mut();
                x.recording = Some(dir);
                x.streaming = None;
            }
            self.reload_media();
            self.toast(tr("Recording…"));
        }
        self.refresh_toolbar();
    }

    // -------------------------------------------------------------- disc

    pub(super) fn open_disc_dialog(&self) {
        let drives = optical_drives();
        let w = self.ui();
        let f = w.global::<Forms>();
        let preferred = self.st.borrow().settings.os("play.drive");
        let mut list = drives.clone();
        if !preferred.is_empty() && !list.contains(&preferred) {
            list.insert(0, preferred.clone());
        }
        f.set_disc_devices(model(list.iter().map(ss).collect()));
        let device = if !preferred.is_empty() {
            preferred
        } else {
            list.first().cloned().unwrap_or_default()
        };
        f.set_disc_device(ss(&device));
        self.x.borrow_mut().disc_devices = list;
        w.global::<AppState>().set_dialog(9);
    }

    /// Build the disc MRL from the "Open Disc" form and play it.
    pub(super) fn play_disc(self: &Rc<Self>) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let device = f.get_disc_device().trim().to_string();
        let kind = f.get_disc_type();
        let menus = !f.get_disc_no_menus();
        let title = f.get_disc_title().max(0);
        let chapter = f.get_disc_chapter().max(0);
        let dev = if device.is_empty() {
            String::new()
        } else {
            #[cfg(target_os = "windows")]
            let d = device.trim_end_matches('\\').to_string();
            #[cfg(not(target_os = "windows"))]
            let d = device.clone();
            d
        };
        let pos = match (title, chapter) {
            (0, _) => String::new(),
            (t, 0) => format!("#{t}"),
            (t, c) => format!("#{t}:{c}"),
        };
        let mrl = match kind {
            0 if menus => format!("dvd://{dev}{pos}"),
            0 => format!("dvdsimple://{dev}{pos}"),
            1 => format!("bluray://{dev}"),
            2 => {
                if title > 0 {
                    format!("cdda://{dev}#{title}")
                } else {
                    format!("cdda://{dev}")
                }
            }
            _ => format!("vcd://{dev}{pos}"),
        };
        if !device.is_empty() {
            self.st
                .borrow_mut()
                .settings
                .set_extra("play.drive", device);
            self.save_settings();
        }
        w.global::<AppState>().set_dialog(0);
        self.open_targets(vec![mrl], false);
    }

    // ----------------------------------------------------------- capture

    pub(super) fn open_capture_dialog(&self) {
        let devices = capture_devices();
        let w = self.ui();
        let f = w.global::<Forms>();
        let mut labels: Vec<SharedString> = devices.iter().map(|d| ss(&d.1)).collect();
        if labels.is_empty() {
            labels.push(ss(tr("No camera found")));
        }
        f.set_capture_devices(model(labels));
        f.set_capture_device(0);
        self.x.borrow_mut().capture_devices = devices;
        w.global::<AppState>().set_dialog(10);
    }

    pub(super) fn play_capture(self: &Rc<Self>) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let screen = f.get_capture_mode() == 1;
        #[cfg(target_os = "linux")]
        if screen && crate::system::is_wayland_session() {
            if self.player().is_none() {
                return self.playback_unavailable();
            }
            let fps = f.get_capture_fps().clamp(1.0, 60.0) as u32;
            let audio = f.get_capture_audio();
            self.stop();
            let generation = self.x.borrow().capture_generation;
            w.global::<AppState>().set_dialog(0);
            self.toast(tr(
                "Choose a screen or window in the desktop sharing dialog.",
            ));
            std::thread::spawn(move || {
                let result = crate::screencast::start(fps, audio);
                post(move |c| {
                    if c.x.borrow().capture_generation != generation {
                        return;
                    }
                    match result {
                        Ok(Some(capture)) => {
                            let mrl = capture.mrl();
                            c.x.borrow_mut().screen_capture = Some(capture);
                            c.begin_capture(mrl, vec![], true);
                        }
                        Ok(None) => c.toast(tr("Screen sharing cancelled")),
                        Err(e) => c.show_error(&tr("Screen capture"), &e),
                    }
                });
            });
            return;
        }
        let with_audio = f.get_capture_audio();
        let mut opts: Vec<String> = Vec::new();
        let mrl = if screen {
            opts.push(format!(
                ":screen-fps={}",
                f.get_capture_fps().clamp(1.0, 60.0)
            ));
            "screen://".to_string()
        } else {
            let devs = self.x.borrow().capture_devices.clone();
            let Some((mrl, _)) = devs.get(f.get_capture_device().max(0) as usize).cloned() else {
                return self.toast(tr("No camera found"));
            };
            mrl
        };
        if with_audio {
            #[cfg(target_os = "linux")]
            opts.push(":input-slave=pulse://".into());
            #[cfg(target_os = "windows")]
            if screen {
                opts.push(":input-slave=dshow://".into());
            }
        }
        self.stop();
        self.begin_capture(mrl, opts, screen);
    }

    fn begin_capture(self: &Rc<Self>, mrl: String, opts: Vec<String>, screen: bool) {
        let w = self.ui();
        w.global::<AppState>().set_dialog(0);
        let Some(p) = self.player() else { return };
        // capture inputs are played directly (not stored in history)
        {
            let mut st = self.st.borrow_mut();
            st.queue = vec![mrl.clone()];
            st.index = Some(0);
            st.current = Some(mrl.clone());
            st.ended = false;
        }
        let mut all = options::media_options(&self.st.borrow().settings, true);
        all.extend(opts);
        if mrl.starts_with("fd://") {
            // VLC 3's native MKV demuxer expects seeks; FFmpeg handles the live pipe.
            all.push(":demux=avformat".into());
            all.push(":file-caching=100".into());
        }
        all.push(":live-caching=300".into());
        if p.open_with_options(&mrl, &all).is_ok() {
            p.play();
            let a = w.global::<AppState>();
            a.set_has_media(true);
            a.set_audio_mode(false);
            a.set_view(0);
            let name = if screen {
                tr("Screen capture")
            } else {
                tr("Capture device")
            };
            a.set_media_name(ss(&name));
            a.set_window_title(ss(&name));
            self.refresh_queue();
        }
    }

    // ----------------------------------------------------------- convert

    pub(super) fn open_convert_dialog(&self, files: Vec<String>) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let current: Vec<String> = if files.is_empty() {
            self.st
                .borrow()
                .current
                .clone()
                .filter(|c| !library::is_url(c))
                .into_iter()
                .collect()
        } else {
            files
        };
        f.set_convert_files(model(current.iter().map(ss).collect()));
        f.set_convert_profiles(model(
            CONVERT_PROFILES.iter().map(|p| ss(tr(p.0))).collect(),
        ));
        if f.get_convert_dest().is_empty() {
            let dir = self
                .st
                .borrow()
                .settings
                .system
                .recording
                .recording_dir
                .clone();
            f.set_convert_dest(ss(library::tilde(&dir)));
        }
        self.refresh_convert_jobs();
        w.global::<AppState>().set_dialog(11);
    }

    pub(super) fn start_convert(self: &Rc<Self>) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let files: Vec<String> = f
            .get_convert_files()
            .iter()
            .map(|s| s.to_string())
            .collect();
        if files.is_empty() {
            return self.toast(tr("Add files to convert"));
        }
        let profile = CONVERT_PROFILES
            [(f.get_convert_profile().max(0) as usize).min(CONVERT_PROFILES.len() - 1)];
        let dest_dir = library::expand_tilde(f.get_convert_dest().trim());
        let _ = std::fs::create_dir_all(&dest_dir);
        let engine = self.engine.borrow();
        let Some(e) = engine.as_ref() else { return };
        let venc = options::x264_venc(&self.st.borrow().settings);
        for src in files {
            let stem = Path::new(&src)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "converted".into());
            let mut dest = dest_dir.join(format!("{stem}.{}", profile.3));
            let mut n = 1;
            while dest.exists() || dest.to_string_lossy() == src {
                dest = dest_dir.join(format!("{stem} ({n}).{}", profile.3));
                n += 1;
            }
            let std_out = format!(
                "std{{access=file,mux={},dst={}}}",
                profile.2,
                sout_path(&dest)
            );
            let chain = if profile.1.is_empty() {
                format!(":sout=#{std_out}")
            } else {
                format!(
                    ":sout=#transcode{{{}}}:{std_out}",
                    profile.1.replace("{venc}", &venc)
                )
            };
            let Ok(player) = e.create_player_with(false) else {
                continue;
            };
            let mut opts = vec![chain, ":sout-keep".to_string(), ":no-sout-spu".into()];
            if profile.4 {
                opts.push(":no-sout-video".into());
            }
            if player.open_with_options(&src, &opts).is_err() {
                continue;
            }
            // no set_volume(0) here: the stream output plays nothing locally,
            // and on Windows the volume is shared with the main player
            player.play();
            self.x.borrow_mut().convert.push(ConvertJob {
                player,
                source: src,
                dest,
                done: false,
                failed: false,
                started: Instant::now(),
            });
        }
        drop(engine);
        f.set_convert_files(model(vec![]));
        self.refresh_convert_jobs();
    }

    /// Progress of the conversions, polled by the UI timer.
    pub(super) fn poll_convert(&self) {
        let mut changed = false;
        let mut finished = Vec::new();
        {
            let mut x = self.x.borrow_mut();
            for j in x.convert.iter_mut().filter(|j| !j.done) {
                match j.player.state() {
                    PlaybackState::Ended => {
                        j.done = true;
                        finished.push(j.dest.clone());
                    }
                    PlaybackState::Error => {
                        j.done = true;
                        j.failed = true;
                    }
                    PlaybackState::Stopped if j.started.elapsed() > Duration::from_secs(3) => {
                        j.done = true;
                        finished.push(j.dest.clone());
                    }
                    _ => {}
                }
                changed = true;
            }
            for j in x.convert.iter().filter(|j| j.done) {
                j.player.stop();
            }
        }
        for f in finished {
            self.toast(tr_fmt("Converted: {}", &[&library::tilde(&f)]));
        }
        if changed {
            self.refresh_convert_jobs();
        }
    }

    fn refresh_convert_jobs(&self) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let x = self.x.borrow();
        let jobs: Vec<ConvertRow> = x
            .convert
            .iter()
            .map(|j| ConvertRow {
                name: ss(library::display_name(&j.source)),
                dest: ss(library::tilde(&j.dest)),
                progress: if j.done {
                    1.0
                } else {
                    j.player.position().max(0.0)
                },
                status: ss(if j.failed {
                    tr("Failed")
                } else if j.done {
                    tr("Done")
                } else {
                    format!("{:.0} %", j.player.position().max(0.0) * 100.0)
                }),
            })
            .collect();
        f.set_convert_jobs(model(jobs));
    }

    // ------------------------------------------------------------ stream

    pub(super) fn open_stream_dialog(&self) {
        let w = self.ui();
        let f = w.global::<Forms>();
        let st = self.st.borrow();
        f.set_stream_port(st.settings.oi("stream.port") as i32);
        f.set_stream_source(ss(st
            .current
            .as_deref()
            .map(library::display_name)
            .unwrap_or_else(|| tr("Nothing is playing"))));
        f.set_stream_url(ss(self.x.borrow().streaming.clone().unwrap_or_default()));
        drop(st);
        w.global::<AppState>().set_dialog(12);
    }

    pub(super) fn toggle_streaming(self: &Rc<Self>) {
        let w = self.ui();
        let f = w.global::<Forms>();
        if self.x.borrow().streaming.is_some() {
            self.x.borrow_mut().streaming = None;
            f.set_stream_url("".into());
            self.reload_media();
            self.toast(tr("Streaming stopped"));
        } else {
            if self.st.borrow().current.is_none() {
                return self.toast(tr("Play something to stream it"));
            }
            let port = f.get_stream_port().clamp(1024, 65535);
            self.st.borrow_mut().settings.set_extra("stream.port", port);
            self.save_settings();
            let url = format!("http://{}:{}/", local_ip(), port);
            {
                let mut x = self.x.borrow_mut();
                x.streaming = Some(url.clone());
                x.recording = None;
            }
            f.set_stream_url(ss(&url));
            self.reload_media();
            self.toast(tr_fmt("Streaming on {}", &[&url]));
        }
        self.refresh_toolbar();
    }

    // -------------------------------------------------------------- cast

    pub(super) fn start_cast_discovery(&self) {
        if !self.st.borrow().settings.ob("cast.discover") {
            self.x.borrow_mut().discovery = None;
            return;
        }
        if self.x.borrow().discovery.is_some() {
            return;
        }
        let d = self
            .engine
            .borrow()
            .as_ref()
            .and_then(|e| e.renderer_discovery("microdns_renderer"));
        self.x.borrow_mut().discovery = d;
        self.scan_lan_casts();
    }

    /// Search the network for Chromecasts ourselves (at most every 20 s).
    pub(super) fn scan_lan_casts(&self) {
        if !self.st.borrow().settings.ob("cast.discover") {
            return;
        }
        {
            let mut x = self.x.borrow_mut();
            if x.lan_scan
                .is_some_and(|t| t.elapsed() < Duration::from_secs(20))
            {
                return;
            }
            x.lan_scan = Some(Instant::now());
        }
        background(
            || crate::mdns::discover(Duration::from_secs(3)),
            |c, found| c.x.borrow_mut().lan_casts = found,
        );
    }

    /// Entries of the "Cast" menu.
    /// Name of the device playback is sent to, if any.
    pub(super) fn cast_name(&self) -> Option<String> {
        let x = self.x.borrow();
        x.cast_target
            .as_ref()
            .map(|t| t.name.clone())
            .or_else(|| x.cast_ip.as_ref().map(|c| c.0.clone()))
    }

    pub(super) fn cast_entries(&self) -> Vec<(String, String, bool)> {
        let x = self.x.borrow();
        let cur = x.cast_target.as_ref().map(|t| t.name.clone());
        let mut v = vec![(
            "cast:local".to_string(),
            tr("This computer"),
            cur.is_none() && x.cast_ip.is_none(),
        )];
        let sending_to = |ip: &str, port: u16| {
            x.cast_ip
                .as_ref()
                .is_some_and(|(_, i, p)| i == ip && *p == port)
        };
        let listed: Vec<String> = v.iter().map(|e| e.1.clone()).collect();
        for (i, d) in x.lan_casts.iter().enumerate() {
            // libVLC's discoverer may list the same device already
            if listed
                .iter()
                .any(|l| l.starts_with(&format!("{} (", d.name)))
            {
                continue;
            }
            v.push((
                format!("cast:lan:{i}"),
                format!("{} (Chromecast)", d.name),
                sending_to(&d.ip.to_string(), d.port),
            ));
        }
        let ip = self.st.borrow().settings.os("cast.ip");
        if !ip.trim().is_empty() {
            v.push((
                "cast:ip".to_string(),
                tr_fmt("Chromecast at {}", &[ip.trim()]),
                sending_to(ip.trim(), 8009),
            ));
        }
        if let Some(d) = &x.discovery {
            for (i, item) in d.items().iter().enumerate() {
                v.push((
                    format!("cast:{i}"),
                    format!("{} ({})", item.name, item.kind),
                    cur.as_deref() == Some(item.name.as_str()),
                ));
            }
        }
        v
    }

    pub(super) fn cast_to(self: &Rc<Self>, arg: &str) {
        let by_address = if arg == "ip" {
            let ip = self.st.borrow().settings.os("cast.ip").trim().to_string();
            (!ip.is_empty()).then(|| (ip.clone(), ip, 8009))
        } else if let Some(i) = arg.strip_prefix("lan:") {
            let i: usize = i.parse().unwrap_or(usize::MAX);
            self.x
                .borrow()
                .lan_casts
                .get(i)
                .map(|d| (d.name.clone(), d.ip.to_string(), d.port))
        } else {
            None
        };
        let from = self.playback_snapshot().map(|(idx, time, _)| (idx, time));
        if arg == "ip" || arg.starts_with("lan:") {
            let Some(dev) = by_address else { return };
            let name = dev.0.clone();
            {
                let mut x = self.x.borrow_mut();
                x.cast_from = from;
                x.cast_target = None;
                x.cast_ip = Some(dev);
                x.recording = None;
                x.streaming = None;
            }
            self.reload_media();
            self.refresh_toolbar();
            return self.toast(tr_fmt("Casting to {}", &[&name]));
        }
        let target = if arg == "local" {
            None
        } else {
            let i: usize = arg.parse().unwrap_or(usize::MAX);
            let items = self
                .x
                .borrow()
                .discovery
                .as_ref()
                .map(|d| d.items())
                .unwrap_or_default();
            match items.get(i).cloned() {
                Some(t) => Some(t),
                None => return,
            }
        };
        let name = target.as_ref().map(|t| t.name.clone());
        {
            let mut x = self.x.borrow_mut();
            x.cast_from = from.filter(|_| target.is_some());
            x.cast_target = target;
            x.cast_ip = None;
        }
        self.reload_media();
        self.refresh_toolbar();
        match name {
            Some(n) => self.toast(tr_fmt("Casting to {}", &[&n])),
            None => self.toast(tr("Playing on this computer")),
        }
    }
}
