//! Keyboard shortcuts (Figma "Keyboard Shortcuts" dialog). Each action has a
//! default key; user overrides live in `settings.app.controls.shortcuts`.

use std::collections::BTreeMap;

/// (section, command id, label, default key)
pub const ACTIONS: &[(&str, &str, &str, &str)] = &[
    ("Basic", "fullscreen", "Toggle fullscreen", "F"),
    ("Basic", "escape", "Exit fullscreen / close dialog", "Esc"),
    ("Basic", "show-time", "Show playback position and time", "T"),
    (
        "Basic",
        "show-controls",
        "Show interface controls in fullscreen",
        "I",
    ),
    ("Basic", "hide-ui", "Hide / show the interface", "Ctrl+\\"),
    (
        "Basic",
        "toggle-side-panel",
        "Show / hide the side panel",
        "Ctrl+B",
    ),
    (
        "Basic",
        "library",
        "Switch between player and library",
        "Ctrl+L",
    ),
    ("Playback", "play-pause", "Play / pause", "Space"),
    ("Playback", "stop", "Stop playback", "S"),
    ("Playback", "next", "Next track", "N"),
    ("Playback", "prev", "Previous track", "P"),
    ("Playback", "jump:-short", "Short skip backward", "Left"),
    ("Playback", "jump:+short", "Short skip forward", "Right"),
    (
        "Playback",
        "jump:-veryshort",
        "Very short skip backward",
        "Shift+Left",
    ),
    (
        "Playback",
        "jump:+veryshort",
        "Very short skip forward",
        "Shift+Right",
    ),
    (
        "Playback",
        "jump:-medium",
        "Medium skip backward",
        "Alt+Left",
    ),
    (
        "Playback",
        "jump:+medium",
        "Medium skip forward",
        "Alt+Right",
    ),
    ("Playback", "jump:-long", "Long skip backward", "Ctrl+Left"),
    ("Playback", "jump:+long", "Long skip forward", "Ctrl+Right"),
    ("Playback", "restart", "Restart from the beginning", "Home"),
    ("Playback", "faster", "Faster", "+"),
    ("Playback", "slower", "Slower", "-"),
    ("Playback", "normal-speed", "Normal speed", "="),
    ("Playback", "repeat", "Cycle repeat mode", "L"),
    ("Playback", "shuffle", "Toggle shuffle", "Shift+L"),
    ("Audio", "volume-up", "Volume up", "Up"),
    ("Audio", "volume-down", "Volume down", "Down"),
    ("Audio", "mute", "Mute", "M"),
    ("Audio", "cycle-audio", "Cycle audio track", "B"),
    ("Subtitles", "cycle-subtitle", "Cycle subtitle track", "V"),
    ("Subtitles", "sub-delay:-", "Subtitle delay -50 ms", "G"),
    ("Subtitles", "sub-delay:+", "Subtitle delay +50 ms", "H"),
    ("Video", "aspect", "Cycle aspect ratio", "A"),
    ("Video", "zoom", "Cycle zoom / crop", "Z"),
    ("Video", "snapshot", "Take a snapshot", "Shift+S"),
    ("Media", "open-files", "Open file(s)", "Ctrl+O"),
    ("Media", "open-folder", "Open folder", "Ctrl+F"),
    ("Media", "open-network", "Open network stream", "Ctrl+N"),
    ("Media", "settings", "Settings", "Ctrl+P"),
    ("Media", "shortcuts", "Keyboard shortcuts", "F1"),
    ("Media", "about", "About Ondrane", "Shift+F1"),
    ("Media", "quit", "Quit Ondrane", "Ctrl+Q"),
];

/// Normalize a Slint key event ("Ctrl+" prefixes + text) into "Ctrl+Shift+Left" form.
pub fn normalize(raw: &str) -> String {
    let mut rest = raw;
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    loop {
        if let Some(r) = rest.strip_prefix("Ctrl+") {
            ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Shift+") {
            shift = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Alt+") {
            alt = true;
            rest = r;
        } else {
            break;
        }
    }
    let mut chars = rest.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return String::new();
    };
    // bare modifier presses (Shift, Control, Alt, AltGr, CapsLock, Meta)
    if ('\u{10}'..='\u{18}').contains(&c) {
        return String::new();
    }
    // Ctrl+letter may arrive as an ASCII control character (Ctrl+L = 0x0C)
    let c = if ctrl && (1..=26).contains(&(c as u32)) {
        (b'A' + c as u8 - 1) as char
    } else {
        c
    };
    let name = match c {
        ' ' => "Space".to_string(),
        '\u{1b}' => "Esc".to_string(),
        '\n' | '\r' => "Enter".to_string(),
        '\t' => "Tab".to_string(),
        '\u{8}' => "Backspace".to_string(),
        '\u{7f}' => "Delete".to_string(),
        '\u{F700}' => "Up".to_string(),
        '\u{F701}' => "Down".to_string(),
        '\u{F702}' => "Left".to_string(),
        '\u{F703}' => "Right".to_string(),
        '\u{F729}' => "Home".to_string(),
        '\u{F72B}' => "End".to_string(),
        '\u{F72C}' => "PageUp".to_string(),
        '\u{F72D}' => "PageDown".to_string(),
        '\u{F704}'..='\u{F70F}' => format!("F{}", c as u32 - 0xF704 + 1),
        // bare modifier presses
        '\u{10}'..='\u{18}' => return String::new(),
        c if c.is_control() => return String::new(),
        c => {
            if c.is_alphabetic() {
                c.to_uppercase().to_string()
            } else {
                // Shift is implied by the produced symbol ("?", "+")
                shift = false;
                c.to_string()
            }
        }
    };
    let mut out = String::new();
    if ctrl {
        out.push_str("Ctrl+");
    }
    if shift {
        out.push_str("Shift+");
    }
    if alt {
        out.push_str("Alt+");
    }
    out.push_str(&name);
    out
}

/// Effective key of each action.
pub fn bindings(overrides: &BTreeMap<String, String>) -> Vec<(String, String)> {
    ACTIONS
        .iter()
        .map(|(_, id, _, key)| {
            (
                id.to_string(),
                overrides
                    .get(*id)
                    .cloned()
                    .unwrap_or_else(|| key.to_string()),
            )
        })
        .collect()
}

/// Command bound to a normalized key.
pub fn command_for(key: &str, overrides: &BTreeMap<String, String>) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    bindings(overrides)
        .into_iter()
        .find(|(_, k)| k == key)
        .map(|(id, _)| id)
}

pub fn key_of(id: &str, overrides: &BTreeMap<String, String>) -> String {
    bindings(overrides)
        .into_iter()
        .find(|(i, _)| i == id)
        .map(|(_, k)| k)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_keys() {
        assert_eq!(normalize(" "), "Space");
        assert_eq!(normalize("f"), "F");
        assert_eq!(normalize("Shift+F"), "Shift+F");
        assert_eq!(normalize("Ctrl+o"), "Ctrl+O");
        assert_eq!(normalize("Shift+?"), "?");
        assert_eq!(normalize("Shift+\u{F702}"), "Shift+Left");
        assert_eq!(normalize("Shift+\u{F704}"), "Shift+F1");
        assert_eq!(normalize("\u{10}"), "");
        assert_eq!(normalize("Ctrl+\u{c}"), "Ctrl+L");
        assert_eq!(normalize("Ctrl+\u{11}"), "");
        assert_eq!(normalize("\t"), "Tab");
    }

    #[test]
    fn overrides_win() {
        let mut o = BTreeMap::new();
        assert_eq!(command_for("Space", &o).as_deref(), Some("play-pause"));
        o.insert("play-pause".to_string(), "K".to_string());
        assert_eq!(command_for("K", &o).as_deref(), Some("play-pause"));
        assert_eq!(command_for("Space", &o), None);
        let keys: Vec<_> = ACTIONS.iter().map(|a| a.3).collect();
        let mut dedup = keys.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(keys.len(), dedup.len(), "duplicate default shortcut");
    }
}
