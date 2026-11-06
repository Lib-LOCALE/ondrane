//! `cargo xtask <task>` — developer tasks for Ondrane, in Rust.
//!
//! Tasks:
//!   figma [path.fig]   Re-extract the icon set and app icons from the Figma file
//!   app-icons          Regenerate only the application icons (png / ico / icns)
//!   run [args..]       Build in release mode and launch the player
//!   diag               Print engine / settings diagnostics
//!   install            Install binary, desktop entry and icon for the current user
//!   appimage           Build target/Ondrane.AppDir (+ AppImage with appimagetool)
//!   windows-installer  Build the NSIS installer (needs makensis)

mod figma;
mod kiwi;
mod licenses;

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn run(cmd: &mut Command) {
    eprintln!("==> {:?}", cmd);
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("cannot run {:?}: {e}", cmd.get_program());
            exit(1)
        }
    }
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn build_release() -> PathBuf {
    run(cargo().args(["build", "--release", "--locked", "-p", "ondrane"]));
    let exe = if cfg!(windows) {
        "ondrane.exe"
    } else {
        "ondrane"
    };
    root().join("target/release").join(exe)
}

fn copy(from: &Path, to: &Path) {
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::copy(from, to)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", from.display(), to.display()));
}

fn copy_dir_all(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    if let Ok(entries) = std::fs::read_dir(from) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dest = to.join(entry.file_name());
            if path.is_dir() {
                copy_dir_all(&path, &dest);
            } else {
                copy(&path, &dest);
            }
        }
    }
}

// ------------------------------------------------------------- app icons
fn render_png(svg: &str, size: u32) -> Vec<u8> {
    let tree =
        resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).expect("valid icon svg");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().unwrap()
}

fn rgba(svg: &str, size: u32) -> Vec<u8> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

fn app_icons() {
    let svg = std::fs::read_to_string(root().join("packaging/ondrane.svg"))
        .expect("canonical VectorCraft icon: packaging/ondrane.svg");
    let root = root();
    for size in [64, 128, 256, 512] {
        let dir = root.join(format!("packaging/linux/icons/{size}x{size}/apps"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ondrane.png"), render_png(&svg, size)).unwrap();
    }
    let mut ico = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let img = ico::IconImage::from_rgba_data(size, size, rgba(&svg, size));
        ico.add_entry(ico::IconDirEntry::encode(&img).unwrap());
    }
    ico.write(std::fs::File::create(root.join("packaging/windows/ondrane.ico")).unwrap())
        .unwrap();
    let mut icns = icns::IconFamily::new();
    for size in [16, 32, 64, 128, 256, 512] {
        let img =
            icns::Image::from_data(icns::PixelFormat::RGBA, size, size, rgba(&svg, size)).unwrap();
        icns.add_icon(&img).unwrap();
    }
    icns.write(std::fs::File::create(root.join("packaging/macos/ondrane.icns")).unwrap())
        .unwrap();
    eprintln!("==> app icons written to packaging/");
}

fn figma(args: &[String]) {
    let Some(path) = args.first() else {
        eprintln!("Usage: cargo xtask figma path/to/design.fig (obtain the CC BY source from its Community listing)");
        exit(2);
    };
    let fig = PathBuf::from(path);
    eprintln!("==> decoding {}", fig.display());
    let msg = figma::load_fig(&fig).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        exit(1)
    });
    let out = root().join("ondrane/ui/assets/icons");
    if out.is_dir() {
        for e in std::fs::read_dir(&out).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "svg") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let (files, icons) = figma::extract_icons(&msg, &out).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        exit(1)
    });
    eprintln!(
        "==> {icons} icon variants ({files} SVG layers) written to {}",
        out.display()
    );
    for filename in ["vlc.svg", "vlc.accent.svg"] {
        let _ = std::fs::remove_file(out.join(filename));
    }
    copy(
        &root().join("assets/branding/ondrane-symbol.svg"),
        &out.join("ondrane.accent.svg"),
    );
    app_icons();
}

// ----------------------------------------------------------- packaging
fn install() {
    let exe = build_release();
    licenses::generate(&root());
    #[cfg(windows)]
    {
        let base = PathBuf::from(std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA"))
            .join("Programs/Ondrane");
        copy(&exe, &base.join("ondrane.exe"));
        copy(
            &root().join("packaging/windows/ondrane.ico"),
            &base.join("ondrane.ico"),
        );
        copy(&root().join("LICENSE"), &base.join("LICENSE"));
        copy(
            &root().join("THIRD_PARTY_NOTICES.md"),
            &base.join("THIRD_PARTY_NOTICES.md"),
        );
        copy_dir_all(
            &root().join("target/third-party-licenses"),
            &base.join("licenses"),
        );
        eprintln!("==> installed to {} (use `cargo xtask windows-installer` for shortcuts and file associations)", base.display());
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
        let bin = home.join(".local/bin/ondrane");
        copy(&exe, &bin);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let share = home.join(".local/share");
        copy(
            &root().join("packaging/linux/ondrane.desktop"),
            &share.join("applications/ondrane.desktop"),
        );
        copy(
            &root().join("packaging/ondrane.svg"),
            &share.join("icons/hicolor/scalable/apps/ondrane.svg"),
        );
        for size in [64, 128, 256, 512] {
            copy(
                &root().join(format!(
                    "packaging/linux/icons/{size}x{size}/apps/ondrane.png"
                )),
                &share.join(format!("icons/hicolor/{size}x{size}/apps/ondrane.png")),
            );
        }
        copy(&root().join("LICENSE"), &share.join("ondrane/LICENSE"));
        copy(
            &root().join("THIRD_PARTY_NOTICES.md"),
            &share.join("ondrane/THIRD_PARTY_NOTICES.md"),
        );
        copy_dir_all(
            &root().join("target/third-party-licenses"),
            &share.join("ondrane/licenses"),
        );
        let _ = Command::new("update-desktop-database")
            .arg(share.join("applications"))
            .status();
        eprintln!("==> installed {} and the desktop entry", bin.display());
    }
}

fn appimage() {
    let exe = build_release();
    let root = root();
    let appdir = root.join("target/Ondrane.AppDir");
    let _ = std::fs::remove_dir_all(&appdir);
    copy(&exe, &appdir.join("usr/bin/ondrane"));
    copy(
        &root.join("packaging/linux/ondrane.desktop"),
        &appdir.join("ondrane.desktop"),
    );
    copy(
        &root.join("packaging/linux/ondrane.desktop"),
        &appdir.join("usr/share/applications/ondrane.desktop"),
    );
    copy(
        &root.join("packaging/linux/io.github.lib_locale.ondrane.metainfo.xml"),
        &appdir.join("usr/share/metainfo/io.github.lib_locale.ondrane.metainfo.xml"),
    );
    let icon = root.join("packaging/linux/icons/256x256/apps/ondrane.png");
    copy(&icon, &appdir.join("ondrane.png"));
    copy(&icon, &appdir.join(".DirIcon"));
    copy(
        &icon,
        &appdir.join("usr/share/icons/hicolor/256x256/apps/ondrane.png"),
    );
    copy(
        &root.join("packaging/ondrane.svg"),
        &appdir.join("usr/share/icons/hicolor/scalable/apps/ondrane.svg"),
    );
    licenses::generate(&root);
    copy(
        &root.join("LICENSE"),
        &appdir.join("usr/share/doc/ondrane/LICENSE"),
    );
    copy(
        &root.join("THIRD_PARTY_NOTICES.md"),
        &appdir.join("usr/share/doc/ondrane/THIRD_PARTY_NOTICES.md"),
    );
    copy_dir_all(
        &root.join("target/third-party-licenses"),
        &appdir.join("usr/share/doc/ondrane/licenses"),
    );
    // AppRun uses the system libVLC. Native runtimes are not bundled.
    #[cfg(unix)]
    std::os::unix::fs::symlink("usr/bin/ondrane", appdir.join("AppRun")).unwrap();
    eprintln!("==> AppDir ready at {}", appdir.display());
    eprintln!("    Install VLC 3 separately; this package does not include a native runtime.");
    let out = root.join(format!("target/Ondrane-{}-x86_64.AppImage", version()));
    if Command::new("appimagetool")
        .arg("--version")
        .output()
        .is_ok()
    {
        run(Command::new("appimagetool")
            .env("ARCH", "x86_64")
            .arg(&appdir)
            .arg(&out));
        eprintln!("==> {}", out.display());
    } else {
        eprintln!(
            "    appimagetool not found: install it to produce {}",
            out.display()
        );
    }
}

fn windows_installer() {
    let exe = build_release();
    let stage = root().join("target/Ondrane");
    let _ = std::fs::remove_dir_all(&stage);
    copy(&exe, &stage.join(exe.file_name().unwrap()));
    copy(
        &root().join("packaging/windows/ondrane.ico"),
        &stage.join("ondrane.ico"),
    );
    copy(
        &root().join("packaging/windows/license-summary.txt"),
        &stage.join("license-summary.txt"),
    );
    licenses::generate(&root());
    copy(&root().join("LICENSE"), &stage.join("LICENSE"));
    copy(&root().join("README.md"), &stage.join("README.md"));
    copy(
        &root().join("THIRD_PARTY_NOTICES.md"),
        &stage.join("THIRD_PARTY_NOTICES.md"),
    );
    copy_dir_all(
        &root().join("target/third-party-licenses"),
        &stage.join("licenses"),
    );
    run(Command::new("makensis")
        .current_dir(root().join("target"))
        .arg(format!("-DVERSION={}", version()))
        .arg(format!("-DSRC={}", stage.display()))
        .arg(format!(
            "-DOUT={}",
            root()
                .join(format!("target/Ondrane-Setup-{}.exe", version()))
                .display()
        ))
        .arg(root().join("packaging/windows/ondrane.nsi")));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (task, rest) = args
        .split_first()
        .map(|(t, r)| (t.as_str(), r))
        .unwrap_or(("help", &[]));
    match task {
        "licenses" => licenses::generate(&root()),
        "figma" => figma(rest),
        "app-icons" => app_icons(),
        "run" => run(cargo()
            .args(["run", "--release", "-p", "ondrane", "--"])
            .args(rest)),
        "diag" => run(cargo().args(["run", "--release", "-p", "ondrane", "--", "--diag"])),
        "install" => install(),
        "appimage" => appimage(),
        "windows-installer" => windows_installer(),
        _ => {
            eprintln!("usage: cargo xtask <figma [file.fig] | app-icons | run [args] | diag | install | appimage | windows-installer | licenses>");
            exit(if task == "help" { 0 } else { 2 });
        }
    }
}
