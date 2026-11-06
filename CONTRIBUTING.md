# Contributing to Ondrane

Use the current stable Rust toolchain and follow the build instructions in the README. Keep changes focused, preserve existing settings and document user-visible limitations. Include a regression test for a reproduced bug; test actual pointer input when changing click handling.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
cargo xtask licenses
```

Linux desktop integration tests are opt-in because they open real windows or use desktop portals. Set isolated `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` paths before running them. `SLINT_BACKEND=winit-software` allows CPU-only checks. `VLC_SMOKE_OUTPUT` selects a screenshot directory. The `desktop_playback` test also needs two generated, 30-second video fixtures in `VLC_SMOKE_MEDIA` and `VLC_SMOKE_NEXT`. See `.github/workflows/ci.yml` for the complete headless Wayland/X11 setup.

```sh
dbus-run-session -- cargo test --workspace filechooser_response_and_cancel -- --ignored
dbus-run-session -- cargo test --workspace screencast_response_fd_cancel_and_close -- --ignored
SLINT_BACKEND=winit-software cargo test --workspace desktop_empty_player_clicks -- --ignored
```

Do not publish private media, credentials, real desktop-capture screenshots or local runtime libraries. Use generated test patterns for screenshots. New visual assets should include editable sources and explicit licence/provenance information. The native branding files are in `assets/branding`; an animated SVG is not a substitute for the static platform icon.

Application code contributions use the repository’s MIT licence. Existing assets retain their own licences; do not relicense the CC BY design contribution as MIT. Please mention the operating system, desktop/compositor, Ondrane version and libVLC version when reporting a playback or portal issue.
