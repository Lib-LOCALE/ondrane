//! "Add-ons & Scripts": the Lua scripts libVLC loads (playlist parsers such as
//! YouTube, artwork and metadata fetchers, service discovery). Built-in
//! scripts come with VLC; user add-ons live in VLC's user `lua` folder and
//! can be installed, disabled and removed here.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Addon {
    pub name: String,
    /// "playlist", "meta/art", "sd", "extensions"…
    pub category: String,
    pub path: PathBuf,
    pub user: bool,
    pub enabled: bool,
}

pub const CATEGORIES: [&str; 7] = [
    "playlist",
    "meta/art",
    "meta/fetcher",
    "meta/reader",
    "sd",
    "extensions",
    "intf",
];

pub fn category_label(c: &str) -> &'static str {
    match c {
        "playlist" => "Playlist & website parsers",
        "meta/art" => "Artwork fetchers",
        "meta/fetcher" => "Metadata fetchers",
        "meta/reader" => "Metadata readers",
        "sd" => "Service discovery",
        "extensions" => "Extensions",
        _ => "Interfaces",
    }
}

/// VLC's per-user Lua folder.
pub fn user_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("vlc")
            .join("lua")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Library/Application Support/org.videolan.vlc/lua")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_default()
            .join("vlc")
            .join("lua")
    }
}

/// Lua folder of the VLC installation that provides libVLC.
pub fn system_dir(libvlc: Option<&Path>) -> Option<PathBuf> {
    let dir = libvlc?.parent()?;
    [
        dir.join("vlc/lua"),
        dir.join("lua"),
        dir.join("../share/lua"),
        PathBuf::from("/usr/lib/vlc/lua"),
        PathBuf::from("/usr/share/vlc/lua"),
    ]
    .into_iter()
    .find(|p| p.join("playlist").is_dir())
}

fn title_of(path: &Path) -> String {
    let stem = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = stem
        .trim_end_matches(".off")
        .trim_end_matches(".disabled")
        .trim_end_matches(".luac")
        .trim_end_matches(".lua")
        .to_string();
    // extensions declare a title in descriptor(); compiled scripts are binary
    if let Ok(text) = std::fs::read_to_string(path) {
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| regex::Regex::new(r#"title\s*=\s*["']([^"']+)["']"#).unwrap());
        if let Some(cap) = re.captures(&text) {
            return cap[1].to_string();
        }
    }
    stem.replace('_', " ")
}

fn scan(root: &Path, user: bool, out: &mut Vec<Addon>) {
    for cat in CATEGORIES {
        let Ok(rd) = std::fs::read_dir(root.join(cat)) else {
            continue;
        };
        let mut found: Vec<Addon> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.to_string_lossy();
                n.ends_with(".lua")
                    || n.ends_with(".luac")
                    || (user && (n.ends_with(".disabled") || n.ends_with(".off")))
            })
            .map(|p| {
                let n = p.to_string_lossy().to_string();
                Addon {
                    name: title_of(&p),
                    category: cat.to_string(),
                    enabled: !(n.ends_with(".disabled") || n.ends_with(".off")),
                    path: p,
                    user,
                }
            })
            .collect();
        found.sort_by_key(|a| a.name.to_lowercase());
        out.extend(found);
    }
}

/// User add-ons first, then the scripts of the VLC installation.
pub fn list(libvlc: Option<&Path>) -> Vec<Addon> {
    let mut out = Vec::new();
    scan(&user_dir(), true, &mut out);
    if let Some(sys) = system_dir(libvlc) {
        scan(&sys, false, &mut out);
    }
    out
}

/// Enable or disable one user add-on (renames `x.lua` ⇄ `x.lua.disabled`).
pub fn set_enabled(addon: &Addon, enabled: bool) -> std::io::Result<PathBuf> {
    if !addon.user || addon.enabled == enabled {
        return Ok(addon.path.clone());
    }
    let s = addon.path.to_string_lossy().to_string();
    let target = if enabled {
        PathBuf::from(
            s.trim_end_matches(".disabled")
                .trim_end_matches(".off")
                .to_string(),
        )
    } else {
        PathBuf::from(format!("{s}.disabled"))
    };
    std::fs::rename(&addon.path, &target)?;
    Ok(target)
}

/// "Load add-ons on startup": park (`.off`) or restore every enabled user
/// add-on before libVLC starts.
pub fn apply_master_switch(load: bool) {
    let root = user_dir();
    for cat in CATEGORIES {
        let Ok(rd) = std::fs::read_dir(root.join(cat)) else {
            continue;
        };
        for p in rd.flatten().map(|e| e.path()) {
            let s = p.to_string_lossy().to_string();
            if load && s.ends_with(".off") {
                let _ = std::fs::rename(&p, s.trim_end_matches(".off"));
            } else if !load && (s.ends_with(".lua") || s.ends_with(".luac")) {
                let _ = std::fs::rename(&p, format!("{s}.off"));
            }
        }
    }
}

/// Guess the category of a Lua script from the functions it defines.
pub fn detect_category(text: &str) -> &'static str {
    if text.contains("function descriptor") && text.contains("function activate") {
        "extensions"
    } else if text.contains("function fetch_art") {
        "meta/art"
    } else if text.contains("function fetch_meta") {
        "meta/fetcher"
    } else if text.contains("function read_meta") {
        "meta/reader"
    } else if text.contains("function descriptor") && text.contains("function main") {
        "sd"
    } else {
        "playlist"
    }
}

/// Copy a script into the user add-on folder; returns the installed path.
/// Rejects non-Lua files and handles invalid file names securely.
pub fn install(file: &Path) -> std::io::Result<PathBuf> {
    let file_name = file.file_name().and_then(|f| f.to_str()).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file name")
    })?;

    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".lua") && !lower.ends_with(".luac") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Only .lua and .luac scripts can be installed",
        ));
    }

    let bytes = std::fs::read(file)?;
    let cat = detect_category(&String::from_utf8_lossy(&bytes));
    let dir = user_dir().join(cat);
    std::fs::create_dir_all(&dir)?;
    let safe_name = Path::new(file_name).file_name().unwrap_or_default();
    let dest = dir.join(safe_name);
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

pub fn remove(addon: &Addon) -> std::io::Result<()> {
    if addon.user {
        std::fs::remove_file(&addon.path)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_and_titles() {
        assert_eq!(
            detect_category("function probe() end\nfunction parse() end"),
            "playlist"
        );
        assert_eq!(detect_category("function fetch_art() end"), "meta/art");
        assert_eq!(
            detect_category(
                "function descriptor() return {title=\"X\"} end function activate() end"
            ),
            "extensions"
        );
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("my_ext.lua");
        std::fs::write(
            &f,
            "function descriptor() return { title = \"My Extension\" } end",
        )
        .unwrap();
        assert_eq!(title_of(&f), "My Extension");

        // Single quote title support
        let f2 = dir.path().join("single_quote.lua");
        std::fs::write(
            &f2,
            "function descriptor() return { title = 'Single Quote Ext' } end",
        )
        .unwrap();
        assert_eq!(title_of(&f2), "Single Quote Ext");

        // Rejection of non-lua files in install
        let bad = dir.path().join("malicious.sh");
        std::fs::write(&bad, "echo bad").unwrap();
        assert!(install(&bad).is_err());
    }
}
