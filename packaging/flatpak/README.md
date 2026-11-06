# Upstream Linux Flatpak

This directory builds Ondrane for distribution on **GitHub Releases**. The application ID is `io.github.lib_locale.ondrane`: the underscore maps to the hyphen in the GitHub account `Lib-LOCALE`.

## Build

Install Flatpak and add the Flathub remote, then install the build tools:

```sh
flatpak install --user flathub org.flatpak.Builder \
  org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
./packaging/flatpak/build.sh
```

The build downloads checksummed sources first, then compiles offline. It builds libebml, libmatroska, FFmpeg 4.4.7 and libVLC 3.0.23 from their upstream source archives. Rust crate sources come from the committed lockfile through the official [flatpak-cargo-generator](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo). Regenerate `cargo-sources.json` whenever the lockfile changes. The supported Freedesktop 25.08 runtime supplies GStreamer and PipeWire support. Parallelism defaults to eight jobs; override `CARGO_BUILD_JOBS` if needed.

The output is `target/ondrane-<version>-<arch>.flatpak`. Install and run it with:

```sh
flatpak install --user ./target/ondrane-0.1.0-x86_64.flatpak
flatpak run io.github.lib_locale.ondrane
```

The Flatpak includes its playback engine; **no host VLC installation is needed**. Native Windows/macOS/standalone Linux packages still require VLC 3 separately. The release's Flatpak source archive includes the native dependency source archives, Rust crate download cache, application source and build instructions.

## Sandbox

- Wayland, with X11 fallback, and DRI for rendering.
- Audio playback/recording through PulseAudio/PipeWire compatibility; network access for streams.
- FileChooser and ScreenCast use desktop portals. No blanket home-directory, host-filesystem or session-bus access.
- Status notifier access is limited to `org.kde.StatusNotifierWatcher`.
- Direct webcam/optical-disc access is not granted. Those native-device features are outside this package's default permissions.
- Settings/history are separate from a native install, under `~/.var/app/io.github.lib_locale.ondrane`. No host-library lookup is required.

The GitHub Flatpak is a local bundle, not an automatic-update repository. Install later release bundles to update it. The runtime continues receiving updates through Flathub.

## Store status and AI assistance

This upstream manifest and the distribution tooling were authored with AI assistance. **Do not copy this manifest into a Flathub submission.** Flathub's current [policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy) prohibits AI-assisted manifests and AI-agent submission PRs, descriptions and review interactions. A human maintainer must independently author a compliant manifest and handle that process. Significant application implementation, rebranding, tests, documentation and packaging were also AI-assisted; this must be disclosed by a human submitter with an honest estimate of extent. No Flathub approval is claimed.

COSMIC Store already lists configured Flathub applications; a second store submission is normally unnecessary. The separate [COSMIC Flatpak repository](https://github.com/pop-os/cosmic-flatpak) is primarily for COSMIC-specific software unsuitable for Flathub, and acceptance of a general media player there is a maintainer decision. Ondrane is built with Slint and does not claim to be a libcosmic application.
