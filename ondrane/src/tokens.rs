//! Design tokens extracted directly from Figma: "VLC Redesign (Community).fig"
//!
//! Provides pixel-perfect Dark Mode and Light Mode color palettes,
//! typography tokens, border radii, spacings, and theme styling.

use serde::{Deserialize, Serialize};

/// Color representation with 8-bit RGBA channels and static hex representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hex: &'static str,
}

impl Color {
    pub const fn from_hex_rgb(hex: &'static str, r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            a: 255,
            hex,
        }
    }

    pub const fn from_hex_rgba(hex: &'static str, r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a, hex }
    }

    /// Return (r, g, b) tuple in range 0..=255.
    pub const fn to_rgb(&self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// Return (r, g, b, a) tuple in range 0..=255.
    pub const fn to_rgba(&self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }

    /// Return normalized floats in range 0.0..=1.0.
    pub fn to_f32_rgba(&self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }

    /// CSS representation: "rgba(r, g, b, a)"
    pub fn to_css_rgba(&self) -> String {
        format!(
            "rgba({}, {}, {}, {:.3})",
            self.r,
            self.g,
            self.b,
            self.a as f32 / 255.0
        )
    }

    /// CSS hex string: "#RRGGBB"
    pub const fn to_hex(&self) -> &'static str {
        self.hex
    }
}

// ============================================================================
// DARK MODE TOKENS (Figma Page "Internal Only Canvas")
// ============================================================================

pub mod dark {
    use super::Color;

    pub mod accent {
        use super::Color;
        /// Signature Ondrane Mint (#78E3CB)
        pub const MAIN: Color = Color::from_hex_rgb("#78E3CB", 120, 227, 203);
        /// Hover & highlight state (#9BEBDA)
        pub const BRIGHT: Color = Color::from_hex_rgb("#9BEBDA", 155, 235, 218);
        /// Inactive / pressed state (#378C7B)
        pub const DIM: Color = Color::from_hex_rgb("#378C7B", 55, 140, 123);
        /// Logo signature mint accent (#78E3CB)
        pub const LOGO: Color = Color::from_hex_rgb("#78E3CB", 120, 227, 203);
    }

    pub mod bg {
        use super::Color;
        /// Floating surfaces, dialog cards, popup menus (#222222)
        pub const A1: Color = Color::from_hex_rgb("#222222", 34, 34, 34);
        /// Secondary panels, playlist sidebar (#1F1F1F)
        pub const A2: Color = Color::from_hex_rgb("#1F1F1F", 31, 31, 31);
        /// Side drawers, settings category sidebar (#1B1B1B)
        pub const A3: Color = Color::from_hex_rgb("#1B1B1B", 27, 27, 27);
        /// Primary floating control bars & toolbar (#181818)
        pub const A4: Color = Color::from_hex_rgb("#181818", 24, 24, 24);
        /// Modal windows and deep overlay background (#151515)
        pub const A5: Color = Color::from_hex_rgb("#151515", 21, 21, 21);
        /// Video canvas & letterbox surround (#0E0E0E)
        pub const B1: Color = Color::from_hex_rgb("#0E0E0E", 14, 14, 14);
        /// Deepest background canvas (#090909)
        pub const B2: Color = Color::from_hex_rgb("#090909", 9, 9, 9);
        /// Subtle orange-tinted background (#1A1611)
        pub const ACCENT_DARK: Color = Color::from_hex_rgb("#1A1611", 26, 22, 17);
        /// Active badge / selection background (#371F06)
        pub const ACCENT_BRIGHT: Color = Color::from_hex_rgb("#371F06", 55, 31, 6);
    }

    pub mod fg {
        use super::Color;
        /// Primary titles and dominant text (#EAEAEA)
        pub const FG1: Color = Color::from_hex_rgb("#EAEAEA", 234, 234, 234);
        /// Secondary text and metadata labels (#C6C6C6)
        pub const FG2: Color = Color::from_hex_rgb("#C6C6C6", 198, 198, 198);
        /// Labels, timestamps, subtle headers (#969696)
        pub const A1: Color = Color::from_hex_rgb("#969696", 150, 150, 150);
        /// Secondary icons and buttons (#8F8F8F)
        pub const A2: Color = Color::from_hex_rgb("#8F8F8F", 143, 143, 143);
        /// Disabled icons, placeholders, hints (#6B6B6B)
        pub const A3: Color = Color::from_hex_rgb("#6B6B6B", 107, 107, 107);
        /// Subtle borders and divider outlines (#3F3F3F)
        pub const A4: Color = Color::from_hex_rgb("#3F3F3F", 63, 63, 63);
    }

    pub mod divider {
        use super::Color;
        /// Visible card separator line (#363636)
        pub const BRIGHT: Color = Color::from_hex_rgb("#363636", 54, 54, 54);
        /// Subtle background section divider (#212121)
        pub const DIM: Color = Color::from_hex_rgb("#212121", 33, 33, 33);
    }
}

// ============================================================================
// LIGHT MODE TOKENS (Figma Page "Internal Only Canvas" & Frame 11)
// ============================================================================

pub mod light {
    use super::Color;

    pub mod accent {
        use super::Color;
        /// Signature light accent (#147D69)
        pub const MAIN: Color = Color::from_hex_rgb("#147D69", 20, 125, 105);
        /// Bright highlight (#178B75)
        pub const BRIGHT: Color = Color::from_hex_rgb("#178B75", 23, 139, 117);
        /// Dimmed / pressed state (#106452)
        pub const DIM: Color = Color::from_hex_rgb("#106452", 16, 100, 82);
    }

    pub mod bg {
        use super::Color;
        /// Card and modal surfaces (#EDEDED)
        pub const A1: Color = Color::from_hex_rgb("#EDEDED", 237, 237, 237);
        /// Secondary panels (#DDDDDD)
        pub const A2: Color = Color::from_hex_rgb("#DDDDDD", 221, 221, 221);
        /// Sidebars and drawers (#D5D5D5)
        pub const A3: Color = Color::from_hex_rgb("#D5D5D5", 213, 213, 213);
        /// Control bars and input backings (#B3B3B3)
        pub const A4: Color = Color::from_hex_rgb("#B3B3B3", 179, 179, 179);
        /// Window background (#E7E7E7)
        pub const B1: Color = Color::from_hex_rgb("#E7E7E7", 231, 231, 231);
        /// Base canvas (#DEDEDE)
        pub const B2: Color = Color::from_hex_rgb("#DEDEDE", 222, 222, 222);
    }

    pub mod fg {
        use super::Color;
        /// Primary dark text for light mode (#3B3B3B)
        pub const A1: Color = Color::from_hex_rgb("#3B3B3B", 59, 59, 59);
        /// Deep high-contrast text (#2D2D2D)
        pub const A2: Color = Color::from_hex_rgb("#2D2D2D", 45, 45, 45);
        /// Secondary muted text and placeholders (#6B6B6B)
        pub const A3: Color = Color::from_hex_rgb("#6B6B6B", 107, 107, 107);
    }

    pub mod divider {
        use super::Color;
        pub const BRIGHT: Color = Color::from_hex_rgb("#BEBEBE", 190, 190, 190);
        pub const DIM: Color = Color::from_hex_rgb("#D0D0D0", 208, 208, 208);
    }
}

// ============================================================================
// GEOMETRIC & LAYOUT TOKENS
// ============================================================================

pub mod radius {
    /// Subtle rounding for chips, menus and inner controls (2px)
    pub const XS: f32 = 2.0;
    /// Standard component radius (4px)
    pub const SM: f32 = 4.0;
    /// Cards, dialogs and floating panels (8px)
    pub const MD: f32 = 8.0;
    /// Large containers (12px)
    pub const LG: f32 = 12.0;
    /// Capsule pills for tags and status badges (16px)
    pub const PILL: f32 = 16.0;
    /// Perfectly circular buttons and icon toggles
    pub const FULL: f32 = 9999.0;
}

pub mod spacing {
    pub const SPACE_2: f32 = 2.0;
    pub const SPACE_4: f32 = 4.0;
    pub const SPACE_6: f32 = 6.0;
    pub const SPACE_8: f32 = 8.0;
    pub const SPACE_10: f32 = 10.0;
    pub const SPACE_12: f32 = 12.0;
    pub const SPACE_16: f32 = 16.0;
    pub const SPACE_20: f32 = 20.0;
    pub const SPACE_24: f32 = 24.0;
    pub const SPACE_32: f32 = 32.0;
    pub const SPACE_40: f32 = 40.0;
    pub const SPACE_48: f32 = 48.0;
}

pub mod typography {
    pub const FONT_FAMILY: &str =
        "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";
    pub const FONT_FAMILY_MONO: &str = "'JetBrains Mono', 'Fira Code', monospace";

    pub const SIZE_XS: f32 = 11.0;
    pub const SIZE_SM: f32 = 12.0;
    pub const SIZE_BASE: f32 = 14.0;
    pub const SIZE_MD: f32 = 16.0;
    pub const SIZE_LG: f32 = 18.0;
    pub const SIZE_XL: f32 = 22.0;
    pub const SIZE_2XL: f32 = 28.0;

    pub const WEIGHT_REGULAR: u16 = 400;
    pub const WEIGHT_MEDIUM: u16 = 500;
    pub const WEIGHT_SEMIBOLD: u16 = 600;
    pub const WEIGHT_BOLD: u16 = 700;
}

// ============================================================================
// THEME CONTAINER
// ============================================================================

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

/// Dynamic theme palette snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemePalette {
    pub mode: ThemeMode,
    pub accent_main: String,
    pub accent_bright: String,
    pub accent_dim: String,
    pub bg_surface: String,
    pub bg_panel: String,
    pub bg_canvas: String,
    pub bg_control: String,
    pub fg_primary: String,
    pub fg_secondary: String,
    pub fg_muted: String,
    pub border: String,
    pub divider: String,
}

impl ThemePalette {
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent_main: dark::accent::MAIN.hex.to_string(),
            accent_bright: dark::accent::BRIGHT.hex.to_string(),
            accent_dim: dark::accent::DIM.hex.to_string(),
            bg_surface: dark::bg::A1.hex.to_string(),
            bg_panel: dark::bg::A2.hex.to_string(),
            bg_canvas: dark::bg::B1.hex.to_string(),
            bg_control: dark::bg::A4.hex.to_string(),
            fg_primary: dark::fg::FG1.hex.to_string(),
            fg_secondary: dark::fg::FG2.hex.to_string(),
            fg_muted: dark::fg::A1.hex.to_string(),
            border: dark::fg::A4.hex.to_string(),
            divider: dark::divider::DIM.hex.to_string(),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            accent_main: light::accent::MAIN.hex.to_string(),
            accent_bright: light::accent::BRIGHT.hex.to_string(),
            accent_dim: light::accent::DIM.hex.to_string(),
            bg_surface: light::bg::A1.hex.to_string(),
            bg_panel: light::bg::A2.hex.to_string(),
            bg_canvas: light::bg::B1.hex.to_string(),
            bg_control: light::bg::A4.hex.to_string(),
            fg_primary: light::fg::A1.hex.to_string(),
            fg_secondary: light::fg::A2.hex.to_string(),
            fg_muted: light::fg::A3.hex.to_string(),
            border: light::divider::BRIGHT.hex.to_string(),
            divider: light::divider::DIM.hex.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_mode_tokens() {
        assert_eq!(dark::accent::MAIN.hex, "#78E3CB");
        assert_eq!(dark::accent::MAIN.to_rgb(), (120, 227, 203));
        assert_eq!(dark::accent::BRIGHT.hex, "#9BEBDA");
        assert_eq!(dark::accent::BRIGHT.to_rgb(), (155, 235, 218));
        assert_eq!(dark::accent::DIM.hex, "#378C7B");

        assert_eq!(dark::bg::A1.hex, "#222222");
        assert_eq!(dark::bg::A2.hex, "#1F1F1F");
        assert_eq!(dark::bg::A3.hex, "#1B1B1B");
        assert_eq!(dark::bg::A4.hex, "#181818");
        assert_eq!(dark::bg::A5.hex, "#151515");
        assert_eq!(dark::bg::B1.hex, "#0E0E0E");
        assert_eq!(dark::bg::B2.hex, "#090909");

        assert_eq!(dark::fg::FG1.hex, "#EAEAEA");
        assert_eq!(dark::fg::FG2.hex, "#C6C6C6");
        assert_eq!(dark::fg::A1.hex, "#969696");
        assert_eq!(dark::fg::A2.hex, "#8F8F8F");
        assert_eq!(dark::fg::A3.hex, "#6B6B6B");
        assert_eq!(dark::fg::A4.hex, "#3F3F3F");

        assert_eq!(dark::divider::BRIGHT.hex, "#363636");
        assert_eq!(dark::divider::DIM.hex, "#212121");
    }

    #[test]
    fn test_light_mode_tokens() {
        assert_eq!(light::accent::MAIN.hex, "#147D69");
        assert_eq!(light::accent::BRIGHT.hex, "#178B75");
        assert_eq!(light::accent::DIM.hex, "#106452");

        assert_eq!(light::bg::A1.hex, "#EDEDED");
        assert_eq!(light::bg::A2.hex, "#DDDDDD");
        assert_eq!(light::bg::A3.hex, "#D5D5D5");
        assert_eq!(light::bg::A4.hex, "#B3B3B3");
        assert_eq!(light::bg::B1.hex, "#E7E7E7");
        assert_eq!(light::bg::B2.hex, "#DEDEDE");

        assert_eq!(light::fg::A1.hex, "#3B3B3B");
        assert_eq!(light::fg::A2.hex, "#2D2D2D");
        assert_eq!(light::fg::A3.hex, "#6B6B6B");
    }

    #[test]
    fn test_color_conversions() {
        let mint = dark::accent::MAIN;
        let rgb = mint.to_rgb();
        assert_eq!(rgb, (120, 227, 203));
        let (r, g, b, a) = mint.to_f32_rgba();
        assert!((r - 120.0 / 255.0).abs() < 1e-4);
        assert!((g - 227.0 / 255.0).abs() < 1e-4);
        assert!((b - 203.0 / 255.0).abs() < 1e-4);
        assert_eq!(a, 1.0);
        assert_eq!(mint.to_css_rgba(), "rgba(120, 227, 203, 1.000)");
    }

    #[test]
    fn test_theme_palette_json() {
        let dark = ThemePalette::dark();
        let json = serde_json::to_string(&dark).unwrap();
        assert!(json.contains("#78E3CB"));
        let parsed: ThemePalette = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, ThemeMode::Dark);
        assert_eq!(parsed.accent_main, "#78E3CB");
    }
}
