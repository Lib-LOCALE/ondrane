#![cfg_attr(not(test), windows_subsystem = "windows")]

//! Ondrane CLI & Application Entry Point
//!
//! Handles command-line arguments (files, streams, flags),
//! initializes the 3-pillar settings, inspects design tokens,
//! and launches the native libVLC engine with `--aout=pulse`.

use std::env;
use std::process;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
fn attach_console() {
    extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn GetStdHandle(n_std_handle: u32) -> isize;
    }
    unsafe {
        const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // -11
        const INVALID_HANDLE_VALUE: isize = -1;
        let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if stdout_handle == 0 || stdout_handle == INVALID_HANDLE_VALUE {
            AttachConsole(0xFFFF_FFFF); // ATTACH_PARENT_PROCESS
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn attach_console() {}

use ondrane::{
    customization::{Profile, Tool, ToolbarConfig},
    engine::{find_libvlc, VlcEngine},
    settings::OndraneSettings,
    tokens::{dark, light, ThemeMode, ThemePalette},
};

fn print_help() {
    println!(
        r#"Ondrane v{} (Rust Edition)
Modern media player based on 'VLC Redesign (Community).fig'

USAGE:
    ondrane [OPTIONS] [FILE | URL ...]

ARGS:
    <FILE | URL>...     One or more local video/audio files or streaming URLs to open

OPTIONS:
    -f, --fullscreen    Launch in fullscreen mode
        --volume <N>    Set playback volume level (0 to 125, default: 80)
        --sub <PATH>    Load an external subtitle file (.srt, .ass, .vtt)
        --theme <MODE>  Force UI theme: 'dark' (default) or 'light'
        --diag          Run internal engine diagnostics (libVLC, PipeWire/Pulse, tokens)
        --dump-tokens   Output Figma color & layout tokens in JSON format
        --dump-settings Output default 3-pillar settings hierarchy in JSON format
        --dump-toolbar  Output customizable control bar presets in JSON format
    -v, --version       Display version information
        --onboarding    Show the welcome screen
        --cli           Play the first argument without the GUI (diagnostics)
    -h, --help          Show this help message
"#,
        env!("CARGO_PKG_VERSION")
    );
}

fn run_diagnostics() -> bool {
    let mut playback_ok = false;
    println!("=======================================================");
    println!("           ONDRANE RUST SELF-DIAGNOSTICS            ");
    println!("=======================================================");

    // 1. Check libVLC library
    println!(
        "\n[1/4] Native libVLC Engine Detection ({}/{}):",
        env::consts::OS,
        env::consts::ARCH
    );
    if let Some(path) = find_libvlc() {
        println!("  ✓ Found libVLC library at: {}", path.display());
        match VlcEngine::load() {
            Ok(engine) => {
                println!("  ✓ Engine loaded successfully!");
                println!("  ✓ Native libVLC version: {}", engine.version());
                #[cfg(target_os = "linux")]
                println!("  • Audio output: automatic, PulseAudio/PipeWire when available");
                #[cfg(target_os = "windows")]
                println!("  ✓ Audio output backend: Windows WASAPI / DirectSound native");

                match engine.create_player() {
                    Ok(player) => {
                        playback_ok = true;
                        println!("  ✓ Media player instantiated successfully.");
                        println!("  ✓ Default volume: {}%", player.volume());
                    }
                    Err(e) => println!("  ✗ Player instantiation error: {}", e),
                }
            }
            Err(e) => println!("  ✗ Engine initialization error: {}", e),
        }
    } else {
        println!("  ✗ libVLC dynamic library not found on system search paths!");
    }

    // 2. Check Figma Design Tokens
    println!("\n[2/4] Figma Design Tokens:");
    println!(
        "  ✓ Dark Accent Main:     {} (RGB: {:?})",
        dark::accent::MAIN.hex,
        dark::accent::MAIN.to_rgb()
    );
    println!(
        "  ✓ Dark Accent Bright:   {} (RGB: {:?})",
        dark::accent::BRIGHT.hex,
        dark::accent::BRIGHT.to_rgb()
    );
    println!(
        "  ✓ Dark BG Canvas (B1):  {} (RGB: {:?})",
        dark::bg::B1.hex,
        dark::bg::B1.to_rgb()
    );
    println!(
        "  ✓ Dark BG Control (A4): {} (RGB: {:?})",
        dark::bg::A4.hex,
        dark::bg::A4.to_rgb()
    );
    println!(
        "  ✓ Dark FG Title (FG1):  {} (RGB: {:?})",
        dark::fg::FG1.hex,
        dark::fg::FG1.to_rgb()
    );
    println!(
        "  ✓ Light Accent Main:    {} (RGB: {:?})",
        light::accent::MAIN.hex,
        light::accent::MAIN.to_rgb()
    );
    println!(
        "  ✓ Light BG Surface:     {} (RGB: {:?})",
        light::bg::A1.hex,
        light::bg::A1.to_rgb()
    );

    // 3. Check 3-Pillar Settings
    println!("\n[3/4] 3-Pillar Settings Hierarchy:");
    let settings = OndraneSettings::load_or_default();
    println!(
        "  ✓ App Pillar:    Language='{}', Theme={:?}, AutoResize={}",
        settings.app.global.language,
        settings.app.global.theme_mode,
        settings.app.interface.auto_resize_window
    );
    println!(
        "  ✓ Media Pillar:  Resume={:?}, HW_Decoding={}, Volume={}, MaxVol={}",
        settings.media.playback.resume_mode,
        settings.media.video.hardware_decoding,
        settings.media.audio.default_volume,
        settings.media.audio.max_volume
    );
    println!(
        "  ✓ System Pillar: Caching={}ms, GPU={:?}, Threads={}",
        settings.system.network.network_caching_ms,
        settings.system.performance.gpu_backend,
        settings.system.performance.decoding_threads
    );
    println!(
        "  ✓ Config Path:   {}",
        OndraneSettings::config_path().display()
    );

    // 4. Check Customizable Toolbar
    println!("\n[4/4] Customizable Control Bar:");
    let toolbar = ToolbarConfig::load_or_default();
    println!(
        "  ✓ Profile: {} ({} optional tools)",
        toolbar.profile.label(),
        toolbar.tools.len()
    );
    println!("  ✓ Available tools: {}", Tool::ALL.len());

    if playback_ok {
        println!("\n✓ Core checks passed; decode a media file to verify playback.\n");
    } else {
        eprintln!("\n✗ Playback engine unavailable. Install VLC 3 and its plugins.\n");
    }
    playback_ok
}

/// Find and initialise libVLC. Without it the player still starts (library,
/// settings...) and explains how to enable playback.
fn load_engine(args: &[String]) -> Option<(std::path::PathBuf, VlcEngine)> {
    match find_libvlc() {
        Some(path) => match VlcEngine::load_with_args(&path, args) {
            Ok(eng) => {
                println!("libVLC initialized cleanly: {}", eng.version());
                Some((path, eng))
            }
            Err(e) => {
                eprintln!("Failed to initialize libVLC engine: {}", e);
                eprintln!("Run `ondrane --diag` for diagnostic information.");
                None
            }
        },
        None => {
            eprintln!("libVLC was not found on standard system paths");
            None
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut targets: Vec<String> = Vec::new();
    let mut subtitle_file: Option<String> = None;
    let mut volume: Option<i32> = None;
    let mut fullscreen: Option<bool> = None;
    let mut theme_mode: Option<ThemeMode> = None;
    let mut cli_mode = false;
    let mut show_onboarding = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                attach_console();
                print_help();
                return;
            }
            "-v" | "--version" => {
                attach_console();
                println!("Ondrane {} (Rust Edition)", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--diag" => {
                attach_console();
                process::exit(if run_diagnostics() { 0 } else { 1 });
            }
            "--dump-tokens" => {
                attach_console();
                let dark_pal = ThemePalette::dark();
                let light_pal = ThemePalette::light();
                let combined = serde_json::json!({
                    "dark": dark_pal,
                    "light": light_pal,
                });
                println!("{}", serde_json::to_string_pretty(&combined).unwrap());
                return;
            }
            "--dump-settings" => {
                attach_console();
                let settings = OndraneSettings::default();
                println!("{}", settings.to_json().unwrap());
                return;
            }
            "--dump-toolbar" => {
                attach_console();
                let profiles: serde_json::Map<String, serde_json::Value> = Profile::PRESETS
                    .iter()
                    .map(|p| {
                        (
                            p.label().to_string(),
                            serde_json::to_value(ToolbarConfig::from_profile(*p)).unwrap(),
                        )
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&profiles).unwrap());
                return;
            }
            "-f" | "--fullscreen" => {
                fullscreen = Some(true);
            }
            "--sub" => {
                i += 1;
                if i < args.len() {
                    subtitle_file = Some(args[i].clone());
                } else {
                    eprintln!("Error: --sub requires a file path argument");
                    process::exit(1);
                }
            }
            "--volume" => {
                i += 1;
                if i == args.len() {
                    eprintln!("Error: --volume requires an integer between 0 and 125");
                    process::exit(1);
                }
                if i < args.len() {
                    if let Ok(v @ 0..=125) = args[i].parse::<i32>() {
                        volume = Some(v);
                    } else {
                        eprintln!("Error: --volume requires an integer between 0 and 125");
                        process::exit(1);
                    }
                }
            }
            "--theme" => {
                i += 1;
                if i == args.len() {
                    eprintln!("Error: --theme requires dark or light");
                    process::exit(1);
                }
                if i < args.len() {
                    match args[i].to_lowercase().as_str() {
                        "dark" => theme_mode = Some(ThemeMode::Dark),
                        "light" => theme_mode = Some(ThemeMode::Light),
                        _ => {
                            eprintln!("Warning: unknown theme '{}', using dark", args[i]);
                            theme_mode = Some(ThemeMode::Dark);
                        }
                    }
                }
            }
            "--onboarding" => {
                show_onboarding = true;
            }
            "--cli" => {
                attach_console();
                cli_mode = true;
            }
            arg if !arg.starts_with('-') => {
                targets.push(arg.to_string());
            }
            unknown => {
                eprintln!("Unknown argument: {}", unknown);
                eprintln!("Use --help for usage instructions.");
                process::exit(1);
            }
        }
        i += 1;
    }

    let mut settings = OndraneSettings::load_or_default();
    if let Some(tm) = theme_mode {
        settings.app.global.theme_mode = tm;
        settings.app.global.color_scheme = ondrane::themes::ColorScheme::Ondrane;
    }
    if let Some(fs) = fullscreen {
        settings.media.video.fullscreen_on_launch = fs;
    }
    if let Some(vol) = volume {
        settings.media.audio.default_volume = vol as u32;
    }

    println!(
        "Starting Ondrane (Rust Edition)... [Theme: {:?}]",
        settings.app.global.theme_mode
    );

    // "Only allow one Ondrane window": hand the files to the running window.
    if !cli_mode && settings.ob("lib.single") {
        let from_file_manager = !targets.is_empty();
        if !from_file_manager || settings.ob("lib.fm-apply") {
            let req = ondrane::instance::OpenRequest {
                targets: targets
                    .iter()
                    .map(|t| {
                        let p = std::path::Path::new(t);
                        if p.exists() {
                            p.canonicalize()
                                .map(|c| c.to_string_lossy().to_string())
                                .unwrap_or_else(|_| t.clone())
                        } else {
                            t.clone()
                        }
                    })
                    .collect(),
                enqueue: settings.ob("lib.enqueue"),
            };
            if ondrane::instance::forward(&req) {
                println!("Opened in the running Ondrane window.");
                return;
            }
        }
    }

    // libVLC is optional for the interface: without it the player still
    // starts (library, settings...) and explains how to enable playback.
    ondrane::addons::apply_master_switch(settings.system.extensions.lua_enabled);
    let engine_args = ondrane::options::engine_args(&settings);
    let engine = if cli_mode {
        load_engine(&engine_args)
    } else {
        None
    };
    // The GUI loads libVLC (plugin scan) in the background so the window
    // appears at once instead of waiting for it.
    let (engine_tx, engine_rx) = std::sync::mpsc::channel();
    if !cli_mode {
        thread::spawn(move || {
            let _ = engine_tx.send(load_engine(&engine_args));
        });
    }

    let toolbar = ToolbarConfig::load_or_default();

    if cli_mode {
        let Some((_, engine)) = engine.as_ref() else {
            process::exit(1);
        };
        let player = match engine.create_player() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to create media player: {}", e);
                process::exit(1);
            }
        };

        if let Some(vol) = volume {
            player.set_volume(vol);
            println!("Volume set to: {}%", player.volume());
        }

        if targets.is_empty() {
            println!("Ready in CLI mode. No media specified.");
            return;
        }

        let target = &targets[0];
        println!("Opening media: {}", target);
        if let Err(e) = player.open(target) {
            eprintln!("Failed to open media: {}", e);
            process::exit(1);
        }

        if let Some(sub) = subtitle_file {
            println!("Adding external subtitles: {}", sub);
            let _ = player.add_subtitle_file(&sub);
        }

        println!("Starting playback...");
        player.play();
        thread::sleep(Duration::from_millis(300));
        println!("Playback active. State: {:?}", player.state());
        return;
    }

    // Launch GUI Application
    println!("Launching Ondrane GUI Window (Slint declarative engine)...");
    match ondrane::GuiApp::new(
        engine_rx,
        settings,
        toolbar,
        targets,
        subtitle_file,
        show_onboarding,
    ) {
        Ok(app) => {
            if let Err(e) = app.run() {
                eprintln!("UI Platform Error: {}", e);
                process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to start GUI application: {}", e);
            process::exit(1);
        }
    }
}
