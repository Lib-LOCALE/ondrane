//! Ondrane - Modern Rust Media Player
//!
//! Faithful implementation of the Figma 'VLC Redesign (Community).fig'
//! design system, 3-pillar settings architecture, customizable control bar,
//! and native libVLC media engine.

pub mod addons;
pub mod customization;
pub mod engine;
pub mod gui;
pub mod i18n;
pub mod instance;
pub mod library;
pub mod mdns;
pub mod options;
pub mod pickers;
pub mod remote;
#[cfg(target_os = "linux")]
pub mod screencast;
pub mod scrobbler;
pub mod settings;
pub mod subtitles;
pub mod system;
pub mod themes;
pub mod thumbs;
pub mod tokens;

pub use customization::{Profile, Tool, ToolbarConfig};
pub use engine::{EngineError, PlaybackState, TrackInfo, VideoBufferState, VlcEngine, VlcPlayer};
pub use gui::GuiApp;
pub use settings::OndraneSettings;
pub use thumbs::{PosterWorker, ThumbnailCache, ThumbnailMaker};
pub use tokens::{dark, light, Color, ThemeMode, ThemePalette};
