//! Subtitle pairing for the Figma "Add Subtitles to Playlist/Folder" dialog:
//! exact stem matches first, then episode tokens (S01E01, 1x01, E01).

use std::path::{Path, PathBuf};

use crate::library::{media_kind, MediaKind};

/// Episode token of a file name ("S01E05", "1X05", "E05", "S01E102"), upper-cased.
/// Carefully rejects video resolutions like 1920x1080 or 720x480.
pub fn episode_token(name: &str) -> Option<String> {
    let upper = name.to_uppercase();
    let b = upper.as_bytes();
    let n = b.len();

    // 1. S01E01 or S01E102 (Season 1-2 digits, Episode 2-4 digits)
    for i in 0..n {
        if b[i] == b'S' {
            let before = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            if !before {
                continue;
            }
            let mut s_end = i + 1;
            while s_end < n && b[s_end].is_ascii_digit() {
                s_end += 1;
            }
            let s_digits = s_end - (i + 1);
            if (1..=2).contains(&s_digits) && s_end < n && b[s_end] == b'E' {
                let mut e_end = s_end + 1;
                while e_end < n && b[e_end].is_ascii_digit() {
                    e_end += 1;
                }
                let e_digits = e_end - (s_end + 1);
                let after = e_end >= n || !b[e_end].is_ascii_alphanumeric();
                if (2..=4).contains(&e_digits) && after {
                    return Some(upper[i..e_end].to_string());
                }
            }
        }
    }

    // 2. 1x01 / 01x01 / 1x102 (Season exactly 1-2 digits, Episode 2-3 digits)
    // Strictly guards against resolutions like 1920x1080 or 720x480.
    for i in 1..n {
        if b[i] == b'X' {
            let mut s_start = i;
            while s_start > 0 && b[s_start - 1].is_ascii_digit() {
                s_start -= 1;
            }
            let s_digits = i - s_start;
            let before = s_start == 0 || !b[s_start - 1].is_ascii_alphanumeric();

            let mut e_end = i + 1;
            while e_end < n && b[e_end].is_ascii_digit() {
                e_end += 1;
            }
            let e_digits = e_end - (i + 1);
            let after = e_end >= n || !b[e_end].is_ascii_alphanumeric();

            if (1..=2).contains(&s_digits) && (2..=3).contains(&e_digits) && before && after {
                return Some(upper[s_start..e_end].to_string());
            }
        }
    }

    // 3. Standalone E01 / E102 (2-4 digits)
    for i in 0..n {
        if b[i] == b'E' {
            let before = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            if !before {
                continue;
            }
            let mut end = i + 1;
            while end < n && b[end].is_ascii_digit() {
                end += 1;
            }
            let digits = end - (i + 1);
            let after = end >= n || !b[end].is_ascii_alphanumeric();
            if (2..=4).contains(&digits) && after {
                return Some(upper[i..end].to_string());
            }
        }
    }
    None
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Best subtitle for a media file among `candidates`.
pub fn best_match<'a>(media: &Path, candidates: &'a [PathBuf]) -> Option<&'a PathBuf> {
    let ms = stem(media);
    if ms.is_empty() {
        return None;
    }
    // exact stem or stem + language suffix ("movie.en.srt", "movie_English.srt")
    if let Some(c) = candidates.iter().find(|c| stem(c) == ms) {
        return Some(c);
    }
    if let Some(c) = candidates.iter().find(|c| {
        let s = stem(c);
        s.starts_with(&ms) && s[ms.len()..].starts_with(['.', '_', '-', ' '])
    }) {
        return Some(c);
    }
    let token = episode_token(&ms)?;
    candidates
        .iter()
        .find(|c| episode_token(&stem(c)).as_deref() == Some(token.as_str()))
}

/// Subtitle files of a folder.
pub fn subtitles_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| media_kind(p) == MediaKind::Subtitle)
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Subtitle next to a media file, if any (used when opening a file).
pub fn find_for_media(media: &Path) -> Option<PathBuf> {
    let dir = media.parent()?;
    best_match(media, &subtitles_in(dir)).cloned()
}

/// Pair every media file with its best subtitle.
pub fn pair_all(media: &[PathBuf], subs: &[PathBuf]) -> Vec<(PathBuf, Option<PathBuf>)> {
    media
        .iter()
        .map(|m| (m.clone(), best_match(m, subs).cloned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        assert_eq!(
            episode_token("The_Wand_S01E05.mp4").as_deref(),
            Some("S01E05")
        );
        assert_eq!(episode_token("show.1x07.mkv").as_deref(), Some("1X07"));
        assert_eq!(episode_token("Show - E12 - Finale").as_deref(), Some("E12"));
        assert_eq!(episode_token("Season 2"), None);

        // Regression: video resolutions must not be parsed as episode tokens
        assert_eq!(episode_token("Movie_1080p_1920x1080.mp4"), None);
        assert_eq!(episode_token("Rip_720x480_DVD.mkv"), None);
        assert_eq!(episode_token("Rip_1280x720.mp4"), None);

        // 3-digit episode numbers
        assert_eq!(
            episode_token("Anime_S01E105.mkv").as_deref(),
            Some("S01E105")
        );
        assert_eq!(episode_token("Show_E102.mp4").as_deref(), Some("E102"));
    }

    #[test]
    fn matching() {
        let subs = vec![
            PathBuf::from("/s/The_Wand_S01E01_English.srt"),
            PathBuf::from("/s/wand.s01e02.srt"),
            PathBuf::from("/s/Movie.srt"),
        ];
        assert_eq!(best_match(Path::new("/v/Movie.mkv"), &subs), Some(&subs[2]));
        assert_eq!(
            best_match(Path::new("/v/The_Wand_S01E01.mp4"), &subs),
            Some(&subs[0])
        );
        assert_eq!(
            best_match(Path::new("/v/The_Wand_S01E02.mp4"), &subs),
            Some(&subs[1])
        );
        assert_eq!(best_match(Path::new("/v/The_Wand_S01E03.mp4"), &subs), None);
        let pairs = pair_all(&[PathBuf::from("/v/Movie.avi")], &subs);
        assert_eq!(pairs[0].1.as_ref(), Some(&subs[2]));
    }
}
