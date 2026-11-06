//! The 3-Pillar Settings Model for Ondrane
//!
//! Faithfully mirrors the 3-tier preferences hierarchy from Figma frames 21-39:
//! 1. App    (Global, Interface, Privacy, Controls)
//! 2. Media  (Playback, Video, Audio, Subtitles, Library)
//! 3. System (Network, Performance, Recording, Extensions)
//!
//! Fully serializable to/from JSON via Serde.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::tokens::{dark, ThemeMode};

// ============================================================================
// 1. APP PILLAR (Frames 21-25, 34)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppGlobalSettings {
    /// Interface language code (e.g. "en", "fr", "system")
    pub language: String,
    /// Theme mode (Dark, Light, System)
    pub theme_mode: ThemeMode,
    /// Built-in colour scheme; VLC preserves the original mode and accent.
    pub color_scheme: crate::themes::ColorScheme,
    pub catppuccin_flavor: crate::themes::CatppuccinFlavor,
    /// Accent color hex (#78E3CB default Ondrane mint)
    pub accent_color: String,
    /// Launch app minimized to tray
    pub start_minimized: bool,
    /// Display error popups when playback fails
    pub show_error_dialogs: bool,
    /// Automatically check for application updates
    pub check_updates: bool,
    /// Developer mode with debug overlays & verbose logs
    pub developer_mode: bool,
    /// Has completed initial onboarding and profile selection
    pub onboarding_completed: bool,
    /// Write a diagnostic log file
    pub save_log: bool,
    /// Log verbosity (0 errors only, 1 warnings, 2 debug)
    pub log_detail: u8,
}

impl Default for AppGlobalSettings {
    fn default() -> Self {
        Self {
            language: "system".to_string(),
            theme_mode: ThemeMode::Dark,
            color_scheme: Default::default(),
            catppuccin_flavor: Default::default(),
            accent_color: dark::accent::MAIN.hex.to_string(),
            start_minimized: false,
            show_error_dialogs: true,
            check_updates: true,
            developer_mode: false,
            onboarding_completed: true, // Default to true so existing runs aren't blocked, but toggleable
            save_log: false,
            log_detail: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppInterfaceSettings {
    /// Path to optional custom skin file (.vlt)
    pub custom_skin_path: Option<String>,
    /// Embed video directly inside the UI window (vs separate video window)
    pub embedded_video: bool,
    /// Automatically resize window to match native video dimensions on open
    pub auto_resize_window: bool,
    /// Keep window always on top
    pub always_on_top: bool,
    /// Show seek bar hover video thumbnail previews
    pub show_seek_previews: bool,
    /// Pin control bars permanently visible outside fullscreen
    pub pin_controls: bool,
    /// Inactivity delay before floating controls hide (in milliseconds)
    pub controls_hide_delay_ms: u32,
    /// Display signature VLC cone icon in volume HUD
    pub osd_volume_cone: bool,
    /// Show the media file name in the title bar
    pub show_filename_in_title: bool,
    /// Auto-hide controls: 0 never, 1 in fullscreen only, 2 always
    pub auto_hide_controls: u8,
    /// Start with the browsing side panel hidden
    pub start_compact: bool,
    /// Remember the browsing side panel state
    pub side_panel_open: bool,
    /// Library grid (true) or list (false) view
    pub library_grid: bool,
}

impl Default for AppInterfaceSettings {
    fn default() -> Self {
        Self {
            custom_skin_path: None,
            embedded_video: true,
            auto_resize_window: true,
            always_on_top: false,
            show_seek_previews: true,
            pin_controls: false,
            controls_hide_delay_ms: 1600,
            osd_volume_cone: true,
            show_filename_in_title: true,
            auto_hide_controls: 1,
            start_compact: false,
            side_panel_open: false,
            library_grid: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppPrivacySettings {
    /// Remember recent media playback history
    pub remember_history: bool,
    /// Fetch album art, movie posters and metadata from the internet
    pub fetch_remote_metadata: bool,
    /// Send anonymous usage metrics and crash telemetry
    pub send_anonymous_telemetry: bool,
    /// Clear playback history on application exit
    pub clear_history_on_exit: bool,
    /// Remember the playback position of each file
    pub remember_position: bool,
}

impl Default for AppPrivacySettings {
    fn default() -> Self {
        Self {
            remember_history: true,
            fetch_remote_metadata: true,
            send_anonymous_telemetry: false,
            clear_history_on_exit: false,
            remember_position: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseWheelAction {
    AdjustVolume,
    SeekPosition,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DoubleClickAction {
    ToggleFullscreen,
    TogglePlayPause,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MiddleClickAction {
    ToggleMute,
    PictureInPicture,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppControlsSettings {
    pub mouse_wheel_action: MouseWheelAction,
    pub double_click_action: DoubleClickAction,
    pub middle_click_action: MiddleClickAction,
    /// Show on-screen keyboard shortcut toast on trigger
    pub osd_shortcuts: bool,
    /// Enable touch/trackpad swipe gestures
    pub gesture_controls: bool,
    pub single_click_play_pause: bool,
    pub reverse_scroll: bool,
    pub skip_very_short_s: u32,
    pub skip_short_s: u32,
    pub skip_medium_s: u32,
    pub skip_long_s: u32,
    /// User overrides of the keyboard shortcut table (action id -> key)
    pub shortcuts: std::collections::BTreeMap<String, String>,
}

impl Default for AppControlsSettings {
    fn default() -> Self {
        Self {
            mouse_wheel_action: MouseWheelAction::AdjustVolume,
            double_click_action: DoubleClickAction::ToggleFullscreen,
            middle_click_action: MiddleClickAction::ToggleMute,
            osd_shortcuts: true,
            gesture_controls: true,
            single_click_play_pause: true,
            reverse_scroll: false,
            skip_very_short_s: 3,
            skip_short_s: 10,
            skip_medium_s: 60,
            skip_long_s: 300,
            shortcuts: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub global: AppGlobalSettings,
    pub interface: AppInterfaceSettings,
    pub privacy: AppPrivacySettings,
    pub controls: AppControlsSettings,
}

// ============================================================================
// 2. MEDIA PILLAR (Frames 26-33, 35)
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResumeMode {
    Always,
    Ask,
    Never,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoopMode {
    None,
    Single,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaPlaybackSettings {
    /// Playback resumption behavior
    pub resume_mode: ResumeMode,
    /// Binge-watching: automatically play the next video in folder when finished
    pub auto_play_next: bool,
    /// Default playback speed multiplier (1.0 = normal)
    pub default_speed: f32,
    /// Default repeat mode
    pub loop_mode: LoopMode,
    /// Remember per-media volume level
    pub remember_volume_per_file: bool,
    /// Start playing as soon as a file opens
    pub start_on_open: bool,
    pub shuffle: bool,
    pub stop_after_each: bool,
    /// Show thumbnail while scrubbing
    pub seek_thumbnails: bool,
}

impl Default for MediaPlaybackSettings {
    fn default() -> Self {
        Self {
            resume_mode: ResumeMode::Ask,
            auto_play_next: true,
            default_speed: 1.0,
            loop_mode: LoopMode::None,
            remember_volume_per_file: false,
            start_on_open: true,
            shuffle: false,
            stop_after_each: false,
            seek_thumbnails: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeinterlaceMode {
    Off,
    Automatic,
    On,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AspectRatio {
    Default,
    SixteenNine,
    FourThree,
    TwentyOneNine,
    OneToOne,
    Fill,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PictureAdjustSettings {
    pub enabled: bool,
    pub brightness: f32, // 0.0 .. 2.0 (1.0 default)
    pub contrast: f32,   // 0.0 .. 2.0 (1.0 default)
    pub saturation: f32, // 0.0 .. 3.0 (1.0 default)
    pub hue: f32,        // 0.0 .. 360.0 (0.0 default)
    pub gamma: f32,      // 0.01 .. 10.0 (1.0 default)
}

impl Default for PictureAdjustSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            hue: 0.0,
            gamma: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaVideoSettings {
    pub fullscreen_on_launch: bool,
    pub disable_screensaver: bool,
    pub video_wallpaper_mode: bool,
    pub hardware_decoding: bool,
    pub deinterlace: DeinterlaceMode,
    pub aspect_ratio: AspectRatio,
    pub crop_geometry: Option<String>,
    pub picture_adjust: PictureAdjustSettings,
    pub enable_video: bool,
}

impl Default for MediaVideoSettings {
    fn default() -> Self {
        Self {
            fullscreen_on_launch: false,
            disable_screensaver: true,
            video_wallpaper_mode: false,
            hardware_decoding: true,
            deinterlace: DeinterlaceMode::Automatic,
            aspect_ratio: AspectRatio::Default,
            crop_geometry: None,
            picture_adjust: PictureAdjustSettings::default(),
            enable_video: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpeakerSetup {
    Original,
    Stereo,
    FivePointOne,
    SevenPointOne,
    Headphones,
}

/// Advanced audio compression settings (Figma Frames 64-67)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioCompressorSettings {
    pub enabled: bool,
    pub attack_ms: f32,      // e.g. 25.0 ms
    pub release_ms: f32,     // e.g. 100.0 ms
    pub threshold_db: f32,   // e.g. -11.0 dB
    pub ratio: f32,          // e.g. 4.0 (:1)
    pub knee_db: f32,        // e.g. 5.0 dB
    pub makeup_gain_db: f32, // e.g. 0.0 dB
}

impl Default for AudioCompressorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            attack_ms: 25.0,
            release_ms: 100.0,
            threshold_db: -11.0,
            ratio: 4.0,
            knee_db: 5.0,
            makeup_gain_db: 0.0,
        }
    }
}

/// 3D Spatializer settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSpatializerSettings {
    pub enabled: bool,
    pub room_size: f32,
    pub width: f32,
    pub wet: f32,
    pub dry: f32,
    pub damping: f32,
}

impl Default for AudioSpatializerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            room_size: 1.0,
            width: 1.0,
            wet: 0.3,
            dry: 0.7,
            damping: 0.5,
        }
    }
}

/// Stereo widener settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioWidenerSettings {
    pub enabled: bool,
    pub width: f32, // 0.0 .. 2.0
}

impl Default for AudioWidenerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 1.2,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioDspSettings {
    pub compressor: AudioCompressorSettings,
    pub spatializer: AudioSpatializerSettings,
    pub stereo_widener: AudioWidenerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaAudioSettings {
    /// Audio output device name or id ("" = auto)
    pub output_device: String,
    pub speaker_setup: SpeakerSetup,
    /// Maximum allowed volume (100%, 125%, 200%)
    pub max_volume: u32,
    /// Default playback volume (80%)
    pub default_volume: u32,
    /// Audio synchronization offset in milliseconds
    pub audio_delay_ms: i32,
    /// 10-band equalizer preset name (e.g. "Flat", "Rock", "Club")
    pub equalizer_preset: Option<String>,
    /// Professional DSP audio effects
    pub audio_dsp: AudioDspSettings,
    pub enable_audio: bool,
    pub volume_step: u32,
    pub remember_last_volume: bool,
    pub last_volume: u32,
    pub equalizer_enabled: bool,
    /// Preamp followed by the 10 band gains, in dB (-20..20)
    pub equalizer_db: Vec<f32>,
}

impl Default for MediaAudioSettings {
    fn default() -> Self {
        Self {
            output_device: String::new(),
            speaker_setup: SpeakerSetup::Stereo,
            max_volume: 125,
            default_volume: 80,
            audio_delay_ms: 0,
            equalizer_preset: None,
            audio_dsp: AudioDspSettings::default(),
            enable_audio: true,
            volume_step: 5,
            remember_last_volume: true,
            last_volume: 80,
            equalizer_enabled: false,
            equalizer_db: vec![12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubtitleMatchMode {
    EpisodeOnly,
    Flexible,
    Exact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubtitleSizeMode {
    Relative,
    Precise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SubtitleStyleSettings {
    pub size_mode: SubtitleSizeMode,
    pub scale_percent: u32,     // 100%
    pub precise_px: u32,        // 32px
    pub font_family: String,    // empty = system
    pub color_hex: String,      // #FFFFFF
    pub outline_px: u32,        // 0, 2, 4, 6
    pub background_opacity: u8, // 0..255
    pub bold: bool,
    pub margin_bottom_px: u32,
}

impl Default for SubtitleStyleSettings {
    fn default() -> Self {
        Self {
            size_mode: SubtitleSizeMode::Relative,
            scale_percent: 100,
            precise_px: 32,
            font_family: String::new(),
            color_hex: "#FFFFFF".to_string(),
            outline_px: 4,
            background_opacity: 0,
            bold: false,
            margin_bottom_px: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaSubtitlesSettings {
    pub preferred_language: String, // e.g. "eng", "fra"
    pub fallback_language: String,
    pub encoding: String, // e.g. "UTF-8"
    pub matching_mode: SubtitleMatchMode,
    pub style: SubtitleStyleSettings,
    pub enabled: bool,
    pub auto_load: bool,
    pub delay_ms: i32,
}

impl Default for MediaSubtitlesSettings {
    fn default() -> Self {
        Self {
            preferred_language: String::new(),
            fallback_language: "eng".to_string(),
            encoding: "UTF-8".to_string(),
            matching_mode: SubtitleMatchMode::EpisodeOnly,
            style: SubtitleStyleSettings::default(),
            enabled: true,
            auto_load: true,
            delay_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LibrarySort {
    NameAsc,
    NameDesc,
    DateAdded,
    Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaLibrarySettings {
    pub watched_folders: Vec<PathBuf>,
    pub default_sort: LibrarySort,
    pub scan_subfolders: bool,
    pub show_hidden: bool,
}

impl Default for MediaLibrarySettings {
    fn default() -> Self {
        Self {
            watched_folders: Vec::new(),
            default_sort: LibrarySort::NameAsc,
            scan_subfolders: true,
            show_hidden: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MediaSettings {
    pub playback: MediaPlaybackSettings,
    pub video: MediaVideoSettings,
    pub audio: MediaAudioSettings,
    pub subtitles: MediaSubtitlesSettings,
    pub library: MediaLibrarySettings,
}

// ============================================================================
// 3. SYSTEM PILLAR (Frames 36-39)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemNetworkSettings {
    /// Network cache size in milliseconds (e.g. 1000ms)
    pub network_caching_ms: u32,
    /// Optional HTTP/SOCKS proxy
    pub proxy_url: Option<String>,
    /// Stream read buffer in KB
    pub buffer_size_kb: u32,
    /// User Agent string for web streams
    pub user_agent: String,
}

impl Default for SystemNetworkSettings {
    fn default() -> Self {
        Self {
            network_caching_ms: 1000,
            proxy_url: None,
            buffer_size_kb: 2048,
            user_agent: "Ondrane/2.0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GpuBackend {
    Auto,
    Vaapi,
    Vdpau,
    Cuda,
    Direct3D11,
    Metal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkipLoopFilter {
    None,
    NonRef,
    Bidir,
    NonKey,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemPerformanceSettings {
    pub gpu_backend: GpuBackend,
    /// Number of decoding threads (0 = auto)
    pub decoding_threads: u32,
    /// Drop delayed frames during heavy CPU loads
    pub drop_late_frames: bool,
    /// Skip H.264/HEVC in-loop deblocking filter
    pub skip_loop_filter: SkipLoopFilter,
}

impl Default for SystemPerformanceSettings {
    fn default() -> Self {
        Self {
            gpu_backend: GpuBackend::Auto,
            decoding_threads: 0,
            drop_late_frames: true,
            skip_loop_filter: SkipLoopFilter::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnapshotFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemRecordingSettings {
    pub snapshot_dir: PathBuf,
    pub snapshot_format: SnapshotFormat,
    pub recording_dir: PathBuf,
    pub recording_container: String,
}

impl Default for SystemRecordingSettings {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let pictures = home.join("Pictures");
        let videos = home.join("Videos");
        Self {
            snapshot_dir: if pictures.exists() {
                pictures
            } else {
                home.clone()
            },
            snapshot_format: SnapshotFormat::Png,
            recording_dir: if videos.exists() { videos } else { home },
            recording_container: "mp4".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemExtensionsSettings {
    pub lua_enabled: bool,
    pub enabled_extensions: Vec<String>,
}

impl Default for SystemExtensionsSettings {
    fn default() -> Self {
        Self {
            lua_enabled: true,
            enabled_extensions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemSettings {
    pub network: SystemNetworkSettings,
    pub performance: SystemPerformanceSettings,
    pub recording: SystemRecordingSettings,
    pub extensions: SystemExtensionsSettings,
}

// ============================================================================
// ROOT SETTINGS REPOSITORY
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OndraneSettings {
    pub app: AppSettings,
    pub media: MediaSettings,
    pub system: SystemSettings,
    /// Values of the remaining Figma settings rows, keyed like the rows
    /// themselves; defaults and libVLC mappings live in `crate::options`.
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

impl OndraneSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize current configuration into pretty JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize configuration from a JSON string.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Get default configuration directory according to OS standards.
    pub fn config_path() -> PathBuf {
        let base = directories::ProjectDirs::from("io.github", "LibLOCALE", "Ondrane")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
                    .unwrap_or_default();
                home.join(".config").join("ondrane")
            });
        base.join("settings.json")
    }

    /// Load settings from the default configuration file, or fallback to defaults.
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if let Some(legacy) = directories::ProjectDirs::from("org", "videolan", "vlcreborn") {
            if let Some(destination) = path.parent() {
                if let Err(e) = migrate_legacy_files(legacy.config_dir(), destination) {
                    eprintln!("Could not copy legacy preferences: {e}");
                }
            }
        }
        Self::load_from_file(&path).unwrap_or_default()
    }

    /// Load settings from a given file path.
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let settings = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(settings)
    }

    /// Save settings to the default configuration file.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_file(&Self::config_path())
    }

    /// Save settings to a given file path atomically (ensuring parent directories exist).
    /// Uses a temporary file in the destination directory followed by an atomic rename,
    /// preventing corruption in case of unexpected termination or power loss.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let json = self.to_json().map_err(std::io::Error::other)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(json.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

// Copy only user data, never scripts or caches; preserve both originals and new files.
fn migrate_legacy_files(source: &Path, destination: &Path) -> std::io::Result<()> {
    for name in ["settings.json", "library.json", "toolbar.json"] {
        let old = source.join(name);
        let new = destination.join(name);
        if !old.is_file() || new.exists() {
            continue;
        }
        fs::create_dir_all(destination)?;
        let mut temporary = tempfile::NamedTempFile::new_in(destination)?;
        std::io::copy(&mut File::open(old)?, &mut temporary)?;
        temporary.as_file().sync_all()?;
        if let Err(e) = temporary.persist_noclobber(new) {
            if e.error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e.error);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_existing_settings_and_legacy_data() {
        let temporary = tempfile::tempdir().unwrap();
        let old = temporary.path().join("old");
        let new = temporary.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("settings.json"), "legacy preferences").unwrap();
        fs::write(old.join("library.json"), "history").unwrap();
        fs::write(old.join("extension.lua"), "do not copy").unwrap();
        fs::write(new.join("settings.json"), "new preferences").unwrap();
        migrate_legacy_files(&old, &new).unwrap();
        migrate_legacy_files(&old, &new).unwrap();
        assert_eq!(
            fs::read_to_string(new.join("settings.json")).unwrap(),
            "new preferences"
        );
        assert_eq!(
            fs::read_to_string(new.join("library.json")).unwrap(),
            "history"
        );
        assert!(old.join("library.json").is_file());
        assert!(!new.join("extension.lua").exists());
    }

    #[test]
    fn test_default_settings_pillars() {
        let s = OndraneSettings::default();
        // App pillar
        assert_eq!(s.app.global.language, "system");
        assert_eq!(s.app.global.theme_mode, ThemeMode::Dark);
        assert_eq!(s.app.global.accent_color, "#78E3CB");
        assert!(s.app.interface.embedded_video);
        assert_eq!(s.app.interface.controls_hide_delay_ms, 1600);
        assert!(s.app.privacy.remember_history);

        // Media pillar
        assert_eq!(s.media.playback.resume_mode, ResumeMode::Ask);
        assert!(s.media.playback.auto_play_next);
        assert_eq!(s.media.playback.default_speed, 1.0);
        assert!(s.media.video.hardware_decoding);
        assert_eq!(s.media.audio.default_volume, 80);
        assert_eq!(s.media.audio.max_volume, 125);
        assert_eq!(s.media.subtitles.encoding, "UTF-8");

        // System pillar
        assert_eq!(s.system.network.network_caching_ms, 1000);
        assert_eq!(s.system.performance.gpu_backend, GpuBackend::Auto);
        assert!(s.system.extensions.lua_enabled);
    }

    #[test]
    fn test_settings_json_roundtrip() {
        let mut s = OndraneSettings::default();
        s.app.global.accent_color = "#9BEBDA".to_string();
        s.media.audio.default_volume = 95;
        s.media.audio.audio_dsp.compressor.enabled = true;
        s.media.audio.audio_dsp.compressor.ratio = 6.0;

        let json = s.to_json().expect("Failed to serialize settings");
        assert!(json.contains("#9BEBDA"));
        assert!(json.contains("audio_dsp"));

        let loaded = OndraneSettings::from_json(&json).expect("Failed to deserialize settings");
        assert_eq!(loaded, s);
    }

    #[test]
    fn test_settings_atomic_save_and_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config/settings.json");

        let mut s = OndraneSettings::default();
        s.app.global.language = "fr".to_string();
        s.save_to_file(&path)
            .expect("Failed to atomically save settings");

        assert!(path.exists());
        let loaded = OndraneSettings::load_from_file(&path).expect("Failed to load saved settings");
        assert_eq!(loaded.app.global.language, "fr");

        // Overwrite atomically
        s.app.global.language = "de".to_string();
        s.save_to_file(&path)
            .expect("Failed to atomically overwrite settings");
        let reloaded = OndraneSettings::load_from_file(&path).expect("Failed to reload settings");
        assert_eq!(reloaded.app.global.language, "de");
    }
}
