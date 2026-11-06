# Third-party notices

Ondrane application code is MIT-licensed. Preserve the copyright notices in `LICENSE`, including Nirmal Joseph Ukken’s original notice and the Ondrane contributors’ notice. Other components retain the terms below; MIT is not a blanket licence for every resource.

## Interface design

**[VLC Redesign](https://www.figma.com/community/file/1691465999134370980/vlc-redesign)**, by **[Juxtopposed](https://www.figma.com/@juxtopposed)**.

- Source: https://www.figma.com/community/file/1691465999134370980/vlc-redesign
- Creator: https://www.figma.com/@juxtopposed
- Licence: **Creative Commons Attribution 4.0 International**, https://creativecommons.org/licenses/by/4.0/
- The exact listing was inspected on 6 November 2026 and explicitly displayed CC BY 4.0.

The interface layout and extracted UI pictograms adapt this design. Changes include the Rust/Slint implementation, Linux desktop portals, light-mode adaptations, six additional colour-scheme families, accessibility fixes and the original Ondrane identity. Credit the original creator, retain the source/licence links and identify adaptations when redistributing these resources. The original Figma archive, preview photos and mockup exports are not included in this repository. Supplied screenshots use generated test media.

The listing identifies a concept, disclaims affiliation with links posted in its comments and encourages contributions directly to VLC. Neither the listing nor this adaptation implies endorsement by Juxtopposed or VideoLAN. Creative Commons does not grant trademark rights or rights an author does not hold in independently sourced third-party material.

## Slint

Slint **1.18.1**, https://slint.dev/ · https://github.com/slint-ui/slint

Ondrane uses Slint under the **Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0**. It includes the official `AboutSlint` widget in an About dialog accessible from the top-level menu, as well as the attribution badge on the public README. The exact licence is reproduced in `docs/legal/LicenseRef-Slint-Royalty-free-2.0.md` and in the generated distribution notices. Its GPL/commercial alternatives are not the licence choice made for this application.

## Rust dependencies

`cargo xtask licenses` reads the locked dependency graph and copies upstream licence, copyright and notice files into `target/third-party-licenses`. It also writes `RUST-DEPENDENCIES.csv` with package versions, declared licence expressions and upstream repositories. This inventory includes optional/build dependencies and other target platforms; it is broader than any one executable’s linked components.

Dependencies include MIT, Apache-2.0, BSD, ISC, Zlib, BSL-1.0, Unicode-3.0 and Unlicense terms, not just MIT/Apache. Preserve their applicable conditions. A choice (`OR`) is different from cumulative conditions (`AND`); Unicode data notices can apply in addition to a crate’s source licence.

**option-ext 0.2.0** is **MPL-2.0**. Its MPL-covered source, licence and manifest are included under `licenses/option-ext-0.2.0/source` in distribution packages. That source is available under MPL-2.0, including any changes made to those covered files. Upstream source is also available from https://static.crates.io/crates/option-ext/option-ext-0.2.0.crate and https://github.com/soc/option-ext. This file-level condition does not relicense all application code as MPL.

The generated bundle also includes `rust/COPYRIGHT-library.html`, its linked
licence texts and the Rust toolchain version. These notices cover the standard
library/runtime material that Cargo’s dependency graph does not enumerate.

## Font

**Inter**, The Inter Project Authors, https://rsms.me/inter/ · https://github.com/rsms/inter

The bundled OTF font files use SIL Open Font License 1.1. The supplied upstream/Debian notice, including its OFL and Apache sections, is preserved in `ondrane/ui/assets/fonts/LICENSE-Inter.txt` and copied into distributions as `licenses/Inter.txt`.

## Colour palettes

These are adapted colour values, not executed Vim/Lua theme code. Upstream notices are preserved in `assets/theme-licenses/` and copied to `licenses/themes/` in distributions.

| Palette | Upstream | Terms |
| --- | --- | --- |
| Catppuccin | https://github.com/catppuccin/palette | MIT |
| Dracula | https://github.com/dracula/dracula-theme · https://spec.draculatheme.com/ | MIT |
| Nord | https://github.com/nordtheme/nord | MIT |
| Gruvbox | https://github.com/morhetz/gruvbox | MIT/X11 as declared in the upstream README; the local notice records that declaration and source, since upstream does not provide a separate LICENSE file |
| Tokyo Night | https://github.com/folke/tokyonight.nvim | Apache-2.0 |
| Rosé Pine | https://github.com/rose-pine/neovim | MIT |

No affiliation with the theme authors is claimed. Some text colours are adapted to maintain contrast in this interface.

## Playback and external software

**libVLC**, by [VideoLAN and contributors](https://www.videolan.org/videolan/), https://www.videolan.org/vlc/libvlc.html · https://code.videolan.org/videolan/vlc

libVLC is LGPL-2.1-or-later; some VLC plugins and dependent libraries have GPL or other terms. Ondrane loads the user’s separately installed VLC 3 runtime dynamically. Native archives and Windows/macOS installers do not copy VLC DLLs, native libraries, plugins or codecs into packages. The Linux Flatpak is different: its manifest compiles libVLC 3.0.23, FFmpeg 4.4.7 (without `--enable-gpl`), libebml 1.4.5 and libmatroska 1.7.1 from pinned source archives and installs their licence texts. VLC plugin licences vary. The matching Flatpak source archive is distributed alongside the binary bundle, including exact dependency archives and the application source. The Freedesktop runtime is separately managed by Flatpak and retains its own licences. Installing or redistributing those separately requires compliance with their applicable licences; Ondrane’s MIT licence does not replace them.

Wayland screen/window capture invokes separately installed GStreamer/PipeWire components. Update checks use the system `curl`. These programs are not embedded in Ondrane packages. Native system libraries and vendor SDKs used by a platform remain subject to their own terms.

## Original branding

The Ondrane orbit/play logo, wordmarks and animation were authored for this project in VectorCraft and EffectCraft. Editable sources and SVG/Lottie exports are in `assets/branding`, licensed under the repository’s MIT terms, Copyright © 2026 Ondrane Contributors. The wordmark outlines derive from Inter under its OFL terms. The design uses no VideoLAN cone or VLC logo.

“VLC” and “VideoLAN” identify VideoLAN’s software and marks where credited. **Ondrane is an independent application, not an official VLC release or an endorsed successor.**
