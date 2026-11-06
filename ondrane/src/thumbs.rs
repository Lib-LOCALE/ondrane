//! Seek-bar preview thumbnail cache and extraction for Ondrane.
//!
//! Provides SHA-1 keyed contact sheet caching and background thumbnail
//! extraction using a silent headless libVLC instance in 100% Pure Rust.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::engine::VlcEngine;

pub const COLS: u32 = 10;
pub const THUMB_W: u32 = 320;
pub const MAX_THUMBS: usize = 60;
pub const MIN_STEP_MS: u64 = 2000;
pub const CACHE_FILES: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbMeta {
    pub w: u32,
    pub h: u32,
    pub step: u64,
    pub have: Vec<usize>,
}

/// Compute the 20-character hex key matching `_key(path)`.
pub fn thumb_key(path: &Path) -> String {
    let sig = if let Ok(meta) = fs::metadata(path) {
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let norm = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        format!(
            "{}|{}|{}",
            norm.to_string_lossy().to_lowercase(),
            size,
            mtime
        )
    } else {
        path.to_string_lossy().to_string()
    };

    let mut hasher = Sha1::new();
    hasher.update(sig.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..20.min(hex.len())].to_string()
}

pub fn default_thumbs_dir() -> PathBuf {
    directories::ProjectDirs::from("org", "videolan", "ondrane")
        .map(|dirs| dirs.cache_dir().join("thumbs"))
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            home.join(".cache").join("ondrane").join("thumbs")
        })
}

pub struct ThumbnailCache {
    cache_dir: PathBuf,
    current_key: Option<String>,
    current_meta: Option<ThumbMeta>,
    current_sheet: Option<RgbaImage>,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ThumbnailCache {
    pub fn new(custom_dir: Option<PathBuf>) -> Self {
        let cache_dir = custom_dir.unwrap_or_else(default_thumbs_dir);
        let _ = fs::create_dir_all(&cache_dir);
        Self {
            cache_dir,
            current_key: None,
            current_meta: None,
            current_sheet: None,
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Load contact sheet and metadata for given file path if available in cache.
    pub fn load_for_media(&mut self, path: &Path) -> bool {
        let key = thumb_key(path);
        if self.current_key.as_deref() == Some(&key) && self.current_sheet.is_some() {
            return true;
        }

        self.current_key = Some(key.clone());
        self.current_meta = None;
        self.current_sheet = None;

        let meta_path = self.cache_dir.join(format!("{}.json", key));
        let jpg_path = self.cache_dir.join(format!("{}.jpg", key));

        if !meta_path.is_file() || !jpg_path.is_file() {
            return false;
        }

        let meta_data = match fs::read_to_string(&meta_path) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let meta: ThumbMeta = match serde_json::from_str(&meta_data) {
            Ok(m) => m,
            Err(_) => return false,
        };

        let img = match image::open(&jpg_path) {
            Ok(im) => im.to_rgba8(),
            Err(_) => return false,
        };

        self.current_meta = Some(meta);
        self.current_sheet = Some(img);
        true
    }

    /// Force reload of the contact sheet from disk for the given path.
    pub fn reload_for_media(&mut self, path: &Path) -> bool {
        self.current_key = None;
        self.current_meta = None;
        self.current_sheet = None;
        self.load_for_media(path)
    }

    /// Find nearest preview thumbnail frame for timestamp `ms`.
    /// Returns `(width, height, raw_rgba_pixels)`.
    pub fn frame_at(&self, ms: u64) -> Option<(u32, u32, Vec<u8>)> {
        let meta = self.current_meta.as_ref()?;
        let sheet = self.current_sheet.as_ref()?;

        if meta.step == 0 || meta.have.is_empty() {
            return None;
        }

        let target_idx = ((ms as f64) / (meta.step as f64)).round() as usize;

        // Find closest index in have
        let mut best_idx = meta.have[0];
        let mut best_diff = (best_idx as isize - target_idx as isize).abs();

        for &idx in &meta.have {
            let diff = (idx as isize - target_idx as isize).abs();
            if diff < best_diff {
                best_diff = diff;
                best_idx = idx;
            }
        }

        // Only use a neighbour that is reasonably close
        let max_distance = 30_000.max(meta.step * 12);
        if (best_diff as u64) * meta.step > max_distance {
            return None;
        }

        let col = (best_idx as u32) % COLS;
        let row = (best_idx as u32) / COLS;
        let x = col * meta.w;
        let y = row * meta.h;

        if x + meta.w > sheet.width() || y + meta.h > sheet.height() {
            return None;
        }

        let sub = imageops::crop_imm(sheet, x, y, meta.w, meta.h);
        Some((meta.w, meta.h, sub.to_image().into_raw()))
    }

    /// Prune old thumbnail files if cache exceeds limit.
    pub fn prune_cache(&self) {
        let entries = match fs::read_dir(&self.cache_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut files = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Ok(meta) = entry.metadata() {
                    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    files.push((mtime, p));
                }
            }
        }

        if files.len() > CACHE_FILES * 2 {
            files.sort_by_key(|(mtime, _)| *mtime);
            let to_remove = files.len() - (CACHE_FILES * 2);
            for (_, p) in files.into_iter().take(to_remove) {
                let _ = fs::remove_file(p);
            }
        }
    }

    /// Save contact sheet and metadata to disk.
    pub fn save_sheet(
        &mut self,
        path: &Path,
        frames: &BTreeMap<usize, image::RgbImage>,
        w: u32,
        h: u32,
        step: u64,
    ) -> bool {
        if frames.is_empty() || w == 0 || h == 0 {
            return false;
        }

        let key = thumb_key(path);
        let max_idx = *frames.keys().max().unwrap_or(&0);
        let rows = (max_idx as u32 / COLS) + 1;

        let sheet_w = w * COLS;
        let sheet_h = h * rows;
        let mut sheet = image::RgbImage::new(sheet_w, sheet_h);

        for (&idx, frame) in frames {
            let col = (idx as u32) % COLS;
            let row = (idx as u32) / COLS;
            let px = col * w;
            let py = row * h;
            imageops::replace(&mut sheet, frame, px as i64, py as i64);
        }

        let jpg_path = self.cache_dir.join(format!("{}.jpg", key));
        let meta_path = self.cache_dir.join(format!("{}.json", key));

        let Ok(mut file) = fs::File::create(&jpg_path) else {
            return false;
        };
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 82);
        if encoder
            .encode(
                sheet.as_raw(),
                sheet_w,
                sheet_h,
                image::ExtendedColorType::Rgb8,
            )
            .is_err()
        {
            return false;
        }

        let have: Vec<usize> = frames.keys().copied().collect();
        let meta = ThumbMeta { w, h, step, have };
        if let Ok(meta_json) = serde_json::to_string(&meta) {
            let _ = fs::write(&meta_path, meta_json);
        }

        self.prune_cache();

        if self.current_key.as_deref() == Some(&key) {
            self.current_meta = Some(meta);
            self.current_sheet = Some(image::DynamicImage::ImageRgb8(sheet).into_rgba8());
        }

        true
    }
}

pub struct ThumbnailMaker {
    stop_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ThumbnailMaker {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Default for ThumbnailMaker {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbnailMaker {
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    pub fn cancel(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.stop_flag.store(false, Ordering::SeqCst);
    }

    /// Start background extraction of seekbar thumbnails for a local video file.
    pub fn start<F>(&mut self, media_path: PathBuf, cache_dir: PathBuf, on_ready: F)
    where
        F: Fn() + Send + 'static,
    {
        self.cancel();
        if !media_path.is_file() {
            return;
        }

        let key = thumb_key(&media_path);
        if cache_dir.join(format!("{}.jpg", key)).is_file()
            && cache_dir.join(format!("{}.json", key)).is_file()
        {
            on_ready();
            return;
        }

        let stop = Arc::clone(&self.stop_flag);
        self.handle = Some(std::thread::spawn(move || {
            let Ok(engine) = VlcEngine::load_headless() else {
                return;
            };
            let Ok(player) = engine.create_player() else {
                return;
            };
            player.set_muted(true);
            player.set_volume(0);

            let path_str = media_path.to_string_lossy().to_string();
            if player.open(&path_str).is_err() {
                return;
            }
            player.play();

            // Wait up to 2.5s for media duration and first video frame to initialize
            let mut waited_ms = 0;
            while waited_ms < 2500 {
                if stop.load(Ordering::SeqCst) {
                    player.stop();
                    return;
                }
                let len = player.length_ms();
                let (w, h) = player.video_dimensions();
                if len > 0 && w > 0 && h > 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
                waited_ms += 50;
            }

            let total_ms = player.length_ms();
            let (vid_w, vid_h) = player.video_dimensions();
            if total_ms <= 0 || vid_w == 0 || vid_h == 0 {
                player.stop();
                return;
            }

            let thumb_w = THUMB_W;
            let thumb_h = ((thumb_w as f64 * (vid_h as f64 / vid_w as f64)).round() as u32).max(90);
            let step_ms = ((total_ms as u64) / (MAX_THUMBS as u64)).max(MIN_STEP_MS);
            let count = ((total_ms as u64) / step_ms).min(MAX_THUMBS as u64) as usize;

            let mut frames = BTreeMap::new();

            for idx in 0..=count {
                if stop.load(Ordering::SeqCst) {
                    player.stop();
                    return;
                }
                let target_ms = (idx as u64) * step_ms;
                player.seek_ms(target_ms as i64);
                std::thread::sleep(Duration::from_millis(85));

                if let Ok(state) = player.video_buffer_state.lock() {
                    if state.width > 0
                        && state.height > 0
                        && state.buffer.len() == (state.width * state.height * 4) as usize
                    {
                        if let Some(rgba) = image::RgbaImage::from_raw(
                            state.width,
                            state.height,
                            state.buffer.clone(),
                        ) {
                            let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
                            let resized = imageops::resize(
                                &rgb,
                                thumb_w,
                                thumb_h,
                                imageops::FilterType::Triangle,
                            );
                            frames.insert(idx, resized);
                        }
                    }
                }
            }

            player.stop();

            if !stop.load(Ordering::SeqCst) && !frames.is_empty() {
                let mut cache = ThumbnailCache::new(Some(cache_dir));
                if cache.save_sheet(&media_path, &frames, thumb_w, thumb_h, step_ms) {
                    on_ready();
                }
            }
        }));
    }
}

/// Path of the cached library poster for a media file.
pub fn poster_path(cache_dir: &Path, media: &Path) -> PathBuf {
    cache_dir.join(format!("{}_poster.jpg", thumb_key(media)))
}

/// Background worker generating one poster frame per video for the
/// library grid ("thumbnail" cards in the Figma design).
pub struct PosterWorker {
    tx: std::sync::mpsc::Sender<PathBuf>,
    cache_dir: PathBuf,
}

impl PosterWorker {
    /// `on_ready(media, poster)` is called from the worker thread.
    pub fn new<F>(cache_dir: PathBuf, on_ready: F) -> Self
    where
        F: Fn(PathBuf, PathBuf) + Send + 'static,
    {
        let _ = fs::create_dir_all(&cache_dir);
        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        let dir = cache_dir.clone();
        std::thread::spawn(move || {
            let mut engine: Option<VlcEngine> = None;
            let mut done = std::collections::HashSet::new();
            while let Ok(media) = rx.recv() {
                if !done.insert(media.clone()) {
                    continue;
                }
                let out = poster_path(&dir, &media);
                if out.is_file() {
                    on_ready(media, out);
                    continue;
                }
                if engine.is_none() {
                    engine = VlcEngine::load_headless().ok();
                }
                let Some(eng) = engine.as_ref() else { continue };
                if extract_poster(eng, &media, &out) {
                    on_ready(media, out);
                }
            }
        });
        Self { tx, cache_dir }
    }

    /// Cached poster if present, otherwise queue its extraction.
    pub fn request(&self, media: &Path) -> Option<PathBuf> {
        let out = poster_path(&self.cache_dir, media);
        if out.is_file() {
            return Some(out);
        }
        let _ = self.tx.send(media.to_path_buf());
        None
    }
}

fn extract_poster(engine: &VlcEngine, media: &Path, out: &Path) -> bool {
    let Ok(player) = engine.create_player() else {
        return false;
    };
    player.set_muted(true);
    if player.open(&media.to_string_lossy()).is_err() {
        return false;
    }
    player.play();
    let mut waited = 0;
    while waited < 4000 {
        let (w, h) = player.video_dimensions();
        if player.length_ms() > 0 && w > 0 && h > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        waited += 50;
    }
    let len = player.length_ms();
    if len > 10_000 {
        player.seek_ms(len / 5);
    }
    std::thread::sleep(Duration::from_millis(350));
    let frame = player.video_buffer_state.lock().ok().and_then(|st| {
        if st.width > 0 && st.height > 0 && st.buffer.len() == (st.width * st.height * 4) as usize {
            RgbaImage::from_raw(st.width, st.height, st.buffer.clone())
        } else {
            None
        }
    });
    player.stop();
    let Some(rgba) = frame else { return false };
    let w = 480u32;
    let h = ((w as f64) * rgba.height() as f64 / rgba.width() as f64)
        .round()
        .max(1.0) as u32;
    let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
    let small = imageops::resize(&rgb, w, h, imageops::FilterType::Triangle);
    small.save(out).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumb_key_generation() {
        let p = Path::new("/tmp/test_video.mkv");
        let k = thumb_key(p);
        assert_eq!(k.len(), 20);
    }

    #[test]
    fn test_thumb_cache_load_and_extract() {
        let temp = tempfile::tempdir().unwrap();
        let mut cache = ThumbnailCache::new(Some(temp.path().to_path_buf()));

        let fake_path = temp.path().join("fake_video.mp4");
        let _ = fs::write(&fake_path, b"test content for fake video");

        let mut frames = BTreeMap::new();
        let red_frame = image::RgbImage::from_fn(16, 9, |_x, _y| image::Rgb([255, 0, 0]));
        let blue_frame = image::RgbImage::from_fn(16, 9, |_x, _y| image::Rgb([0, 0, 255]));

        frames.insert(0, red_frame);
        frames.insert(1, blue_frame);

        assert!(cache.save_sheet(&fake_path, &frames, 16, 9, 2000));
        assert!(cache.load_for_media(&fake_path));

        let frame0 = cache.frame_at(100);
        assert!(frame0.is_some());
        let (w, h, data) = frame0.unwrap();
        assert_eq!(w, 16);
        assert_eq!(h, 9);
        assert_eq!(data.len(), 16 * 9 * 4);
        assert!(data[0] >= 240); // Red (accounting for lossy JPEG encoder)
        assert!(data[1] <= 15);
        assert!(data[2] <= 15);

        let frame1 = cache.frame_at(2100);
        assert!(frame1.is_some());
        let (_w, _h, data1) = frame1.unwrap();
        assert_eq!(data1.len(), 16 * 9 * 4);
        assert!(data1[2] >= 240); // Blue (accounting for lossy JPEG encoder)
        assert!(data1[0] <= 15);
        assert!(data1[1] <= 15);
    }
}
