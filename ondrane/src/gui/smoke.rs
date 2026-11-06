//! Opt-in desktop integration test: requires a live compositor, libVLC and two videos.
use super::*;

fn capture(c: &Controller, name: &str) {
    let dir = std::env::var("VLC_SMOKE_OUTPUT").expect("set VLC_SMOKE_OUTPUT");
    std::fs::create_dir_all(&dir).unwrap();
    let pixels = c.ui().window().take_snapshot().expect("window snapshot");
    image::save_buffer(
        Path::new(&dir).join(format!("{name}.png")),
        pixels.as_bytes(),
        pixels.width(),
        pixels.height(),
        image::ColorType::Rgba8,
    )
    .unwrap();
}

#[test]
#[ignore = "requires live desktop, VLC_SMOKE_MEDIA, VLC_SMOKE_NEXT and isolated XDG directories"]
fn desktop_playback() {
    let first = std::env::var("VLC_SMOKE_MEDIA").expect("set VLC_SMOKE_MEDIA");
    let second = std::env::var("VLC_SMOKE_NEXT").expect("set VLC_SMOKE_NEXT");
    let mut settings = OndraneSettings::default();
    settings.app.global.check_updates = false;
    settings.media.video.hardware_decoding = false;
    settings.media.playback.seek_thumbnails = true;
    settings.app.global.color_scheme = crate::themes::ColorScheme::Catppuccin;
    settings.set_extra("ui.xwayland", std::env::var_os("VLC_SMOKE_X11").is_some());
    settings.set_extra("lib.single", false);
    let (tx, rx) = std::sync::mpsc::channel();
    let path = crate::engine::find_libvlc().expect("libVLC runtime required");
    let engine = VlcEngine::load_with_args(&path, &crate::options::engine_args(&settings)).unwrap();
    tx.send(Some((path, engine))).unwrap();
    let app = GuiApp::new(
        rx,
        settings,
        ToolbarConfig::default(),
        vec![first, second],
        None,
        false,
    )
    .unwrap();
    let gpu_verified = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if std::env::var_os("VLC_SMOKE_GPU").is_some() {
        let verified = gpu_verified.clone();
        app.window
            .window()
            .set_rendering_notifier(move |state, api| {
                if matches!(state, slint::RenderingState::RenderingSetup) {
                    if let slint::GraphicsAPI::NativeOpenGL { get_proc_address } = api {
                        let ptr = get_proc_address(c"glGetString");
                        assert!(!ptr.is_null());
                        // The notifier runs with the renderer's own OpenGL context current.
                        unsafe {
                            let get_string: unsafe extern "system" fn(
                                u32,
                            )
                                -> *const std::ffi::c_char = std::mem::transmute(ptr);
                            let name = get_string(0x1F01);
                            assert!(!name.is_null());
                            let renderer = std::ffi::CStr::from_ptr(name).to_string_lossy();
                            eprintln!("Slint GL_RENDERER: {renderer}");
                            assert!(!renderer.to_lowercase().contains("llvmpipe"));
                            verified.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
            })
            .unwrap();
    }
    app.window.global::<AppState>().set_dialog(0);
    let c = app.controller.clone();
    let timer = slint::Timer::default();
    let mut step = 0;
    let mut next_started = Instant::now();
    let started = Instant::now();
    let expected_second = std::env::var("VLC_SMOKE_NEXT").unwrap();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(800),
        move || {
            let w = c.ui();
            let a = w.global::<AppState>();
            match step {
                0 => {
                    if !a.get_has_frame() && started.elapsed() < Duration::from_secs(15) {
                        return;
                    }
                    if std::env::var_os("VLC_SMOKE_GPU").is_some() {
                        assert!(gpu_verified.load(std::sync::atomic::Ordering::SeqCst));
                    }
                    assert!(a.get_engine_ok());
                    assert!(
                        a.get_has_frame(),
                        "decoded video must reach the Slint window"
                    );
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Playing);
                    let (width, height) = c.player().unwrap().video_dimensions();
                    assert!(width >= 640 && height >= 360, "video output dimensions");
                    capture(&c, "01-playback");
                    c.command("play-pause");
                }
                1 => {
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Paused);
                    c.set_volume(45);
                    c.command("mute");
                    assert!(c.player().unwrap().is_muted());
                    c.command("mute");
                    c.command("speed:1.5");
                    assert!((c.player().unwrap().rate() - 1.5).abs() < 0.01);
                    c.handle_act_float("seek", 0.4);
                    c.command("play-pause");
                }
                2 => {
                    assert!(c.player().unwrap().time_ms() >= 10_000);
                    assert_eq!(c.player().unwrap().volume(), 45);
                    c.command("fullscreen");
                }
                3 => {
                    assert!(w.window().is_fullscreen());
                    capture(&c, "02-fullscreen");
                    c.command("escape");
                    c.command("library");
                }
                4 => {
                    assert!(
                        !a.get_fullscreen(),
                        "Escape must leave fullscreen when no modal is open (dialog={}, menu={})",
                        a.get_dialog(),
                        a.get_menu_open()
                    );
                    assert_eq!(a.get_view(), 1);
                    capture(&c, "03-library");
                    c.command("settings");
                }
                5 => {
                    assert!(a.get_settings_open());
                    for page in 0..13 {
                        c.command(&format!("settings-page:{page}"));
                    }
                    c.command("settings-page:1");
                    capture(&c, "04-settings");
                    c.command("escape");
                    c.command("open-player");
                    c.command("next");
                    next_started = Instant::now();
                }
                6 => {
                    assert_eq!(
                        c.st.borrow().current.as_deref(),
                        Some(expected_second.as_str())
                    );
                    let p = c.player().unwrap();
                    let buffer = p.video_buffer_state.lock().unwrap();
                    let offset =
                        ((buffer.height / 10 * buffer.width + buffer.width / 10) * 4) as usize;
                    let pixel = &buffer.buffer[offset..offset + 3];
                    let grey = pixel[0].abs_diff(pixel[1]) < 10 && pixel[1].abs_diff(pixel[2]) < 10;
                    eprintln!(
                        "next frame: {:?}, {}ms, RGB={pixel:?}",
                        p.state(),
                        p.time_ms()
                    );
                    if (!grey || !a.get_has_frame())
                        && next_started.elapsed() < Duration::from_secs(10)
                    {
                        return;
                    }
                    assert!(grey, "the second clip must replace the first clip's pixels");
                    drop(buffer);
                    assert!(a.get_has_frame());
                    capture(&c, "05-next-same-size");
                    c.restart_engine();
                }
                7 => {
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Playing);
                    assert!(a.get_has_frame());
                    c.command("stop");
                }
                8 => {
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Stopped);
                    assert!(!a.get_playing());
                    slint::quit_event_loop().unwrap();
                }
                _ => unreachable!(),
            }
            eprintln!("desktop smoke step {step}: OK");
            step += 1;
        },
    );
    app.run().unwrap();
}

#[test]
#[ignore = "requires live desktop, VLC_SMOKE_OUTPUT and isolated XDG directories"]
fn desktop_themes() {
    use crate::themes::{CatppuccinFlavor, ColorScheme};
    let mut settings = OndraneSettings::default();
    settings.app.global.language = "fr".into();
    settings.app.global.check_updates = false;
    settings.set_extra("lib.single", false);
    let (_tx, rx) = std::sync::mpsc::channel();
    let app = GuiApp::new(rx, settings, ToolbarConfig::default(), vec![], None, false).unwrap();
    app.window.global::<AppState>().set_dialog(0);
    if std::env::var_os("VLC_SMOKE_NARROW").is_some() {
        app.window
            .window()
            .set_size(slint::LogicalSize::new(900.0, 760.0));
    }
    let c = app.controller.clone();
    c.command("settings");
    c.command("settings-page:0");
    let cases = [
        (1, 0, "catppuccin-mocha"),
        (1, 1, "catppuccin-macchiato"),
        (1, 2, "catppuccin-frappe"),
        (1, 3, "catppuccin-latte"),
        (2, 0, "dracula"),
        (3, 0, "nord"),
        (4, 0, "gruvbox"),
        (5, 0, "tokyo-night"),
        (6, 0, "rose-pine"),
        (0, 0, "vlc"),
    ];
    let timer = slint::Timer::default();
    let mut step = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let w = c.ui();
            let a = w.global::<AppState>();
            let theme = w.global::<Theme>();
            if step > 0 {
                let (scheme, flavor, name) = cases[step - 1];
                let saved =
                    OndraneSettings::load_from_file(&OndraneSettings::config_path()).unwrap();
                assert_eq!(saved.app.global.color_scheme, ColorScheme::ALL[scheme]);
                assert!(!saved.ob("ui.skin"));
                if let Some(p) = ColorScheme::ALL[scheme].palette(CatppuccinFlavor::ALL[flavor]) {
                    assert_eq!(theme.get_bg_b1(), parse_hex_color(p.base).unwrap());
                    assert_eq!(theme.get_fg_1(), parse_hex_color(p.text).unwrap());
                    assert_eq!(theme.get_accent(), parse_hex_color(p.accent).unwrap());
                    assert_eq!(theme.get_dark(), p.dark);
                } else {
                    assert_eq!(theme.get_bg_b1(), parse_hex_color("#0e0e0e").unwrap());
                    assert_eq!(theme.get_accent(), parse_hex_color("#78e3cb").unwrap());
                }
                capture(&c, &format!("theme-{name}"));
                eprintln!("theme {name}: applied, saved and rendered");
            }
            if step == cases.len() {
                slint::quit_event_loop().unwrap();
                return;
            }
            let (scheme, flavor, _) = cases[step];
            // A built-in choice must be visible even when a custom skin was active.
            c.st.borrow_mut().settings.set_extra("ui.skin", true);
            a.invoke_setting_int("app.palette".into(), scheme as i32);
            if scheme == 1 {
                a.invoke_setting_int("app.catppuccin".into(), flavor as i32);
            }
            assert!(a.get_settings_open());
            step += 1;
        },
    );
    app.run().unwrap();
}

#[test]
#[ignore = "requires desktop ScreenCast source selection, GStreamer PipeWire and libVLC"]
fn desktop_screencast() {
    let capture = crate::screencast::start(15, false)
        .unwrap()
        .expect("select a screen in the desktop portal");
    let engine = VlcEngine::load_headless().unwrap();
    let p = engine.create_player().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    p.set_video_callback(move |w, h, rgba| {
        if rgba.iter().step_by(4).any(|v| *v > 20) {
            let _ = tx.try_send((w, h));
        }
    });
    p.open_with_options(
        &capture.mrl(),
        &[":file-caching=100".into(), ":demux=avformat".into()],
    )
    .unwrap();
    p.play();
    let (w, h) = rx
        .recv_timeout(Duration::from_secs(25))
        .expect("PipeWire frames must reach libVLC");
    assert!(w >= 320 && h >= 200);
    eprintln!("ScreenCast -> PipeWire -> GStreamer -> libVLC: {w}x{h} pixels received");
    p.stop();
    drop(capture);
}

#[test]
#[ignore = "requires connected V4L2 camera and libVLC; does not save camera images"]
fn desktop_camera() {
    let engine = VlcEngine::load_headless().unwrap();
    let p = engine.create_player().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    p.set_video_callback(move |w, h, rgba| {
        if rgba.len() >= (w * h * 4) as usize {
            let _ = tx.try_send((w, h));
        }
    });
    p.open_with_options(
        "v4l2:///dev/video0",
        &[
            ":v4l2-width=640".into(),
            ":v4l2-height=480".into(),
            ":v4l2-fps=15".into(),
            ":v4l2-chroma=MJPG".into(),
        ],
    )
    .unwrap();
    p.play();
    let (w, h) = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("USB camera frames must reach libVLC");
    assert!(w >= 320 && h >= 200);
    eprintln!("USB V4L2 camera -> libVLC: {w}x{h} pixels received; no image saved");
    p.stop();
}

#[test]
#[ignore = "requires GStreamer software test source and libVLC"]
fn capture_pipe_transport() {
    use std::os::fd::AsRawFd;
    let mut child = std::process::Command::new("gst-launch-1.0")
        .args([
            "-q",
            "videotestsrc",
            "is-live=true",
            "!",
            "video/x-raw,width=640,height=360,framerate=15/1",
            "!",
            "videoconvert",
            "!",
            "jpegenc",
            "!",
            "matroskamux",
            "streamable=true",
            "!",
            "fdsink",
            "fd=1",
            "sync=false",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let out = child.stdout.take().unwrap();
    let recording_dir = tempfile::tempdir().unwrap();
    let recording = recording_dir.path().join("capture.mkv");
    let engine = VlcEngine::load_headless().unwrap();
    let p = engine.create_player().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    p.set_video_callback(move |w, h, _| {
        let _ = tx.try_send((w, h));
    });
    p.open_with_options(
        &format!("fd://{}", out.as_raw_fd()),
        &[
            ":file-caching=100".into(),
            ":demux=avformat".into(),
            format!(
                ":sout=#duplicate{{dst=display,dst=std{{access=file,mux=mkv,dst={}}}}}",
                recording.display()
            ),
        ],
    )
    .unwrap();
    p.play();
    let result = (0..5)
        .map(|_| rx.recv_timeout(Duration::from_secs(10)))
        .collect::<Result<Vec<_>, _>>();
    p.stop();
    let _ = child.kill();
    let _ = child.wait();
    for (w, h) in result.unwrap() {
        assert_eq!(w, 640);
        assert!((360..=384).contains(&h));
    }
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&recording)
        .args(["-frames:v", "1", "-f", "null", "-"])
        .status()
        .unwrap();
    assert!(decoded.success(), "recorded Matroska must decode");
}

#[test]
#[ignore = "requires real ScreenCast desktop selection and libVLC"]
fn desktop_capture_ui() {
    let mut settings = OndraneSettings::default();
    settings.app.global.check_updates = false;
    settings.app.global.language = "fr".into();
    settings.media.playback.seek_thumbnails = false;
    settings.set_extra("lib.single", false);
    let path = crate::engine::find_libvlc().unwrap();
    let engine = VlcEngine::load_with_args(&path, &crate::options::engine_args(&settings)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(Some((path, engine))).unwrap();
    let app = GuiApp::new(rx, settings, ToolbarConfig::default(), vec![], None, false).unwrap();
    app.window.global::<AppState>().set_dialog(0);
    let c = app.controller.clone();
    let timer = slint::Timer::default();
    let mut step = 0;
    let start = Instant::now();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let w = c.ui();
            let a = w.global::<AppState>();
            assert!(
                start.elapsed() < Duration::from_secs(150),
                "desktop selection/capture timeout"
            );
            match step {
                0 => {
                    if c.player().is_none() {
                        return;
                    }
                    let f = w.global::<Forms>();
                    f.set_capture_mode(1);
                    f.set_capture_audio(false);
                    f.set_capture_fps(15.0);
                    c.play_capture();
                    step = 1;
                }
                1..=3 => {
                    if !a.get_has_frame() {
                        return;
                    }
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Playing);
                    assert!(c.x.borrow().screen_capture.is_some());
                    let (width, height) = c.player().unwrap().video_dimensions();
                    assert!(width >= 320 && height >= 200);
                    eprintln!("ScreenCast UI phase {step}: {width}x{height}, playing");
                    match step {
                        1 => {
                            capture(&c, "06-wayland-screen");
                            c.reload_media();
                        }
                        2 => c.restart_engine(),
                        3 => c.stop(),
                        _ => unreachable!(),
                    }
                    step += 1;
                }
                4 => {
                    assert!(c.x.borrow().screen_capture.is_none());
                    assert_eq!(c.player().unwrap().state(), PlaybackState::Stopped);
                    assert!(!a.get_has_frame());
                    eprintln!("ScreenCast UI: stop and source cleanup OK");
                    slint::quit_event_loop().unwrap();
                    step += 1;
                }
                _ => {}
            }
        },
    );
    app.run().unwrap();
}

#[test]
#[ignore = "requires a live desktop and SLINT_BACKEND=winit-software"]
fn desktop_empty_player_clicks() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let w = MainWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(900.0, 600.0));
    w.global::<AppState>().set_engine_ok(true);
    // Unique test color locates the rendered button without duplicating layout geometry.
    w.global::<Theme>()
        .set_accent(slint::Color::from_rgb_u8(255, 0, 255));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let received = actions.clone();
    w.global::<AppState>()
        .on_act(move |action| received.borrow_mut().push(action.to_string()));
    w.show().unwrap();
    let weak = w.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, Duration::from_millis(400), move || {
        let w = weak.upgrade().unwrap();
        let pixels = w.window().take_snapshot().unwrap();
        let width = pixels.width() as usize;
        let row = pixels.as_bytes().chunks_exact(width * 4).enumerate().find_map(|(y, row)| {
            let mut run = 0;
            for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
                run = if p[..3] == [255, 0, 255] { run + 1 } else { 0 };
                if run >= 100 { return Some((x - 49, y + 12)); }
            }
            None
        }).expect("rendered Open Files button");
        let scale = w.window().scale_factor();
        let position = slint::LogicalPosition::new(row.0 as f32 / scale, row.1 as f32 / scale);
        let click = |position| {
            w.window().dispatch_event(WindowEvent::PointerMoved { position });
            w.window().dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
            w.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        };
        click(position);
        assert!(actions.borrow().iter().any(|a| a == "open-files"), "the visible button must receive the click: {:?}", actions.borrow());
        assert!(!actions.borrow().iter().any(|a| a == "canvas-click"));
        click(slint::LogicalPosition::new(position.x + 162.0, position.y));
        assert_eq!(w.global::<AppState>().get_dialog(), 1, "network button must also receive clicks");
        w.global::<AppState>().set_dialog(0);
        actions.borrow_mut().clear();
        click(slint::LogicalPosition::new(450.0, 100.0));
        assert!(actions.borrow().iter().any(|a| a == "canvas-click"), "background canvas remains interactive");
        w.global::<AppState>().set_view(1);
        w.global::<AppState>().set_has_media(true);
        w.window().take_snapshot().unwrap();
        let size = w.window().size().to_logical(scale);
        // MiniPlayer has a fixed 391 x 339 layout, anchored six pixels from the edges.
        let origin = slint::LogicalPosition::new(size.width - 391.0 - 6.0 + 8.0, size.height - 339.0 - 6.0 + 8.0);
        let stop = slint::LogicalPosition::new(origin.x + 24.0, origin.y + 24.0);
        w.window().dispatch_event(WindowEvent::PointerMoved { position: stop });
        w.window().take_snapshot().unwrap();
        actions.borrow_mut().clear();
        click(stop);
        assert!(actions.borrow().iter().any(|a| a == "stop"), "mini-player Stop must receive clicks: {:?}", actions.borrow());
        let play = slint::LogicalPosition::new(origin.x + 187.5, origin.y + 105.5);
        w.window().dispatch_event(WindowEvent::PointerMoved { position: play });
        w.window().take_snapshot().unwrap();
        click(play);
        assert!(actions.borrow().iter().any(|a| a == "play-pause"), "mini-player playback must receive clicks: {:?}", actions.borrow());
        eprintln!("Open Files, Network Stream, background, mini-player Stop and playback: pointer dispatch passed");
        slint::quit_event_loop().unwrap();
    });
    slint::run_event_loop().unwrap();
}

#[test]
#[ignore = "requires a live desktop, libVLC, VLC_SMOKE_OUTPUT and isolated XDG directories"]
fn desktop_branding() {
    let mut settings = OndraneSettings::default();
    settings.app.global.language = "en".into();
    settings.app.global.check_updates = false;
    settings.set_extra("lib.single", false);
    let (tx, rx) = std::sync::mpsc::channel();
    let path = crate::engine::find_libvlc().expect("libVLC runtime");
    let engine = VlcEngine::load_with_args(&path, &crate::options::engine_args(&settings)).unwrap();
    tx.send(Some((path, engine))).unwrap();
    let app = GuiApp::new(rx, settings, ToolbarConfig::default(), vec![], None, false).unwrap();
    let c = app.controller.clone();
    let timer = slint::Timer::default();
    let mut step = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(800),
        move || {
            let w = c.ui();
            assert!(w.global::<AppState>().get_engine_ok());
            match step {
                0 => {
                    capture(&c, "ondrane-welcome");
                    c.command("about");
                }
                1 => {
                    assert_eq!(w.global::<AppState>().get_dialog(), 7);
                    capture(&c, "ondrane-about");
                    c.command("escape");
                    c.command("settings");
                }
                2 => {
                    capture(&c, "ondrane-settings");
                    slint::quit_event_loop().unwrap();
                }
                _ => unreachable!(),
            }
            step += 1;
        },
    );
    app.run().unwrap();
}
