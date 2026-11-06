# Validation scope

This is a 0.1 preview, not a claim that every hardware/desktop combination is supported.

## Locally exercised on 6 November 2026

Linux with a COSMIC Wayland desktop, CPU-rendered Slint, and libVLC 3.0.20:

- Ordinary Rust suite: **45 library tests plus one developer-tool test passed**. Desktop/hardware tests are opt-in and excluded from that count.
- GUI playback scenario: decoded frames reach the window; pause, volume/mute, speed, seek, fullscreen, library/settings navigation, second media, engine restart and Stop are exercised. Stop also checks that stale queued frames cannot reappear.
- Real pointer dispatch: Open Files, Network Stream, canvas background, mini-player Stop and play/pause all receive clicks. This reproduces the overlay bug that direct controller-command tests missed.
- Colour schemes: all ten supplied family/flavour selections are applied, saved, reloaded and rendered, including the four Catppuccin variants and the return to Ondrane.
- New identity: welcome, AboutSlint attribution and Appearance settings rendered and inspected. Static icon checked at 24 and 512 pixels; VectorCraft exports and EffectCraft Lottie report no export warnings.
- Legacy-data migration: existing Ondrane files are not overwritten; original settings/history are preserved and scripts are not copied.
- Notice assembly: 661 third-party crates in the resolved workspace, plus Inter and palette notices; MPL `option-ext` source is included. Optional/build/other-platform dependencies are included in that inventory and may not all be linked into a given executable.

The inherited Linux integration was also previously exercised with COSMIC’s real screen selector at 2560×1440, a USB webcam/microphone, NVIDIA OpenGL UI rendering and separate-window VDPAU playback. Those device checks predate the Ondrane rename and are not substitutes for retesting on another machine. The rename campaign uses software rendering to keep other GPU workloads available.

## Automated CI

The public workflow builds and tests Linux, Windows and macOS. Linux runs isolated FileChooser/ScreenCast protocol tests, synthetic capture transport, and headless Wayland/X11 desktop scenarios. The headless runner uses a PulseAudio null sink so missing physical sound hardware does not create native audio-error dialogs during input tests. Its software renderer does not test physical GPU acceleration or a user’s actual screen-sharing permission dialog. Passing CI does not establish macOS/Windows playback or device parity; native runtime/peripheral tests remain separate.

## Upstream Flatpak validation

The x86_64 GitHub Flatpak uses Freedesktop Platform 25.08 and a source-built libVLC 3.0.23 / FFmpeg 4.4.7 playback engine. All 46 ordinary tests also passed in the SDK's release build. The release integration-test executable was then exercised against the **Platform runtime**, with a read-only app directory, generated media, isolated settings, no GPU device access, and software-rendered Slint on the COSMIC Wayland desktop:

- `desktop_playback`: all nine playback/navigation/restart/Stop steps passed.
- `desktop_empty_player_clicks`: Open Files, Network Stream and mini-player pointer dispatch passed.
- `desktop_branding`: welcome/About/Appearance rendering passed.
- `capture_pipe_transport`: synthetic GStreamer-to-libVLC transport and recording decode passed.
- `--diag`: loads `/app/lib/libvlc.so.5`, creates a player and reports libVLC 3.0.23. The VLC timestamp compatibility patch and final cache regeneration prevent stale plugin-cache errors after stripping and OSTree export.

These are real runtime tests, but the file-selection action test does not automate the protected desktop chooser, and synthetic capture does not repeat a user's real ScreenCast consent. Manifest lint has no errors and one runtime-upgrade advisory (26.08 is available; 25.08 remains the chosen supported branch). AppStream XML validation passes. Flathub's repository linter reports its two screenshot-hosting requirements because this is a GitHub-hosted upstream bundle, not a Flathub-hosted repository. Do not describe that result as a Flathub approval.

## Remaining limits

- libVLC may emit vmem/decoder/timestamp diagnostics during probing, seek and shutdown even when the tested playback scenario succeeds. These are not globally suppressed, and the project does not claim a warning-free native engine log.
- Embedded video uses CPU decoding and pixel copies; zero-copy GPU video and HDR passthrough are not implemented.
- Native Wayland drag-and-drop is limited by winit 0.30; optional XWayland mode provides a workaround.
- Optical media, removable storage and Chromecast behaviour have not been verified with matching physical devices in this campaign.
- macOS preview packages are unsigned and not notarised. No Windows/macOS hardware certification is claimed.

Private local runtime files, screen recordings, personal configuration and raw desktop-capture screenshots are excluded from the public source tree. Public screenshots show only generated media, including a wordmark demo rendered in EffectCraft.
