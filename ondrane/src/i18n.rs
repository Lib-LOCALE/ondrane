//! Interface language ("Settings › General › Interface language").
//!
//! The Slint files mark their texts with `@tr(...)`; the strings built in Rust
//! (menus, settings rows, notifications) go through [`tr`]. Both read the same
//! gettext catalogues, `lang/<code>/LC_MESSAGES/ondrane.po`, bundled into
//! the binary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// (code, native name, catalogue)
const CATALOGUES: [(&str, &str, &str); 2] = [
    ("en", "English", ""),
    (
        "fr",
        "Français",
        include_str!("../lang/fr/LC_MESSAGES/ondrane.po"),
    ),
];

/// Choices of the "Interface language" dropdown: system, then each language.
pub fn choices() -> Vec<String> {
    let mut v = vec![tr("System language")];
    v.extend(CATALOGUES.iter().map(|c| c.1.to_string()));
    v
}

/// Index in [`choices`] of a stored language code ("system", "en", "fr").
pub fn choice_index(code: &str) -> usize {
    CATALOGUES
        .iter()
        .position(|c| c.0 == code)
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// Stored code of a [`choices`] index.
pub fn choice_code(index: usize) -> &'static str {
    match index {
        0 => "system",
        i => CATALOGUES.get(i - 1).map(|c| c.0).unwrap_or("system"),
    }
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn tables() -> &'static Vec<HashMap<String, String>> {
    static T: OnceLock<Vec<HashMap<String, String>>> = OnceLock::new();
    T.get_or_init(|| CATALOGUES.iter().map(|c| parse_po(c.2)).collect())
}

/// Language of the desktop session, reduced to a code we have ("fr"/"en").
pub fn system_language() -> &'static str {
    let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    CATALOGUES
        .iter()
        .find(|c| locale.starts_with(c.0))
        .map(|c| c.0)
        .unwrap_or("en")
}

/// Activate a language ("system", "en", "fr"); returns the effective code.
pub fn set_language(code: &str) -> &'static str {
    let code = if code == "system" || code.is_empty() {
        system_language()
    } else {
        code
    };
    let idx = CATALOGUES.iter().position(|c| c.0 == code).unwrap_or(0);
    ACTIVE.store(idx, Ordering::SeqCst);
    let eff = CATALOGUES[idx].0;
    let _ = slint::select_bundled_translation(eff);
    eff
}

pub fn current() -> &'static str {
    CATALOGUES[ACTIVE.load(Ordering::SeqCst)].0
}

/// Translate an English interface string.
pub fn tr(s: &str) -> String {
    let i = ACTIVE.load(Ordering::SeqCst);
    if i == 0 {
        return s.to_string();
    }
    tables()[i]
        .get(s)
        .filter(|t| !t.is_empty())
        .cloned()
        .unwrap_or_else(|| s.to_string())
}

/// Translate a template and substitute its `{}` placeholders in order.
pub fn tr_fmt(template: &str, args: &[&str]) -> String {
    let t = tr(template);
    let mut out = String::with_capacity(t.len() + 16);
    let mut rest = t.as_str();
    let mut args = args.iter();
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        out.push_str(args.next().copied().unwrap_or(""));
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Minimal gettext `.po` reader (msgid / msgstr, multi-line strings).
pub fn parse_po(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let (mut id, mut st) = (String::new(), String::new());
    let mut cur: Option<bool> = None; // Some(true) msgid, Some(false) msgstr
    let mut flush = |id: &mut String, st: &mut String| {
        if !id.is_empty() {
            map.insert(std::mem::take(id), std::mem::take(st));
        } else {
            st.clear();
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("msgid ") {
            flush(&mut id, &mut st);
            id = unquote(rest);
            cur = Some(true);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            st = unquote(rest);
            cur = Some(false);
        } else if line.starts_with('"') {
            match cur {
                Some(true) => id.push_str(&unquote(line)),
                Some(false) => st.push_str(&unquote(line)),
                None => {}
            }
        } else if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("msgctxt")
            || line.starts_with("msgid_plural")
            || line.starts_with("msgstr[")
        {
            cur = None;
        }
    }
    flush(&mut id, &mut st);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn po_and_templates() {
        let m = parse_po("msgid \"Hello\"\nmsgstr \"Bonjour\"\n\nmsgid \"\"\n\"Two \"\n\"lines\"\nmsgstr \"Deux lignes\"\n");
        assert_eq!(m["Hello"], "Bonjour");
        assert_eq!(m["Two lines"], "Deux lignes");
        assert_eq!(tr_fmt("{} of {}", &["1", "2"]), "1 of 2");

        // Escaped quotes and backslashes
        let escaped = parse_po("msgid \"Quote \\\"test\\\" and backslash \\\\\"\nmsgstr \"Citation \\\"test\\\" et antislash \\\\\"\n");
        assert_eq!(
            escaped["Quote \"test\" and backslash \\"],
            "Citation \"test\" et antislash \\"
        );

        // every French entry is translated
        let fr = parse_po(CATALOGUES[1].2);
        assert!(fr.len() > 300);
        let missing: Vec<_> = fr
            .iter()
            .filter(|(_, v)| v.is_empty())
            .map(|(k, _)| k)
            .collect();
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }
}
