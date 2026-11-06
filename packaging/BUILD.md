# Building and packaging Ondrane

Use stable Rust (minimum 1.92 for Slint 1.18), a working C/C++ toolchain and VLC 3 installed separately. Build with the committed lockfile.

```sh
cargo build --release --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Linux

On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libxkbcommon-x11-dev vlc
cargo xtask install
```

The per-user installer writes `~/.local/bin/ondrane`, a desktop entry, icons and notices under `~/.local/share/ondrane`. It does not install a native playback runtime. `cargo xtask appimage` stages an AppDir and creates an AppImage if `appimagetool` is available. The AppImage still needs a compatible VLC 3 installation; it is not a self-contained codec bundle. Official release archives contain the executable and notices and are built on Ubuntu 24.04; older distributions may need to build locally.

## Windows

Install the stable Rust MSVC toolchain, Visual Studio C++ Build Tools and NSIS 3 (`makensis` on PATH). Install 64-bit VLC 3 separately for playback.

```powershell
cargo build --release --locked
cargo xtask windows-installer
```

The installer is written to `target/Ondrane-Setup-<version>.exe`. It is per-user and includes the application and notices. It deliberately does not copy adjacent VLC DLLs or plugins. Portable archives are assembled by the release workflow.

## macOS

Install Xcode Command Line Tools, stable Rust and VLC 3. Build with `cargo build --release --locked`. The release workflow packages `Ondrane.app` using `packaging/macos/Info.plist` and the ICNS resource; its `Contents/Resources` includes third-party notices. The app loads libVLC from the separately installed VLC app or a custom path. The initial preview is unsigned and not notarised; native playback and device parity are not established by a CI build alone.

## Required distribution notices

```sh
cargo xtask licenses
```

This generates `target/third-party-licenses` from `cargo metadata --locked`, copies available upstream licence/copyright/notice files (including alternative licence texts), records versions in a CSV, includes MPL-covered `option-ext` source, and adds Inter/palette notices and the installed Rust toolchain’s standard-library copyright/licence bundle. It fails if a crate has no declared licence or no notice material can be found. README/source-header fallback material is retained for crates that put their terms there. This is an assembly tool, not an automatic legal interpretation of every licence expression or an inventory of separately installed native libraries.

Ship the generated directory, root `LICENSE` and `THIRD_PARTY_NOTICES.md` with the executable. The Windows, AppImage and CI release packaging paths do this. If distributing additional runtimes yourself, audit their exact-build licences and corresponding source obligations separately; do not call such a package an official Ondrane runtime bundle without doing that work.

## Artwork

`cargo xtask app-icons` derives platform PNG, ICO and ICNS files from `packaging/ondrane.svg`. That canonical SVG was exported from `assets/branding/ondrane.vectorcraft` using VectorCraft. Motion is authored in `assets/branding/ondrane-motion.ecproj` using EffectCraft. See the branding brief for native sources, Lottie and SVG/GIF exports.

The original Juxtopposed Figma archive is not redistributed. To regenerate its CC BY pictograms, obtain the source from its Community listing, export a `.fig`, then run `cargo xtask figma /path/to/design.fig`. The task keeps the separately authored Ondrane app symbol and excludes the original cone. Preserve the attribution in `THIRD_PARTY_NOTICES.md`.
