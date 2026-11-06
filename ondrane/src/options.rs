//! The Figma settings rows that are not part of the typed settings tree, and
//! how every setting reaches libVLC: instance arguments (`--audio-filter=…`,
//! applied by restarting the engine) or per-media options (`:sub-language=…`,
//! applied when a media is opened).

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::settings::{DeinterlaceMode, OndraneSettings, SkipLoopFilter};

/// How a change to an option takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// Read by the application itself (window, OSD, tray…).
    App,
    /// A per-media option: the current media is reopened at the same time.
    Media,
    /// A libVLC instance argument: the engine is restarted.
    Engine,
}

#[derive(Debug, Clone, Copy)]
pub enum Def {
    B(bool),
    I(i64),
    F(f64),
    S(&'static str),
}

pub struct Opt {
    pub key: &'static str,
    pub def: Def,
    pub apply: Apply,
}

const fn o(key: &'static str, def: Def, apply: Apply) -> Opt {
    Opt { key, def, apply }
}

use Apply::*;
use Def::*;

pub const OPTS: &[Opt] = &[
    // General
    o("log.file", S(""), Engine),
    // Interface
    o("ui.skin", B(false), App),
    o("ui.skin-file", S(""), App),
    o("ui.resize", I(0), App),
    o("ui.remember-geometry", B(true), App),
    o("ui.geometry", S(""), App),
    o("ui.decorations", B(false), App),
    o("ui.fs-screen", I(0), App),
    o("ui.xwayland", B(false), App),
    o("ui.fs-controls", B(true), App),
    o("ui.tray", B(false), App),
    o("ui.tray-minimized", B(false), App),
    o("ui.popup", I(1), App),
    o("ui.popup-duration", F(5.0), App),
    o("ui.tree", B(false), App),
    // Privacy
    o("privacy.exclude", S(""), App),
    o("lastfm.enable", B(false), App),
    o("lastfm.user", S(""), App),
    o("lastfm.pass", S(""), App),
    o("lastfm.url", S("post.audioscrobbler.com"), App),
    // Controls
    o("mouse.wheel", I(0), App),
    o("mouse.hwheel", I(0), App),
    o("remote.web", B(false), App),
    o("remote.port", I(8080), App),
    o("remote.password", S(""), App),
    // Playback
    o("play.cork", B(true), App),
    o("play.quality", I(0), Media),
    o("play.menu-lang", I(0), Media),
    o("play.audio-track", I(0), Media),
    o("play.sub-track", I(0), Media),
    o("play.finish", I(0), App),
    o("play.precise", B(true), Media),
    o("play.drive", S(""), App),
    o("play.avi", I(3), Media),
    o("play.mkv-preload", B(true), Media),
    // Video
    o("video.deint-method", I(0), App),
    o("video.scale", B(true), App),
    o("video.zoom", I(0), App),
    o("video.position", I(0), App),
    o("vf.extract", B(false), Engine),
    o("vf.extract-color", S("#FF0000"), Engine),
    o("vf.threshold", B(false), Engine),
    o("vf.threshold-color", S("#FF0000"), Engine),
    o("vf.threshold-sat", F(20.0), Engine),
    o("vf.threshold-sim", F(15.0), Engine),
    o("vf.invert", B(false), Engine),
    o("vf.posterize", B(false), Engine),
    o("vf.gradient", B(false), Engine),
    o("vf.gradient-mode", I(0), Engine),
    o("vf.gradient-color", B(false), Engine),
    o("vf.cartoon", B(false), Engine),
    o("vf.sepia", F(0.0), Engine),
    o("crop.top", I(0), App),
    o("crop.bottom", I(0), App),
    o("crop.left", I(0), App),
    o("crop.right", I(0), App),
    o("snap.prefix", S("vlcsnap-"), App),
    o("snap.size", I(0), App),
    o("snap.sequential", B(true), App),
    o("snap.preview", B(true), App),
    // Audio
    o("audio.device", S(""), App),
    o("audio.speakers", I(0), App),
    o("audio.normvol", B(false), Engine),
    o("audio.norm-level", F(2.0), Engine),
    o("audio.night", B(false), Engine),
    o("audio.replaygain", I(0), Engine),
    o("audio.rg-preamp", F(0.0), Engine),
    o("audio.eq-2pass", B(false), Engine),
    o("audio.spatial", B(false), Engine),
    o("audio.spatial-mode", I(0), Engine),
    o("audio.widen", B(false), Engine),
    o("audio.compressor", B(false), Engine),
    o("audio.pitch", F(0.0), Engine),
    o("audio.chorus", B(false), Engine),
    o("audio.karaoke", B(false), Engine),
    o("comp.rms", F(0.2), Engine),
    o("comp.attack", F(25.0), Engine),
    o("comp.release", F(100.0), Engine),
    o("comp.threshold", F(-11.0), Engine),
    o("comp.ratio", F(4.0), Engine),
    o("comp.knee", F(5.0), Engine),
    o("comp.makeup", F(7.0), Engine),
    o("sp.size", F(8.0), Engine),
    o("sp.width", F(10.0), Engine),
    o("sp.wet", F(4.0), Engine),
    o("sp.dry", F(5.0), Engine),
    o("sp.damp", F(5.0), Engine),
    o("widen.delay", F(20.0), Engine),
    o("widen.feedback", F(0.3), Engine),
    o("widen.crossfeed", F(0.3), Engine),
    o("widen.dry", F(0.8), Engine),
    o("audio.lang", I(0), Media),
    o("audio.visual", I(0), Engine),
    // Subtitles
    o("sub.lang", I(0), Media),
    o("sub.encoding", I(0), Media),
    o("sub.fuzzy", I(1), Media),
    o("sub.font", S(""), Engine),
    o("sub.size-mode", I(0), Engine),
    o("sub.size", I(0), Engine),
    o("sub.size-px", F(32.0), Engine),
    o("sub.bold", B(false), Engine),
    o("sub.color", S("#FFFFFF"), Engine),
    o("sub.opacity", F(100.0), Engine),
    o("sub.outline", I(2), Engine),
    o("sub.outline-color", S("#000000"), Engine),
    o("sub.shadow", B(true), Engine),
    o("sub.shadow-color", S("#000000"), Engine),
    o("sub.shadow-dist", I(0), Engine),
    o("sub.background", I(0), Engine),
    o("sub.bg-color", S("#000000"), Engine),
    o("sub.bg-opacity", F(50.0), Engine),
    o("sub.position", I(0), Engine),
    o("osd.feedback", B(true), App),
    o("osd.feedback-dur", F(1.8), App),
    o("osd.filename", B(true), App),
    o("osd.filename-dur", F(3.0), App),
    // Playlists, folders & files
    o("lib.single", B(false), App),
    o("lib.fm-apply", B(true), App),
    o("lib.enqueue", B(false), App),
    o("lib.folder", I(0), App),
    o("lib.use-ml", B(true), App),
    // Network
    o("net.preset", I(1), App),
    o("net.file-caching", F(1.0), Media),
    o("net.disc-caching", F(0.3), Media),
    o("net.live-caching", F(0.3), Media),
    o("net.proxy-pwd", S(""), Media),
    o("net.verify", B(true), App),
    o("net.cipher", I(1), Engine),
    // Performance
    o("perf.postproc", I(0), Engine),
    o("perf.rt", B(false), Engine),
    o("perf.priority", I(0), Engine),
    o("perf.encoder", I(0), App),
    o("perf.decoder", I(0), Media),
    o("perf.clock-sync", I(0), Media),
    o("perf.clock-jitter", I(5000), Media),
    o("perf.cr-average", I(40), Media),
    // Recording, casting & streaming
    o("rec.copy", B(true), App),
    o("cast.discover", B(true), App),
    o("cast.quality", I(0), Media),
    o("cast.ip", S(""), App),
    o("stream.local", B(true), App),
    o("stream.audio", B(true), App),
    o("stream.video", B(true), App),
    o("stream.subs", B(true), App),
    o("stream.preset", I(5), App),
    o("stream.tune", I(0), App),
    o("stream.port", I(8090), App),
];

pub fn find(key: &str) -> Option<&'static Opt> {
    OPTS.iter().find(|o| o.key == key)
}

// ------------------------------------------------------------ value lists

/// (label, libVLC language list)
pub const LANGUAGES: [(&str, &str); 9] = [
    ("System language", ""),
    ("English", "en,eng"),
    ("French", "fr,fre,fra"),
    ("German", "de,ger,deu"),
    ("Spanish", "es,spa"),
    ("Italian", "it,ita"),
    ("Portuguese", "pt,por"),
    ("Japanese", "ja,jpn"),
    ("Original", "und"),
];
pub const ENCODINGS: [(&str, &str); 9] = [
    ("Automatic", ""),
    ("UTF-8", "UTF-8"),
    ("Western (Windows-1252)", "Windows-1252"),
    ("Western (ISO-8859-15)", "ISO-8859-15"),
    ("Central European (Windows-1250)", "Windows-1250"),
    ("Cyrillic (Windows-1251)", "Windows-1251"),
    ("Arabic (Windows-1256)", "Windows-1256"),
    ("Chinese (GB18030)", "GB18030"),
    ("Japanese (Shift-JIS)", "Shift_JIS"),
];
pub const FUZZY: [(&str, i64); 4] = [
    ("Exact file name", 4),
    ("Title with extra characters", 3),
    ("Title anywhere in the name", 2),
    ("Any subtitle file of the folder", 1),
];
pub const DEINT_METHODS: [(&str, &str); 11] = [
    ("Automatic", "yadif"),
    ("Blend", "blend"),
    ("Bob", "bob"),
    ("Discard", "discard"),
    ("Linear", "linear"),
    ("Mean", "mean"),
    ("X", "x"),
    ("Yadif", "yadif"),
    ("Yadif (2x)", "yadif2x"),
    ("Phosphor", "phosphor"),
    ("Film NTSC (IVTC)", "ivtc"),
];
pub const QUALITIES: [(&str, i64); 6] = [
    ("Best available", -1),
    ("2160p", 2160),
    ("1080p", 1080),
    ("720p", 720),
    ("480p", 480),
    ("360p", 360),
];
pub const VISUALS: [(&str, &str); 7] = [
    ("Disabled", ""),
    ("Spectrometer", "visual:spectrometer"),
    ("Spectrum", "visual:spectrum"),
    ("Scope", "visual:scope"),
    ("VU meter", "visual:vuMeter"),
    ("Goom", "goom"),
    ("3D spectrum", "glspectrum"),
];
pub const X264_PRESETS: [&str; 9] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];
pub const X264_TUNES: [(&str, &str); 7] = [
    ("None", ""),
    ("Film", "film"),
    ("Animation", "animation"),
    ("Grain", "grain"),
    ("Still image", "stillimage"),
    ("Fast decode", "fastdecode"),
    ("Zero latency", "zerolatency"),
];
pub const DECODERS: [(&str, &str); 3] = [
    ("Automatic", ""),
    ("FFmpeg (avcodec)", "avcodec,any"),
    ("dav1d (AV1)", "dav1d,any"),
];
pub const ENCODERS: [(&str, &str); 5] = [
    ("Automatic", ""),
    ("x264 (H.264)", "x264"),
    ("x265 (HEVC)", "x265"),
    ("VP8 / VP9 (libvpx)", "vpx"),
    ("FFmpeg (avcodec)", "avcodec"),
];
/// Snapshot widths ("Same as video" = 0).
pub const SNAP_WIDTHS: [(&str, u32); 5] = [
    ("Same as video", 0),
    ("1920 px wide", 1920),
    ("1280 px wide", 1280),
    ("640 px wide", 640),
    ("320 px wide", 320),
];
/// Buffering presets: (label, file, disc, network, live) seconds.
pub const BUFFER_PRESETS: [(&str, f64, f64, f64, f64); 3] = [
    ("Low latency", 0.3, 0.3, 0.3, 0.1),
    ("Normal", 1.0, 0.3, 1.0, 0.3),
    ("High", 3.0, 1.0, 5.0, 1.0),
];

// --------------------------------------------------------------- accessors

impl OndraneSettings {
    fn extra_or_default(&self, key: &str) -> Value {
        if let Some(v) = self.extra.get(key) {
            return v.clone();
        }
        match find(key).map(|o| o.def) {
            Some(B(b)) => Value::from(b),
            Some(I(i)) => Value::from(i),
            Some(F(f)) => Value::from(f),
            Some(S(s)) => Value::from(s),
            None => Value::Null,
        }
    }

    pub fn ob(&self, key: &str) -> bool {
        self.extra_or_default(key).as_bool().unwrap_or(false)
    }

    pub fn oi(&self, key: &str) -> i64 {
        let v = self.extra_or_default(key);
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .unwrap_or(0)
    }

    pub fn of(&self, key: &str) -> f64 {
        self.extra_or_default(key).as_f64().unwrap_or(0.0)
    }

    pub fn os(&self, key: &str) -> String {
        self.extra_or_default(key)
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    pub fn set_extra(&mut self, key: &str, v: impl Into<Value>) {
        self.extra.insert(key.to_string(), v.into());
    }
}

fn pick<T: Copy>(list: &[(&str, T)], i: i64) -> T {
    if list.is_empty() {
        panic!("pick called on empty list");
    }
    list[(i.max(0) as usize).min(list.len() - 1)].1
}

/// "#RRGGBB" → 0xRRGGBB for `--freetype-*-color`. Defaults to white if invalid.
fn color_int(hex: &str) -> u32 {
    let clean = hex.trim().trim_start_matches('#');
    if clean.is_empty() {
        0x00FF_FFFF
    } else {
        u32::from_str_radix(clean, 16).unwrap_or(0x00FF_FFFF) & 0xFF_FFFF
    }
}

pub fn default_log_file() -> PathBuf {
    OndraneSettings::config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join("ondrane.log")
}

pub fn log_file(s: &OndraneSettings) -> PathBuf {
    let custom = s.os("log.file");
    if custom.trim().is_empty() {
        default_log_file()
    } else {
        crate::library::expand_tilde(custom.trim())
    }
}

// libVLC 3 hardware surfaces cannot reliably feed the Linux RGBA memory output.
fn hardware_decoder(s: &OndraneSettings) -> &'static str {
    if s.media.video.hardware_decoding
        && !(cfg!(target_os = "linux") && s.app.interface.embedded_video)
    {
        "any"
    } else {
        "none"
    }
}

/// libVLC instance arguments for the current settings.
pub fn engine_args(s: &OndraneSettings) -> Vec<String> {
    // Set this at instance creation as well as on media, including background
    // players. VLC 3 may still probe output formats before rejecting hardware.
    let mut a = vec![format!("--avcodec-hw={}", hardware_decoder(s))];
    let g = &s.app.global;
    if g.developer_mode {
        a.push("--verbose=2".into());
    } else {
        a.push("--quiet".into());
    }
    if g.save_log {
        a.push("--file-logging".into());
        a.push(format!("--logfile={}", log_file(s).display()));
        a.push(format!("--log-verbose={}", g.log_detail.min(2)));
    }
    a.push(
        if s.app.privacy.fetch_remote_metadata {
            "--metadata-network-access"
        } else {
            "--no-metadata-network-access"
        }
        .into(),
    );

    // audio filters
    let mut filters: Vec<&str> = Vec::new();
    if s.ob("audio.normvol") {
        filters.push("normvol");
        a.push(format!("--norm-max-level={:.1}", s.of("audio.norm-level")));
    }
    if s.ob("audio.night") {
        filters.push("compressor");
        a.extend([
            "--compressor-rms-peak=0.2".into(),
            "--compressor-attack=25".into(),
            "--compressor-release=100".into(),
            "--compressor-threshold=-24".into(),
            "--compressor-ratio=8".into(),
            "--compressor-knee=2.5".into(),
            "--compressor-makeup-gain=9".into(),
        ]);
    } else if s.ob("audio.compressor") {
        filters.push("compressor");
        a.push(format!("--compressor-rms-peak={:.2}", s.of("comp.rms")));
        a.push(format!("--compressor-attack={:.1}", s.of("comp.attack")));
        a.push(format!("--compressor-release={:.1}", s.of("comp.release")));
        a.push(format!(
            "--compressor-threshold={:.1}",
            s.of("comp.threshold")
        ));
        a.push(format!("--compressor-ratio={:.1}", s.of("comp.ratio")));
        a.push(format!("--compressor-knee={:.1}", s.of("comp.knee")));
        a.push(format!(
            "--compressor-makeup-gain={:.1}",
            s.of("comp.makeup")
        ));
    }
    if s.ob("audio.spatial") {
        if s.oi("audio.spatial-mode") == 0 {
            filters.push("headphone");
        } else {
            filters.push("spatializer");
            // the Figma panel shows the Qt 0..10 scale
            a.push(format!(
                "--spatializer-roomsize={:.2}",
                s.of("sp.size") / 10.0
            ));
            a.push(format!(
                "--spatializer-width={:.2}",
                s.of("sp.width") / 10.0
            ));
            a.push(format!("--spatializer-wet={:.2}", s.of("sp.wet") / 10.0));
            a.push(format!("--spatializer-dry={:.2}", s.of("sp.dry") / 10.0));
            a.push(format!("--spatializer-damp={:.2}", s.of("sp.damp") / 10.0));
        }
    }
    if s.ob("audio.widen") {
        filters.push("stereo_widen");
        a.push(format!("--stereowiden-delay={:.1}", s.of("widen.delay")));
        a.push(format!(
            "--stereowiden-feedback={:.2}",
            s.of("widen.feedback")
        ));
        a.push(format!(
            "--stereowiden-crossfeed={:.2}",
            s.of("widen.crossfeed")
        ));
        a.push(format!("--stereowiden-dry-mix={:.2}", s.of("widen.dry")));
    }
    let pitch = s.of("audio.pitch");
    if pitch.abs() > 0.01 {
        filters.push("scaletempo_pitch");
        a.push(format!("--pitch-shift={:.2}", pitch));
    }
    if s.ob("audio.chorus") {
        filters.push("chorus_flanger");
    }
    if s.ob("audio.karaoke") {
        filters.push("karaoke");
    }
    if !filters.is_empty() {
        a.push(format!("--audio-filter={}", filters.join(":")));
    }
    if s.ob("audio.eq-2pass") {
        a.push("--equalizer-2pass".into());
    }
    let rg = ["none", "track", "album"][s.oi("audio.replaygain").clamp(0, 2) as usize];
    a.push(format!("--audio-replay-gain-mode={rg}"));
    a.push(format!(
        "--audio-replay-gain-preamp={:.1}",
        s.of("audio.rg-preamp")
    ));
    let visual = pick(&VISUALS, s.oi("audio.visual"));
    if let Some((module, effect)) = visual.split_once(':') {
        a.push(format!("--audio-visual={module}"));
        a.push(format!("--effect-list={effect}"));
    } else if !visual.is_empty() {
        a.push(format!("--audio-visual={visual}"));
    }

    // subtitle rendering (freetype)
    let font = s.os("sub.font");
    if !font.trim().is_empty() {
        a.push(format!("--freetype-font={}", font.trim()));
    }
    if s.oi("sub.size-mode") == 1 {
        a.push(format!(
            "--freetype-fontsize={}",
            s.of("sub.size-px").round() as i64
        ));
    } else {
        let rel = [0, 20, 18, 16, 12, 6][s.oi("sub.size").clamp(0, 5) as usize];
        a.push(format!("--freetype-rel-fontsize={rel}"));
    }
    if s.ob("sub.bold") {
        a.push("--freetype-bold".into());
    }
    a.push(format!(
        "--freetype-color={}",
        color_int(&s.os("sub.color"))
    ));
    a.push(format!(
        "--freetype-opacity={}",
        (s.of("sub.opacity") * 2.55).round().clamp(0.0, 255.0) as i64
    ));
    a.push(format!(
        "--freetype-outline-thickness={}",
        [0, 2, 4, 6][s.oi("sub.outline").clamp(0, 3) as usize]
    ));
    a.push(format!(
        "--freetype-outline-color={}",
        color_int(&s.os("sub.outline-color"))
    ));
    a.push(format!(
        "--freetype-shadow-opacity={}",
        if s.ob("sub.shadow") { 128 } else { 0 }
    ));
    a.push(format!(
        "--freetype-shadow-color={}",
        color_int(&s.os("sub.shadow-color"))
    ));
    a.push(format!(
        "--freetype-shadow-distance={}",
        [0.04, 0.07, 0.1][s.oi("sub.shadow-dist").clamp(0, 2) as usize]
    ));
    let bg = if s.oi("sub.background") == 1 {
        (s.of("sub.bg-opacity") * 2.55).round().clamp(0.0, 255.0) as i64
    } else {
        0
    };
    a.push(format!("--freetype-background-opacity={bg}"));
    a.push(format!(
        "--freetype-background-color={}",
        color_int(&s.os("sub.bg-color"))
    ));
    a.push(format!(
        "--sub-margin={}",
        [0, 60, 160][s.oi("sub.position").clamp(0, 2) as usize]
    ));

    // network & performance
    a.push(format!(
        "--gnutls-priorities={}",
        ["PERFORMANCE", "NORMAL", "SECURE256"][s.oi("net.cipher").clamp(0, 2) as usize]
    ));
    // video filters: post-processing and the "Video - Colors" panel
    let mut vf: Vec<&str> = Vec::new();
    let pp = s.oi("perf.postproc").clamp(0, 3);
    if pp > 0 {
        vf.push("postproc");
        a.push(format!("--postproc-q={}", pp * 2));
    }
    if s.ob("vf.extract") {
        vf.push("extract");
        a.push(format!(
            "--extract-component={}",
            color_int(&s.os("vf.extract-color"))
        ));
    }
    if s.ob("vf.threshold") {
        vf.push("colorthres");
        a.push(format!(
            "--colorthres-color={}",
            color_int(&s.os("vf.threshold-color"))
        ));
        a.push(format!(
            "--colorthres-saturationthres={}",
            s.of("vf.threshold-sat").round() as i64
        ));
        a.push(format!(
            "--colorthres-similaritythres={}",
            s.of("vf.threshold-sim").round() as i64
        ));
    }
    if s.ob("vf.invert") {
        vf.push("invert");
    }
    if s.ob("vf.posterize") {
        vf.push("posterize");
    }
    if s.ob("vf.gradient") {
        vf.push("gradient");
        a.push(format!(
            "--gradient-mode={}",
            ["gradient", "edge", "hough"][s.oi("vf.gradient-mode").clamp(0, 2) as usize]
        ));
        a.push(format!(
            "--gradient-type={}",
            s.ob("vf.gradient-color") as i32
        ));
        a.push(
            if s.ob("vf.cartoon") {
                "--gradient-cartoon"
            } else {
                "--no-gradient-cartoon"
            }
            .into(),
        );
    }
    let sepia = s.of("vf.sepia").round() as i64;
    if sepia > 0 {
        vf.push("sepia");
        a.push(format!("--sepia-intensity={}", sepia.min(255)));
    }
    if !vf.is_empty() {
        a.push(format!("--video-filter={}", vf.join(":")));
    }
    if s.ob("perf.rt") {
        a.push("--rt-priority".into());
        a.push(format!(
            "--rt-offset={}",
            s.oi("perf.priority").clamp(-20, 20)
        ));
    }
    a
}

/// Per-media options for `target` (without the resume position).
pub fn media_options(s: &OndraneSettings, is_url: bool) -> Vec<String> {
    let secs_ms = |k: &str| (s.of(k) * 1000.0).round().max(0.0) as i64;
    let mut o = vec![
        format!(":network-caching={}", s.system.network.network_caching_ms),
        format!(":file-caching={}", secs_ms("net.file-caching")),
        format!(":disc-caching={}", secs_ms("net.disc-caching")),
        format!(":live-caching={}", secs_ms("net.live-caching")),
        format!(":avcodec-hw={}", hardware_decoder(s)),
        format!(":avcodec-threads={}", s.system.performance.decoding_threads),
        format!(
            ":avcodec-skiploopfilter={}",
            match s.system.performance.skip_loop_filter {
                SkipLoopFilter::None => 0,
                SkipLoopFilter::NonRef => 1,
                SkipLoopFilter::Bidir => 2,
                SkipLoopFilter::NonKey => 3,
                SkipLoopFilter::All => 4,
            }
        ),
        format!(
            ":deinterlace={}",
            match s.media.video.deinterlace {
                DeinterlaceMode::Off => 0,
                DeinterlaceMode::Automatic => -1,
                DeinterlaceMode::On => 1,
            }
        ),
        format!(
            ":deinterlace-mode={}",
            pick(&DEINT_METHODS, s.oi("video.deint-method"))
        ),
        format!(
            ":preferred-resolution={}",
            pick(&QUALITIES, s.oi("play.quality"))
        ),
        format!(":avi-index={}", s.oi("play.avi").clamp(0, 3)),
        format!(":sub-autodetect-fuzzy={}", pick(&FUZZY, s.oi("sub.fuzzy"))),
        format!(
            ":clock-synchro={}",
            [-1, 0, 1][s.oi("perf.clock-sync").clamp(0, 2) as usize]
        ),
        format!(":clock-jitter={}", s.oi("perf.clock-jitter").max(0)),
        format!(":cr-average={}", s.oi("perf.cr-average").max(1)),
        format!(
            ":sout-chromecast-conversion-quality={}",
            [2, 0, 1, 3][s.oi("cast.quality").clamp(0, 3) as usize]
        ),
    ];
    if !s.ob("play.mkv-preload") {
        o.push(":no-mkv-preload-local-dir".into());
    }
    if !s.ob("play.precise") {
        o.push(":input-fast-seek".into());
    }
    let menu = pick(&LANGUAGES, s.oi("play.menu-lang"));
    if !menu.is_empty() {
        o.push(format!(
            ":menu-language={}",
            menu.split(',').next().unwrap_or("")
        ));
    }
    for (key, opt) in [
        ("play.audio-track", "audio-track"),
        ("play.sub-track", "sub-track"),
    ] {
        let n = s.oi(key);
        if n > 0 {
            o.push(format!(":{opt}={}", n - 1));
        }
    }
    for (key, opt) in [
        ("audio.lang", "audio-language"),
        ("sub.lang", "sub-language"),
    ] {
        let l = pick(&LANGUAGES, s.oi(key));
        if !l.is_empty() {
            o.push(format!(":{opt}={l}"));
        }
    }
    let enc = pick(&ENCODINGS, s.oi("sub.encoding"));
    if !enc.is_empty() {
        o.push(format!(":subsdec-encoding={enc}"));
    }
    let dec = pick(&DECODERS, s.oi("perf.decoder"));
    if !dec.is_empty() {
        o.push(format!(":codec={dec}"));
    }
    if !s.system.performance.drop_late_frames {
        o.push(":no-drop-late-frames".into());
    }
    if !s.media.video.enable_video {
        o.push(":no-video".into());
    }
    if !s.media.audio.enable_audio {
        o.push(":no-audio".into());
    }
    if !s.media.subtitles.enabled {
        o.push(":no-spu".into());
    }
    if !s.media.subtitles.auto_load {
        o.push(":no-sub-autodetect-file".into());
    }
    if let Some(proxy) = s
        .system
        .network
        .proxy_url
        .as_ref()
        .filter(|p| !p.is_empty())
    {
        o.push(format!(":http-proxy={proxy}"));
        let pwd = s.os("net.proxy-pwd");
        if !pwd.is_empty() {
            o.push(format!(":http-proxy-pwd={pwd}"));
        }
    }
    if is_url {
        o.push(format!(":http-user-agent={}", s.system.network.user_agent));
    }
    if !s.media.playback.start_on_open {
        o.push(":start-paused".into());
    }
    o
}

/// Video encoder chain for transcoding (record / stream / convert).
pub fn x264_venc(s: &OndraneSettings) -> String {
    let preset = X264_PRESETS[(s.oi("stream.preset").clamp(0, 8)) as usize];
    let tune = pick(&X264_TUNES, s.oi("stream.tune"));
    let enc = pick(&ENCODERS, s.oi("perf.encoder"));
    match enc {
        "" | "x264" => {
            if tune.is_empty() {
                format!("x264{{preset={preset}}}")
            } else {
                format!("x264{{preset={preset},tune={tune}}}")
            }
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_decoding_matches_the_video_output() {
        let mut s = OndraneSettings::default();
        let expected = if cfg!(target_os = "linux") {
            "none"
        } else {
            "any"
        };
        assert!(engine_args(&s).contains(&format!("--avcodec-hw={expected}")));
        s.app.interface.embedded_video = false;
        assert!(engine_args(&s).contains(&"--avcodec-hw=any".into()));
        s.media.video.hardware_decoding = false;
        assert!(engine_args(&s).contains(&"--avcodec-hw=none".into()));
        assert!(media_options(&s, false).contains(&":avcodec-hw=none".into()));
    }

    #[test]
    fn defaults_and_overrides() {
        let mut s = OndraneSettings::default();
        assert!(s.ob("play.precise"));
        assert_eq!(s.oi("remote.port"), 8080);
        s.set_extra("audio.normvol", true);
        s.set_extra("audio.pitch", 2.0);
        let args = engine_args(&s);
        assert!(args
            .iter()
            .any(|a| a == "--audio-filter=normvol:scaletempo_pitch"));
        s.set_extra("sub.lang", 2);
        let o = media_options(&s, false);
        assert!(o.iter().any(|x| x == ":sub-language=fr,fre,fra"));
        assert!(OPTS.iter().all(|o| find(o.key).is_some()));

        // Color fallback test
        assert_eq!(color_int("#FF0000"), 0xFF0000);
        assert_eq!(color_int("invalid"), 0xFFFFFF);
        assert_eq!(color_int(""), 0xFFFFFF);
    }
}
