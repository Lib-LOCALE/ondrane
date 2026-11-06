//! Built-in Linux colour schemes, adapted to the existing Slint skin tokens.
//! Official sources and upstream licences: docs/THEMES.md.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    Catppuccin,
    Dracula,
    Nord,
    Gruvbox,
    TokyoNight,
    RosePine,
    #[default]
    #[serde(other)]
    #[serde(rename = "ondrane", alias = "vlc")]
    Ondrane,
}

impl ColorScheme {
    pub const ALL: [Self; 7] = [
        Self::Ondrane,
        Self::Catppuccin,
        Self::Dracula,
        Self::Nord,
        Self::Gruvbox,
        Self::TokyoNight,
        Self::RosePine,
    ];
    pub const NAMES: [&'static str; 7] = [
        "Ondrane",
        "Catppuccin",
        "Dracula",
        "Nord",
        "Gruvbox",
        "Tokyo Night",
        "Rosé Pine",
    ];

    pub fn palette(self, flavor: CatppuccinFlavor) -> Option<Palette> {
        // base, recessed, surface, raised, text, secondary, muted, accent
        let (dark, c) = match self {
            Self::Ondrane => return None,
            Self::Catppuccin => match flavor {
                CatppuccinFlavor::Mocha => (
                    true,
                    [
                        "#1e1e2e", "#11111b", "#181825", "#313244", "#cdd6f4", "#a6adc8",
                        "#6c7086", "#cba6f7",
                    ],
                ),
                CatppuccinFlavor::Macchiato => (
                    true,
                    [
                        "#24273a", "#181926", "#1e2030", "#363a4f", "#cad3f5", "#a5adcb",
                        "#6e738d", "#c6a0f6",
                    ],
                ),
                CatppuccinFlavor::Frappe => (
                    true,
                    [
                        "#303446", "#232634", "#292c3c", "#414559", "#c6d0f5", "#b5bfe2",
                        "#737994", "#ca9ee6",
                    ],
                ),
                CatppuccinFlavor::Latte => (
                    false,
                    [
                        "#eff1f5", "#dce0e8", "#e6e9ef", "#ccd0da", "#4c4f69", "#4c4f69",
                        "#9ca0b0", "#8839ef",
                    ],
                ),
            },
            Self::Dracula => (
                true,
                [
                    "#282a36", "#21222c", "#282a36", "#44475a", "#f8f8f2", "#f8f8f2", "#6272a4",
                    "#bd93f9",
                ],
            ),
            Self::Nord => (
                true,
                [
                    "#2e3440", "#2e3440", "#3b4252", "#434c5e", "#eceff4", "#d8dee9", "#4c566a",
                    "#88c0d0",
                ],
            ),
            Self::Gruvbox => (
                true,
                [
                    "#282828", "#1d2021", "#32302f", "#3c3836", "#ebdbb2", "#d5c4a1", "#665c54",
                    "#fabd2f",
                ],
            ),
            Self::TokyoNight => (
                true,
                [
                    "#1a1b26", "#16161e", "#1a1b26", "#292e42", "#c0caf5", "#a9b1d6", "#565f89",
                    "#7aa2f7",
                ],
            ),
            Self::RosePine => (
                true,
                [
                    "#191724", "#16141f", "#1f1d2e", "#26233a", "#e0def4", "#908caa", "#6e6a86",
                    "#c4a7e7",
                ],
            ),
        };
        Some(Palette {
            dark,
            base: c[0],
            recessed: c[1],
            surface: c[2],
            raised: c[3],
            text: c[4],
            secondary: c[5],
            muted: c[6],
            accent: c[7],
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CatppuccinFlavor {
    Macchiato,
    Frappe,
    Latte,
    #[default]
    #[serde(other)]
    Mocha,
}
impl CatppuccinFlavor {
    pub const ALL: [Self; 4] = [Self::Mocha, Self::Macchiato, Self::Frappe, Self::Latte];
    pub const NAMES: [&'static str; 4] = ["Mocha", "Macchiato", "Frappé", "Latte"];
}

pub struct Palette {
    pub dark: bool,
    pub base: &'static str,
    pub recessed: &'static str,
    pub surface: &'static str,
    pub raised: &'static str,
    pub text: &'static str,
    pub secondary: &'static str,
    pub muted: &'static str,
    pub accent: &'static str,
}
impl Palette {
    pub fn tokens(&self) -> [(&'static str, &'static str); 20] {
        [
            ("bg-canvas", "#000000"),
            ("bg-b1", self.base),
            ("bg-b2", self.recessed),
            ("bg-a1", self.raised),
            ("bg-a2", self.raised),
            ("bg-a3", self.surface),
            ("bg-a4", self.surface),
            ("bg-a5", self.recessed),
            ("titlebar", self.surface),
            ("well", self.recessed),
            ("thumb-bg", self.surface),
            ("fg-1", self.text),
            ("fg-2", self.text),
            ("fg-a1", self.secondary),
            ("fg-a2", self.secondary),
            ("fg-a3", self.secondary),
            ("fg-a4", self.muted),
            ("knob-off", self.muted),
            ("divider-dim", self.raised),
            ("divider-bright", self.muted),
        ]
    }
}

/// Black/white choice with the greater WCAG contrast on a solid accent.
pub fn accent_uses_light_ink(r: u8, g: u8, b: u8) -> bool {
    let luminance: f64 = [r, g, b]
        .into_iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(v, weight)| {
            let v = v as f64 / 255.0;
            weight
                * if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
        })
        .sum();
    luminance < 0.179
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(hex: &str) -> f64 {
        let c = u32::from_str_radix(&hex[1..], 16).unwrap();
        [16, 8, 0]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(shift, weight)| {
                let v = ((c >> shift) & 255) as f64 / 255.0;
                weight
                    * if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
    #[test]
    fn all_palettes_have_readable_text_and_complete_skin_tokens() {
        for scheme in ColorScheme::ALL {
            for flavor in CatppuccinFlavor::ALL {
                let Some(p) = scheme.palette(flavor) else {
                    continue;
                };
                let c = u32::from_str_radix(&p.accent[1..], 16).unwrap();
                let ink = if accent_uses_light_ink((c >> 16) as u8, (c >> 8) as u8, c as u8) {
                    "#ffffff"
                } else {
                    "#000000"
                };
                let (a, b) = (luminance(ink), luminance(p.accent));
                assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
                let mut names: Vec<_> = p.tokens().iter().map(|(k, _)| *k).collect();
                names.sort_unstable();
                names.dedup();
                assert_eq!(names.len(), 20);
                for fg in [p.text, p.secondary] {
                    for bg in [p.base, p.recessed, p.surface, p.raised] {
                        let (a, b) = (luminance(fg), luminance(bg));
                        let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                        assert!(ratio >= 4.5, "{scheme:?}/{flavor:?}: {fg} on {bg}: {ratio}");
                    }
                }
            }
        }
    }
    #[test]
    fn persisted_selection_is_backward_and_forward_compatible() {
        let old = crate::settings::OndraneSettings::from_json("{}").unwrap();
        assert_eq!(old.app.global.color_scheme, ColorScheme::Ondrane);
        for scheme in ColorScheme::ALL {
            for flavor in CatppuccinFlavor::ALL {
                let mut s = old.clone();
                s.app.global.color_scheme = scheme;
                s.app.global.catppuccin_flavor = flavor;
                assert_eq!(
                    crate::settings::OndraneSettings::from_json(&s.to_json().unwrap()).unwrap(),
                    s
                );
            }
        }
        let s = crate::settings::OndraneSettings::from_json(r#"{"app":{"global":{"color_scheme":"future-theme","catppuccin_flavor":"future-flavor"}}}"#).unwrap();
        assert_eq!(s.app.global.color_scheme, ColorScheme::Ondrane);
        assert_eq!(s.app.global.catppuccin_flavor, CatppuccinFlavor::Mocha);
    }
}
