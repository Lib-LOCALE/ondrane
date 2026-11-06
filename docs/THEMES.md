# Colour schemes

Settings → General → Appearance provides Ondrane’s own customisable dark/light/system scheme and six adapted theme families:

| Theme | Source | Licence |
| --- | --- | --- |
| Catppuccin — Mocha, Macchiato, Frappé, Latte | https://github.com/catppuccin/palette | MIT |
| Dracula | https://spec.draculatheme.com/ | MIT |
| Nord | https://www.nordtheme.com/docs/colors-and-palettes/ | MIT |
| Gruvbox dark medium | https://github.com/morhetz/gruvbox | MIT/X11, declared in upstream README |
| Tokyo Night | https://github.com/folke/tokyonight.nvim | Apache-2.0 |
| Rosé Pine main | https://github.com/rose-pine/neovim | MIT |

These are adaptations, not official ports or a statistical ranking of Linux theme usage. No Vim/Lua code is executed. Notices are in `assets/theme-licenses` and are copied into distribution packages.

Colour-scheme choices apply immediately and survive a restart. Selecting a built-in palette disables a custom JSON skin without deleting its path. Returning to Ondrane restores its stored mode and custom accent. Catppuccin Latte is light; the other supplied theme variants are dark. Background surfaces, text roles and accent colours map to the existing UI skin tokens.

Tests check persistence, fallback for unknown scheme identifiers, and main/secondary text and solid-button contrast for the six palette families. Frappé secondary text uses `subtext1` and Latte uses `text` rather than low-contrast comment colours. Solid-button labels choose black or white according to the accent. The `desktop_themes` integration test selects, saves and renders every supplied variant.
