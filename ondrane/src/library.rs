//! Media library: folder browsing, watch history, playlists.
//!
//! Backs the Figma "Home / Videos / Music / Network / Playlists / Recent"
//! sections. Everything is persisted as JSON next to the settings file.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v", "mpg", "mpeg", "ts", "m2ts", "3gp",
    "ogv", "vob", "divx", "rm", "rmvb", "asf",
];
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "oga", "opus", "m4a", "aac", "alac", "wma", "aiff", "aif", "ape",
    "mka",
];
pub const SUBTITLE_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "smi"];
pub const PLAYLIST_EXTS: &[&str] = &["m3u", "m3u8", "pls", "xspf"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Subtitle,
    Playlist,
    Other,
}

pub fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn media_kind(path: &Path) -> MediaKind {
    let ext = extension_lower(path);
    let ext = ext.as_str();
    if VIDEO_EXTS.contains(&ext) {
        MediaKind::Video
    } else if AUDIO_EXTS.contains(&ext) {
        MediaKind::Audio
    } else if SUBTITLE_EXTS.contains(&ext) {
        MediaKind::Subtitle
    } else if PLAYLIST_EXTS.contains(&ext) {
        MediaKind::Playlist
    } else {
        MediaKind::Other
    }
}

pub fn is_playable(path: &Path) -> bool {
    matches!(media_kind(path), MediaKind::Video | MediaKind::Audio)
}

fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(h) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                bytes.push(h);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn is_url(target: &str) -> bool {
    if let Ok(u) = url::Url::parse(target) {
        let s = u.scheme();
        s != "file"
            && !s.is_empty()
            && (u.has_host() || matches!(s, "rtsp" | "rtmp" | "mms" | "udp"))
    } else {
        target.contains("://") && !target.starts_with("file://")
    }
}

/// Display name of a media path or URL ("The_Wand_S01E05.mp4").
pub fn display_name(target: &str) -> String {
    if let Ok(u) = url::Url::parse(target) {
        if u.scheme() != "file" {
            if let Some(mut segments) = u.path_segments() {
                if let Some(last) = segments.rfind(|s| !s.is_empty()) {
                    return percent_decode(last);
                }
            }
            if let Some(host) = u.host_str() {
                return host.to_string();
            }
        } else if let Ok(file_path) = u.to_file_path() {
            if let Some(name) = file_path.file_name() {
                return name.to_string_lossy().to_string();
            }
        }
    }
    if is_url(target) {
        let trimmed = target.trim_end_matches('/');
        let base = trimmed.split('?').next().unwrap_or(trimmed);
        return base.rsplit('/').next().unwrap_or(base).to_string();
    }
    Path::new(target)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| target.to_string())
}

/// Window title for a media ("The_Wand_S01E04.mp4" -> "The Wand S01E04").
pub fn pretty_title(target: &str) -> String {
    let name = display_name(target);
    let stem = match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name.as_str(),
    };
    stem.replace(['_', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub kind: MediaKind,
    pub modified: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortKey {
    UnwatchedFirst,
    WatchedFirst,
    NewestFirst,
    OldestFirst,
    TitleAsc,
    TitleDesc,
}

impl SortKey {
    pub const ALL: [SortKey; 6] = [
        SortKey::UnwatchedFirst,
        SortKey::WatchedFirst,
        SortKey::NewestFirst,
        SortKey::OldestFirst,
        SortKey::TitleAsc,
        SortKey::TitleDesc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::UnwatchedFirst => "Unwatched First",
            SortKey::WatchedFirst => "Watched First",
            SortKey::NewestFirst => "Newest First",
            SortKey::OldestFirst => "Oldest First",
            SortKey::TitleAsc => "Title A-Z",
            SortKey::TitleDesc => "Title Z-A",
        }
    }
}

fn mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Natural ordering so "E2" sorts before "E10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().filter(|c| c.is_ascii_digit()) {
                    na.push(*c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().filter(|c| c.is_ascii_digit()) {
                    nb.push(*c);
                    bi.next();
                }
                let (va, vb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = va.len().cmp(&vb.len()).then_with(|| va.cmp(vb));
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (Some(ca), Some(cb)) => {
                let ord = ca.to_lowercase().cmp(cb.to_lowercase());
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// List a folder: sub-folders first, then media files matching `filter`
/// (`None` = any playable media).
pub fn list_dir(dir: &Path, filter: Option<MediaKind>, show_hidden: bool) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        let kind = if is_dir {
            MediaKind::Other
        } else {
            media_kind(&path)
        };
        if !is_dir {
            let keep = match filter {
                Some(k) => k == kind,
                None => matches!(kind, MediaKind::Video | MediaKind::Audio),
            };
            if !keep {
                continue;
            }
        }
        out.push(Entry {
            modified: mtime(&path),
            path,
            name,
            is_dir,
            kind,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    out
}

pub fn sort_entries(entries: &mut [Entry], key: SortKey, store: &LibraryStore) {
    entries.sort_by(|a, b| {
        let dirs = b.is_dir.cmp(&a.is_dir);
        if dirs != std::cmp::Ordering::Equal {
            return dirs;
        }
        let name = natural_cmp(&a.name, &b.name);
        let watched = |e: &Entry| store.is_watched(&e.path.to_string_lossy());
        match key {
            SortKey::UnwatchedFirst => watched(a).cmp(&watched(b)).then(name),
            SortKey::WatchedFirst => watched(b).cmp(&watched(a)).then(name),
            SortKey::NewestFirst => b.modified.cmp(&a.modified).then(name),
            SortKey::OldestFirst => a.modified.cmp(&b.modified).then(name),
            SortKey::TitleAsc => name,
            SortKey::TitleDesc => name.reverse(),
        }
    });
}

/// Playable siblings of a file, naturally sorted (used for "play next in folder").
pub fn folder_siblings(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    list_dir(parent, None, false)
        .into_iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path)
        .collect()
}

fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn videos_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.video_dir().map(Path::to_path_buf))
        .filter(|p| p.is_dir())
        .unwrap_or_else(home_dir)
}

pub fn music_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.audio_dir().map(Path::to_path_buf))
        .filter(|p| p.is_dir())
        .unwrap_or_else(home_dir)
}

/// Collapse the home directory to "~" for display.
pub fn tilde(path: &Path) -> String {
    let home = home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

pub fn expand_tilde(text: &str) -> PathBuf {
    if let Some(rest) = text.strip_prefix("~/") {
        home_dir().join(rest)
    } else if text == "~" {
        home_dir()
    } else {
        PathBuf::from(text)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub target: String,
    pub position_ms: i64,
    pub length_ms: i64,
    pub last_played: u64,
    pub play_count: u32,
    #[serde(default)]
    pub watched: bool,
}

impl HistoryEntry {
    pub fn progress(&self) -> f32 {
        if self.watched {
            return 1.0;
        }
        if self.length_ms <= 0 {
            return 0.0;
        }
        (self.position_ms as f32 / self.length_ms as f32).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub name: String,
    pub items: Vec<String>,
    pub created: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LibraryStore {
    pub history: Vec<HistoryEntry>,
    pub playlists: Vec<Playlist>,
    pub favorites: Vec<String>,
    pub folder_visits: HashMap<String, u32>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

pub const HISTORY_LIMIT: usize = 500;

impl LibraryStore {
    pub fn default_path() -> PathBuf {
        crate::settings::OndraneSettings::config_path()
            .parent()
            .map(|p| p.join("library.json"))
            .unwrap_or_else(|| PathBuf::from("library.json"))
    }

    pub fn load(path: PathBuf) -> Self {
        let mut store: LibraryStore = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        store.path = Some(path);
        store
    }

    pub fn load_default() -> Self {
        Self::load(Self::default_path())
    }

    pub fn save(&self) {
        let _ = self.save_result();
    }

    /// Save the library store atomically using a temporary file in the destination
    /// directory followed by an atomic rename.
    pub fn save_result(&self) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent)?;
            let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            std::io::Write::write_all(&mut temp, json.as_bytes())?;
            temp.as_file().sync_all()?;
            temp.persist(path).map_err(|e| e.error)?;
        }
        Ok(())
    }

    pub fn entry(&self, target: &str) -> Option<&HistoryEntry> {
        self.history.iter().find(|h| h.target == target)
    }

    pub fn progress(&self, target: &str) -> f32 {
        self.entry(target)
            .map(HistoryEntry::progress)
            .unwrap_or(0.0)
    }

    pub fn is_watched(&self, target: &str) -> bool {
        self.entry(target).map(|h| h.watched).unwrap_or(false)
    }

    /// Position to resume from, if the item was left part-way through.
    pub fn resume_position(&self, target: &str) -> Option<i64> {
        let h = self.entry(target)?;
        let p = h.progress();
        if !h.watched && h.position_ms > 5_000 && p < 0.95 {
            Some(h.position_ms)
        } else {
            None
        }
    }

    fn entry_mut(&mut self, target: &str) -> &mut HistoryEntry {
        if let Some(i) = self.history.iter().position(|h| h.target == target) {
            let e = self.history.remove(i);
            self.history.insert(0, e);
        } else {
            self.history.insert(
                0,
                HistoryEntry {
                    target: target.to_string(),
                    ..Default::default()
                },
            );
            self.history.truncate(HISTORY_LIMIT);
        }
        &mut self.history[0]
    }

    pub fn record_open(&mut self, target: &str) {
        let e = self.entry_mut(target);
        e.play_count += 1;
        e.last_played = now_secs();
        if !is_url(target) {
            if let Some(parent) = Path::new(target).parent() {
                *self
                    .folder_visits
                    .entry(parent.to_string_lossy().to_string())
                    .or_insert(0) += 1;
            }
        }
    }

    pub fn record_progress(&mut self, target: &str, position_ms: i64, length_ms: i64) {
        let e = self.entry_mut(target);
        e.position_ms = position_ms.max(0);
        if length_ms > 0 {
            e.length_ms = length_ms;
            if position_ms as f64 >= length_ms as f64 * 0.95 {
                e.watched = true;
            } else if position_ms > 5_000 {
                // watching it again: resume from here next time
                e.watched = false;
            }
        }
        e.last_played = now_secs();
    }

    pub fn set_watched(&mut self, target: &str, watched: bool) {
        let e = self.entry_mut(target);
        e.watched = watched;
        if !watched {
            e.position_ms = 0;
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.folder_visits.clear();
    }

    /// Videos started but not finished, most recent first ("Continue Watching").
    pub fn continue_watching(&self, limit: usize) -> Vec<&HistoryEntry> {
        self.history
            .iter()
            .filter(|h| {
                media_kind(Path::new(&h.target)) == MediaKind::Video
                    && !h.watched
                    && h.position_ms > 0
            })
            .take(limit)
            .collect()
    }

    /// Recently played audio ("Listen Again").
    pub fn listen_again(&self, limit: usize) -> Vec<&HistoryEntry> {
        self.history
            .iter()
            .filter(|h| media_kind(Path::new(&h.target)) == MediaKind::Audio)
            .take(limit)
            .collect()
    }

    /// Most visited folders that still exist ("Frequently Visited Folders").
    pub fn frequent_folders(&self, limit: usize) -> Vec<PathBuf> {
        let mut v: Vec<(&String, &u32)> = self.folder_visits.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        v.into_iter()
            .map(|(p, _)| PathBuf::from(p))
            .filter(|p| p.is_dir())
            .take(limit)
            .collect()
    }

    pub fn toggle_favorite(&mut self, target: &str) -> bool {
        if let Some(i) = self.favorites.iter().position(|f| f == target) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(target.to_string());
            true
        }
    }

    pub fn is_favorite(&self, target: &str) -> bool {
        self.favorites.iter().any(|f| f == target)
    }

    pub fn create_playlist(&mut self, name: &str, items: Vec<String>) -> usize {
        let base = if name.trim().is_empty() {
            "New Playlist"
        } else {
            name.trim()
        };
        let mut title = base.to_string();
        let mut n = 2;
        while self.playlists.iter().any(|p| p.name == title) {
            title = format!("{} {}", base, n);
            n += 1;
        }
        self.playlists.push(Playlist {
            name: title,
            items,
            created: now_secs(),
        });
        self.playlists.len() - 1
    }

    pub fn add_to_playlist(&mut self, index: usize, target: &str) {
        if let Some(p) = self.playlists.get_mut(index) {
            if !p.items.iter().any(|i| i == target) {
                p.items.push(target.to_string());
            }
        }
    }

    pub fn delete_playlist(&mut self, index: usize) {
        if index < self.playlists.len() {
            self.playlists.remove(index);
        }
    }
}

/// Serialize a list of targets as an extended M3U playlist.
pub fn to_m3u(items: &[String]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for item in items {
        out.push_str(&format!("#EXTINF:-1,{}\n{}\n", display_name(item), item));
    }
    out
}

fn is_windows_drive_path(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

fn resolve_playlist_entry(line: &str, base: Option<&Path>) -> String {
    let l = line.trim();
    if is_url(l) {
        return l.to_string();
    }
    if let Ok(u) = url::Url::parse(l) {
        if u.scheme() == "file" {
            if let Ok(p) = u.to_file_path() {
                return p.to_string_lossy().to_string();
            }
        }
    }
    if Path::new(l).is_absolute() || is_windows_drive_path(l) {
        return l.to_string();
    }
    if let Some(b) = base {
        #[cfg(not(windows))]
        let normalized = l.replace('\\', "/");
        #[cfg(windows)]
        let normalized = l;
        b.join(normalized).to_string_lossy().to_string()
    } else {
        l.to_string()
    }
}

/// Parse an M3U/M3U8 playlist, resolving relative entries against `base`.
/// Handles UTF-8 BOM, Windows backslashes, absolute Windows drive paths,
/// `file://` URIs, and auto-detects PLS playlists if passed.
pub fn parse_m3u(text: &str, base: Option<&Path>) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim_start().starts_with("[playlist]") {
        return parse_pls(text, base);
    }
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| resolve_playlist_entry(l, base))
        .collect()
}

/// Parse a PLS playlist, extracting FileN= entries and resolving them against `base`.
pub fn parse_pls(text: &str, base: Option<&Path>) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            if key.len() > 4
                && key[..4].eq_ignore_ascii_case("file")
                && key[4..].chars().all(|c| c.is_ascii_digit())
            {
                let val = val.trim();
                if !val.is_empty() {
                    entries.push(resolve_playlist_entry(val, base));
                }
            }
        }
    }
    entries
}

/// Deterministic pseudo-waveform (0..1 bar heights) for the music seek bar.
pub fn waveform_for(target: &str, bars: usize) -> Vec<f32> {
    let mut h: u64 = 1469598103934665603;
    for b in target.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    (0..bars)
        .map(|i| {
            h ^= h << 13;
            h ^= h >> 7;
            h ^= h << 17;
            let noise = (h % 1000) as f32 / 1000.0;
            let envelope = 0.55 + 0.45 * ((i as f32 * 0.13).sin() * 0.5 + 0.5);
            (0.15 + 0.85 * noise * envelope).clamp(0.1, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_names() {
        assert_eq!(media_kind(Path::new("a/B.MKV")), MediaKind::Video);
        assert_eq!(media_kind(Path::new("x.flac")), MediaKind::Audio);
        assert_eq!(media_kind(Path::new("x.srt")), MediaKind::Subtitle);
        assert_eq!(pretty_title("/v/The_Wand_S01E04.mp4"), "The Wand S01E04");
        assert_eq!(display_name("https://host/live/stream.m3u8"), "stream.m3u8");
    }

    #[test]
    fn natural_sorting() {
        let mut v = vec!["E10", "E2", "e1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["e1", "E2", "E10"]);
    }

    #[test]
    fn list_and_sort_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("Season 1")).unwrap();
        for f in [
            "ep10.mkv",
            "ep2.mp4",
            "notes.txt",
            "song.mp3",
            ".hidden.mp4",
        ] {
            fs::write(dir.path().join(f), b"x").unwrap();
        }
        let all = list_dir(dir.path(), None, false);
        let names: Vec<_> = all.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Season 1", "ep2.mp4", "ep10.mkv", "song.mp3"]);
        let videos = list_dir(dir.path(), Some(MediaKind::Video), false);
        assert_eq!(videos.len(), 3);

        let mut store = LibraryStore::default();
        store.set_watched(&dir.path().join("ep2.mp4").to_string_lossy(), true);
        let mut entries = list_dir(dir.path(), Some(MediaKind::Video), false);
        sort_entries(&mut entries, SortKey::UnwatchedFirst, &store);
        assert_eq!(entries[1].name, "ep10.mkv");
        assert_eq!(entries[2].name, "ep2.mp4");
    }

    #[test]
    fn history_progress_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib.json");
        let mut store = LibraryStore::load(path.clone());
        store.record_open("/v/a.mp4");
        store.record_progress("/v/a.mp4", 30_000, 100_000);
        assert!((store.progress("/v/a.mp4") - 0.3).abs() < 1e-6);
        assert_eq!(store.resume_position("/v/a.mp4"), Some(30_000));
        assert_eq!(store.continue_watching(5).len(), 1);
        store.record_progress("/v/a.mp4", 99_000, 100_000);
        assert!(store.is_watched("/v/a.mp4"));
        assert_eq!(store.resume_position("/v/a.mp4"), None);
        let idx = store.create_playlist("Cool Songs", vec![]);
        store.add_to_playlist(idx, "/m/song.mp3");
        store.add_to_playlist(idx, "/m/song.mp3");
        assert_eq!(store.create_playlist("Cool Songs", vec![]), 1);
        store.save();

        let loaded = LibraryStore::load(path);
        assert_eq!(loaded.playlists.len(), 2);
        assert_eq!(loaded.playlists[0].items, vec!["/m/song.mp3".to_string()]);
        assert_eq!(loaded.playlists[1].name, "Cool Songs 2");
        assert!(loaded.is_watched("/v/a.mp4"));
    }

    #[test]
    fn m3u_roundtrip() {
        let items = vec!["/a/b.mp3".to_string(), "http://x/y.m3u8".to_string()];
        let text = to_m3u(&items);
        assert_eq!(parse_m3u(&text, None), items);
        assert_eq!(
            parse_m3u("rel.mp3\n", Some(Path::new("/base"))),
            vec![Path::new("/base")
                .join("rel.mp3")
                .to_string_lossy()
                .to_string()]
        );
    }

    #[test]
    fn m3u_advanced_parsing() {
        // UTF-8 BOM handling
        let with_bom = "\u{feff}#EXTM3U\n#EXTINF:-1,Track\nsong.mp3\n";
        assert_eq!(parse_m3u(with_bom, None), vec!["song.mp3"]);

        // Windows drive path
        let win_m3u = "#EXTM3U\nC:\\Music\\song.mp3\n";
        assert_eq!(
            parse_m3u(win_m3u, Some(Path::new("/base"))),
            vec!["C:\\Music\\song.mp3"]
        );

        // file:// URI
        let file_uri = "file:///tmp/movie.mp4\n";
        let parsed = parse_m3u(file_uri, None);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].ends_with("movie.mp4"));

        // PLS format
        let pls_text = "[playlist]\nNumberOfEntries=2\nFile1=http://stream.org:8000\nTitle1=Live\nFile2=local.mp3\nTitle2=Local\n";
        let pls_res = parse_pls(pls_text, Some(Path::new("/music")));
        assert_eq!(pls_res.len(), 2);
        assert_eq!(pls_res[0], "http://stream.org:8000");
        assert_eq!(
            pls_res[1],
            Path::new("/music").join("local.mp3").to_string_lossy()
        );

        // PLS auto-detection through parse_m3u
        let auto_pls = parse_m3u(pls_text, Some(Path::new("/music")));
        assert_eq!(auto_pls, pls_res);

        // Display name with percent-encoding and query params
        assert_eq!(
            display_name("https://example.com/videos/Cool%20Show%20S01E01.mp4?auth=secret#t=10"),
            "Cool Show S01E01.mp4"
        );
    }

    #[test]
    fn waveform_is_stable() {
        let a = waveform_for("x", 50);
        assert_eq!(a, waveform_for("x", 50));
        assert!(a.iter().all(|v| (0.1..=1.0).contains(v)));
    }
}
