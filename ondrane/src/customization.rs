//! Control-bar customization ("Player - customize" frames) and the
//! "Profile" submenu of the main menu (Movies / Shows / Music / Work).
//!
//! The fixed part of the Figma control bar (previous, play, next, times,
//! seek bar, volume, audio, subtitles, playback settings, queue) is always
//! present; users add the optional tools listed in the Customize dialog.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    PlaybackSpeed,
    Forward10,
    Backward10,
    Forward15,
    Backward15,
    Stop,
    Repeat,
    Shuffle,
    Snapshot,
    PictureInPicture,
    Zoom,
    RightSidebar,
    QuitVlc,
    VlcSettings,
    GoHome,
    Properties,
    Replay,
    Favorite,
    Fullscreen,
    Playlists,
    Record,
    Stream,
    Cast,
}

impl Tool {
    pub const ALL: [Tool; 23] = [
        Tool::PlaybackSpeed,
        Tool::Forward10,
        Tool::Backward10,
        Tool::Forward15,
        Tool::Backward15,
        Tool::Stop,
        Tool::Repeat,
        Tool::Shuffle,
        Tool::Snapshot,
        Tool::PictureInPicture,
        Tool::Zoom,
        Tool::RightSidebar,
        Tool::QuitVlc,
        Tool::VlcSettings,
        Tool::GoHome,
        Tool::Properties,
        Tool::Replay,
        Tool::Favorite,
        Tool::Fullscreen,
        Tool::Playlists,
        Tool::Record,
        Tool::Stream,
        Tool::Cast,
    ];

    /// Stable identifier used by the UI ("tool:<id>").
    pub fn id(self) -> &'static str {
        match self {
            Tool::PlaybackSpeed => "speed",
            Tool::Forward10 => "fwd10",
            Tool::Backward10 => "back10",
            Tool::Forward15 => "fwd15",
            Tool::Backward15 => "back15",
            Tool::Stop => "stop",
            Tool::Repeat => "repeat",
            Tool::Shuffle => "shuffle",
            Tool::Snapshot => "snapshot",
            Tool::PictureInPicture => "pip",
            Tool::Zoom => "zoom",
            Tool::RightSidebar => "sidebar",
            Tool::QuitVlc => "quit",
            Tool::VlcSettings => "settings",
            Tool::GoHome => "home",
            Tool::Properties => "properties",
            Tool::Replay => "replay",
            Tool::Favorite => "favorite",
            Tool::Fullscreen => "fullscreen",
            Tool::Playlists => "playlists",
            Tool::Record => "record",
            Tool::Stream => "stream",
            Tool::Cast => "cast",
        }
    }

    pub fn from_id(id: &str) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|t| t.id() == id)
    }

    /// Label shown in the Customize Player dialog.
    pub fn label(self) -> &'static str {
        match self {
            Tool::PlaybackSpeed => "Playback Speed",
            Tool::Forward10 => "Forward 10s",
            Tool::Backward10 => "Backward 10s",
            Tool::Forward15 => "Forward 15s",
            Tool::Backward15 => "Backward 15s",
            Tool::Stop => "Stop",
            Tool::Repeat => "Repeat",
            Tool::Shuffle => "Shuffle",
            Tool::Snapshot => "Snapshot",
            Tool::PictureInPicture => "Picture in Picture",
            Tool::Zoom => "Zoom",
            Tool::RightSidebar => "Right Sidebar",
            Tool::QuitVlc => "Quit Ondrane",
            Tool::VlcSettings => "VLC Settings",
            Tool::GoHome => "Go Home",
            Tool::Properties => "Properties",
            Tool::Replay => "Replay",
            Tool::Favorite => "Favorite",
            Tool::Fullscreen => "Fullscreen",
            Tool::Playlists => "Playlists",
            Tool::Record => "Record",
            Tool::Stream => "Stream",
            Tool::Cast => "Cast",
        }
    }

    /// Name of the Figma icon variant (file stem in ui/assets/icons).
    pub fn icon_name(self) -> &'static str {
        match self {
            Tool::PlaybackSpeed => "speed",
            Tool::Forward10 => "forward-10-sec",
            Tool::Backward10 => "backward-10-sec",
            Tool::Forward15 => "forward-15-sec",
            Tool::Backward15 => "backward-15-sec",
            Tool::Stop => "stop",
            Tool::Repeat => "repeat-right",
            Tool::Shuffle => "shuffle-right",
            Tool::Snapshot => "snapshot",
            Tool::PictureInPicture => "PiP",
            Tool::Zoom => "zoom",
            Tool::RightSidebar => "sidebar-right",
            Tool::QuitVlc => "quit",
            Tool::VlcSettings => "settings",
            Tool::GoHome => "home",
            Tool::Properties => "info",
            Tool::Replay => "replay",
            Tool::Favorite => "favorite",
            Tool::Fullscreen => "fullscreen",
            Tool::Playlists => "playlist",
            Tool::Record => "record",
            Tool::Stream => "streaming",
            Tool::Cast => "tv",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    Movies,
    Shows,
    Music,
    Work,
    Custom,
}

impl Profile {
    pub const PRESETS: [Profile; 4] = [
        Profile::Movies,
        Profile::Shows,
        Profile::Music,
        Profile::Work,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Profile::Movies => "Movies",
            Profile::Shows => "Shows",
            Profile::Music => "Music",
            Profile::Work => "Work",
            Profile::Custom => "Custom",
        }
    }

    pub fn tools(self) -> Vec<Tool> {
        match self {
            Profile::Movies => vec![],
            Profile::Shows => vec![Tool::Backward10, Tool::Forward10, Tool::PlaybackSpeed],
            Profile::Music => vec![Tool::Shuffle, Tool::Repeat, Tool::Favorite],
            Profile::Work => vec![
                Tool::Backward15,
                Tool::Forward15,
                Tool::PlaybackSpeed,
                Tool::Snapshot,
            ],
            Profile::Custom => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolbarConfig {
    pub profile: Profile,
    pub tools: Vec<Tool>,
}

impl Default for ToolbarConfig {
    fn default() -> Self {
        Self::from_profile(Profile::Movies)
    }
}

impl ToolbarConfig {
    pub fn from_profile(profile: Profile) -> Self {
        Self {
            profile,
            tools: profile.tools(),
        }
    }

    pub fn contains(&self, tool: Tool) -> bool {
        self.tools.contains(&tool)
    }

    /// Add the tool if absent, remove it otherwise. Returns whether it is now active.
    pub fn toggle(&mut self, tool: Tool) -> bool {
        self.profile = Profile::Custom;
        if let Some(i) = self.tools.iter().position(|t| *t == tool) {
            self.tools.remove(i);
            false
        } else {
            self.tools.push(tool);
            true
        }
    }

    pub fn move_tool(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tools.len() || to >= self.tools.len() {
            return false;
        }
        if from == to {
            return true;
        }
        let t = self.tools.remove(from);
        self.tools.insert(to, t);
        self.profile = Profile::Custom;
        true
    }

    pub fn config_path() -> PathBuf {
        crate::settings::OndraneSettings::config_path()
            .parent()
            .map(|p| p.join("toolbar.json"))
            .unwrap_or_else(|| PathBuf::from("toolbar.json"))
    }

    pub fn load_or_default() -> Self {
        Self::load_from_file(&Self::config_path()).unwrap_or_default()
    }

    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let text = fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save the toolbar configuration to the default configuration path.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_file(&Self::config_path())
    }

    /// Save the toolbar configuration atomically using a temporary file in the
    /// destination directory followed by an atomic rename.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        std::io::Write::write_all(&mut temp, json.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip_and_icons_exist() {
        let icons = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/assets/icons");
        for t in Tool::ALL {
            assert_eq!(Tool::from_id(t.id()), Some(t));
            let base = icons.join(format!("{}.svg", t.icon_name()));
            let accent = icons.join(format!("{}.accent.svg", t.icon_name()));
            assert!(base.exists() || accent.exists(), "missing icon for {:?}", t);
        }
    }

    #[test]
    fn toggle_and_persist() {
        let mut cfg = ToolbarConfig::from_profile(Profile::Shows);
        assert!(cfg.contains(Tool::Forward10));
        assert!(!cfg.toggle(Tool::Forward10));
        assert!(cfg.toggle(Tool::Snapshot));
        assert_eq!(cfg.profile, Profile::Custom);
        assert!(cfg.move_tool(0, 1));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("toolbar.json");
        cfg.save_to_file(&path).unwrap();
        assert_eq!(ToolbarConfig::load_from_file(&path).unwrap(), cfg);
    }
}
