//! Dynamic FFI Bridge to libVLC (libvlc.so.5 / libvlc.dll / libvlc.dylib)
//!
//! Provides safe, high-performance bindings to the native libVLC media engine
//! across Linux (X11/Wayland/PulseAudio/PipeWire) and Windows 10/11 x64 (Direct3D11/WASAPI).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

mod ext;
use ext::*;
pub use ext::{DialogEvent, DialogId, PlayerEvent, PlayerEvents, RendererDiscovery, RendererItem};

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Failed to dynamically load libVLC: {0}")]
    LibraryLoadError(String),
    #[error("Failed to resolve libVLC symbol '{0}': {1}")]
    SymbolError(String, String),
    #[error("Failed to initialize libVLC instance (libvlc_new returned NULL)")]
    InstanceInitError,
    #[error("Failed to instantiate media player (libvlc_media_player_new returned NULL)")]
    PlayerInitError,
    #[error("Failed to create media target '{0}'")]
    MediaCreationError(String),
    #[error("Invalid C-string argument: {0}")]
    CStringError(#[from] std::ffi::NulError),
    #[error("Snapshot failed to write to '{0}'")]
    SnapshotError(String),
}

// ============================================================================
// LOW-LEVEL C STRUCTS & ENUMS
// ============================================================================

enum LibVlcInstance {}
enum LibVlcMediaPlayer {}
enum LibVlcMedia {}

#[repr(C)]
struct LibVlcTrackDescription {
    i_id: c_int,
    psz_name: *mut c_char,
    p_next: *mut LibVlcTrackDescription,
}

#[repr(C)]
struct LibVlcAudioOutputDevice {
    p_next: *mut LibVlcAudioOutputDevice,
    psz_device: *mut c_char,
    psz_description: *mut c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibVlcState {
    NothingSpecial = 0,
    Opening = 1,
    Buffering = 2,
    Playing = 3,
    Paused = 4,
    Stopped = 5,
    Ended = 6,
    Error = 7,
}

/// High-level playback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Opening,
    Buffering,
    Playing,
    Paused,
    Stopped,
    Ended,
    Error,
}

impl From<c_int> for PlaybackState {
    fn from(val: c_int) -> Self {
        match val {
            1 => PlaybackState::Opening,
            2 => PlaybackState::Buffering,
            3 => PlaybackState::Playing,
            4 => PlaybackState::Paused,
            5 => PlaybackState::Stopped,
            6 => PlaybackState::Ended,
            7 => PlaybackState::Error,
            _ => PlaybackState::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub description: String,
}

// ============================================================================
// DYNAMIC FUNCTION TABLE
// ============================================================================

type FnLibvlcNew =
    unsafe extern "C" fn(argc: c_int, argv: *const *const c_char) -> *mut LibVlcInstance;
type FnLibvlcRelease = unsafe extern "C" fn(p_instance: *mut LibVlcInstance);
type FnLibvlcGetVersion = unsafe extern "C" fn() -> *const c_char;

type FnMediaPlayerNew =
    unsafe extern "C" fn(p_instance: *mut LibVlcInstance) -> *mut LibVlcMediaPlayer;
type FnMediaPlayerRelease = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer);
type FnMediaNewPath =
    unsafe extern "C" fn(p_instance: *mut LibVlcInstance, path: *const c_char) -> *mut LibVlcMedia;
type FnMediaNewLocation = unsafe extern "C" fn(
    p_instance: *mut LibVlcInstance,
    psz_mrl: *const c_char,
) -> *mut LibVlcMedia;
type FnMediaRelease = unsafe extern "C" fn(p_md: *mut LibVlcMedia);
type FnMediaAddOption = unsafe extern "C" fn(p_md: *mut LibVlcMedia, psz_options: *const c_char);
type FnMediaPlayerSetMedia =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, p_md: *mut LibVlcMedia);

type FnMediaPlayerPlay = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnMediaPlayerPause = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer);
type FnMediaPlayerSetPause = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, do_pause: c_int);
type FnMediaPlayerStop = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer);
type FnMediaPlayerIsPlaying = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnMediaPlayerGetState = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnMediaPlayerGetLength = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> i64;
type FnMediaPlayerGetTime = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> i64;
type FnMediaPlayerSetTime = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_time: i64);
type FnMediaPlayerGetPosition = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_float;
type FnMediaPlayerSetPosition = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, f_pos: c_float);
type FnMediaPlayerSetRate =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, rate: c_float) -> c_int;
type FnMediaPlayerGetRate = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_float;
type FnMediaPlayerNextFrame = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer);

type FnAudioGetVolume = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnAudioSetVolume =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_volume: c_int) -> c_int;
type FnAudioGetMute = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnAudioSetMute = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, status: c_int);
type FnAudioToggleMute = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer);
type FnAudioGetTrackCount = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnAudioGetTrackDescription =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> *mut LibVlcTrackDescription;
type FnAudioGetTrack = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnAudioSetTrack = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_track: c_int) -> c_int;
type FnAudioGetDelay = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> i64;
type FnAudioSetDelay = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_delay: i64) -> c_int;

type FnVideoGetSpuCount = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnVideoGetSpuDescription =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> *mut LibVlcTrackDescription;
type FnVideoGetSpu = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> c_int;
type FnVideoSetSpu = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_spu: c_int) -> c_int;
type FnVideoGetSpuDelay = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> i64;
type FnVideoSetSpuDelay = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, i_delay: i64) -> c_int;
type FnVideoSetSubtitleFile =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, psz_subtitle: *const c_char) -> c_int;
type FnTrackDescriptionListRelease = unsafe extern "C" fn(p_tdl: *mut LibVlcTrackDescription);

type FnMediaPlayerSetXwindow = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, drawable: u32);
type FnMediaPlayerSetHwnd =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, drawable: *mut c_void);
type FnMediaPlayerSetNsobject =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, drawable: *mut c_void);
type FnVideoSetAspectRatio =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, psz_aspect: *const c_char);
type FnVideoSetCropGeometry =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, psz_geometry: *const c_char);
type FnVideoTakeSnapshot = unsafe extern "C" fn(
    p_mi: *mut LibVlcMediaPlayer,
    num: c_int,
    psz_filepath: *const c_char,
    i_width: c_int,
    i_height: c_int,
) -> c_int;

type FnVideoFormatCb = unsafe extern "C" fn(
    opaque: *mut *mut c_void,
    chroma: *mut c_char,
    width: *mut c_uint,
    height: *mut c_uint,
    pitches: *mut c_uint,
    lines: *mut c_uint,
) -> c_uint;
type FnVideoCleanupCb = unsafe extern "C" fn(opaque: *mut c_void);
type FnVideoLockCb =
    unsafe extern "C" fn(opaque: *mut c_void, planes: *mut *mut c_void) -> *mut c_void;
type FnVideoUnlockCb =
    unsafe extern "C" fn(opaque: *mut c_void, picture: *mut c_void, planes: *const *mut c_void);
type FnVideoDisplayCb = unsafe extern "C" fn(opaque: *mut c_void, picture: *mut c_void);
type FnVideoSetCallbacks = unsafe extern "C" fn(
    mp: *mut LibVlcMediaPlayer,
    lock: FnVideoLockCb,
    unlock: FnVideoUnlockCb,
    display: FnVideoDisplayCb,
    opaque: *mut c_void,
);
type FnVideoSetFormatCallbacks = unsafe extern "C" fn(
    mp: *mut LibVlcMediaPlayer,
    setup: FnVideoFormatCb,
    cleanup: FnVideoCleanupCb,
);

type FnAudioOutputDeviceEnum =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> *mut LibVlcAudioOutputDevice;
type FnAudioOutputDeviceListRelease = unsafe extern "C" fn(p_list: *mut LibVlcAudioOutputDevice);
type FnAudioOutputDeviceSet = unsafe extern "C" fn(
    p_mi: *mut LibVlcMediaPlayer,
    psz_audio_output: *const c_char,
    psz_device_id: *const c_char,
);

pub const ADJUST_ENABLE: c_uint = 0;
pub const ADJUST_CONTRAST: c_uint = 1;
pub const ADJUST_BRIGHTNESS: c_uint = 2;
pub const ADJUST_HUE: c_uint = 3;
pub const ADJUST_SATURATION: c_uint = 4;
pub const ADJUST_GAMMA: c_uint = 5;

type FnVideoSetAdjustInt =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, option: c_uint, value: c_int);
type FnVideoSetAdjustFloat =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, option: c_uint, value: c_float);
type FnAudioEqualizerGetPresetCount = unsafe extern "C" fn() -> c_uint;
type FnAudioEqualizerGetPresetName = unsafe extern "C" fn(u_index: c_uint) -> *const c_char;
type FnAudioEqualizerNew = unsafe extern "C" fn() -> *mut c_void;
type FnAudioEqualizerNewFromPreset = unsafe extern "C" fn(u_index: c_uint) -> *mut c_void;
type FnAudioEqualizerRelease = unsafe extern "C" fn(p_equalizer: *mut c_void);
type FnAudioEqualizerSetPreamp =
    unsafe extern "C" fn(p_equalizer: *mut c_void, f_preamp: c_float) -> c_int;
type FnAudioEqualizerGetPreamp = unsafe extern "C" fn(p_equalizer: *mut c_void) -> c_float;
type FnAudioEqualizerSetAmpAtIndex =
    unsafe extern "C" fn(p_equalizer: *mut c_void, f_amp: c_float, u_band: c_uint) -> c_int;
type FnAudioEqualizerGetAmpAtIndex =
    unsafe extern "C" fn(p_equalizer: *mut c_void, u_band: c_uint) -> c_float;
type FnMediaPlayerSetEqualizer =
    unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer, p_equalizer: *mut c_void) -> c_int;
type FnMediaPlayerGetMedia = unsafe extern "C" fn(p_mi: *mut LibVlcMediaPlayer) -> *mut LibVlcMedia;
type FnMediaGetMeta = unsafe extern "C" fn(p_md: *mut LibVlcMedia, e_meta: c_int) -> *mut c_char;
type FnFree = unsafe extern "C" fn(ptr: *mut c_void);

#[allow(dead_code)]
struct LibVlcSymbols {
    _companion_libs: Vec<libloading::Library>,
    _lib: libloading::Library,
    libvlc_new: FnLibvlcNew,
    libvlc_release: FnLibvlcRelease,
    libvlc_get_version: FnLibvlcGetVersion,
    libvlc_media_player_new: FnMediaPlayerNew,
    libvlc_media_player_release: FnMediaPlayerRelease,
    libvlc_media_new_path: FnMediaNewPath,
    libvlc_media_new_location: FnMediaNewLocation,
    libvlc_media_release: FnMediaRelease,
    libvlc_media_add_option: FnMediaAddOption,
    libvlc_media_player_set_media: FnMediaPlayerSetMedia,
    libvlc_media_player_play: FnMediaPlayerPlay,
    libvlc_media_player_pause: FnMediaPlayerPause,
    libvlc_media_player_set_pause: FnMediaPlayerSetPause,
    libvlc_media_player_stop: FnMediaPlayerStop,
    libvlc_media_player_is_playing: FnMediaPlayerIsPlaying,
    libvlc_media_player_get_state: FnMediaPlayerGetState,
    libvlc_media_player_get_length: FnMediaPlayerGetLength,
    libvlc_media_player_get_time: FnMediaPlayerGetTime,
    libvlc_media_player_set_time: FnMediaPlayerSetTime,
    libvlc_media_player_get_position: FnMediaPlayerGetPosition,
    libvlc_media_player_set_position: FnMediaPlayerSetPosition,
    libvlc_media_player_set_rate: FnMediaPlayerSetRate,
    libvlc_media_player_get_rate: FnMediaPlayerGetRate,
    libvlc_media_player_next_frame: FnMediaPlayerNextFrame,
    libvlc_audio_get_volume: FnAudioGetVolume,
    libvlc_audio_set_volume: FnAudioSetVolume,
    libvlc_audio_get_mute: FnAudioGetMute,
    libvlc_audio_set_mute: FnAudioSetMute,
    libvlc_audio_toggle_mute: FnAudioToggleMute,
    libvlc_audio_get_track_count: FnAudioGetTrackCount,
    libvlc_audio_get_track_description: FnAudioGetTrackDescription,
    libvlc_audio_get_track: FnAudioGetTrack,
    libvlc_audio_set_track: FnAudioSetTrack,
    libvlc_audio_get_delay: FnAudioGetDelay,
    libvlc_audio_set_delay: FnAudioSetDelay,
    libvlc_video_get_spu_count: FnVideoGetSpuCount,
    libvlc_video_get_spu_description: FnVideoGetSpuDescription,
    libvlc_video_get_spu: FnVideoGetSpu,
    libvlc_video_set_spu: FnVideoSetSpu,
    libvlc_video_get_spu_delay: FnVideoGetSpuDelay,
    libvlc_video_set_spu_delay: FnVideoSetSpuDelay,
    libvlc_video_set_subtitle_file: FnVideoSetSubtitleFile,
    libvlc_track_description_list_release: FnTrackDescriptionListRelease,
    libvlc_media_player_set_xwindow: Option<FnMediaPlayerSetXwindow>,
    libvlc_media_player_set_hwnd: Option<FnMediaPlayerSetHwnd>,
    libvlc_media_player_set_nsobject: Option<FnMediaPlayerSetNsobject>,
    libvlc_video_set_aspect_ratio: FnVideoSetAspectRatio,
    libvlc_video_set_crop_geometry: FnVideoSetCropGeometry,
    libvlc_video_take_snapshot: FnVideoTakeSnapshot,
    libvlc_audio_output_device_enum: Option<FnAudioOutputDeviceEnum>,
    libvlc_audio_output_device_list_release: Option<FnAudioOutputDeviceListRelease>,
    libvlc_audio_output_device_set: Option<FnAudioOutputDeviceSet>,
    libvlc_video_set_adjust_int: Option<FnVideoSetAdjustInt>,
    libvlc_video_set_adjust_float: Option<FnVideoSetAdjustFloat>,
    libvlc_audio_equalizer_get_preset_count: Option<FnAudioEqualizerGetPresetCount>,
    libvlc_audio_equalizer_get_preset_name: Option<FnAudioEqualizerGetPresetName>,
    libvlc_audio_equalizer_new: Option<FnAudioEqualizerNew>,
    libvlc_audio_equalizer_new_from_preset: Option<FnAudioEqualizerNewFromPreset>,
    libvlc_audio_equalizer_release: Option<FnAudioEqualizerRelease>,
    libvlc_audio_equalizer_set_preamp: Option<FnAudioEqualizerSetPreamp>,
    libvlc_audio_equalizer_get_preamp: Option<FnAudioEqualizerGetPreamp>,
    libvlc_audio_equalizer_set_amp_at_index: Option<FnAudioEqualizerSetAmpAtIndex>,
    libvlc_audio_equalizer_get_amp_at_index: Option<FnAudioEqualizerGetAmpAtIndex>,
    libvlc_media_player_set_equalizer: Option<FnMediaPlayerSetEqualizer>,
    pub libvlc_video_set_callbacks: Option<FnVideoSetCallbacks>,
    pub libvlc_video_set_format_callbacks: Option<FnVideoSetFormatCallbacks>,
    libvlc_media_player_get_media: Option<FnMediaPlayerGetMedia>,
    libvlc_media_get_meta: Option<FnMediaGetMeta>,
    libvlc_free: Option<FnFree>,
    ext: ExtSymbols,
}

/// Preload companion shared libraries (such as `libvlccore.dll` on Windows or `libvlccore.so.9` on Linux)
/// and configure `VLC_PLUGIN_PATH` automatically when loading from a standalone or system VLC directory.
fn prepare_vlc_environment(lib_path: &Path) -> Vec<libloading::Library> {
    #[allow(unused_mut)] // only Linux/Windows preload companion libraries
    let mut companions = Vec::new();
    let Some(parent) = lib_path.parent() else {
        return companions;
    };
    if parent.as_os_str().is_empty() {
        return companions;
    }

    // 1. Configure VLC_PLUGIN_PATH if not already set
    if std::env::var_os("VLC_PLUGIN_PATH").is_none() {
        let plugin_candidates = [parent.join("plugins"), parent.join("vlc").join("plugins")];
        for cand in &plugin_candidates {
            if cand.is_dir() {
                std::env::set_var("VLC_PLUGIN_PATH", cand);
                break;
            }
        }
    }

    // 2. On Windows 10/11 x64, ensure parent directory is in PATH so dependent DLLs resolve,
    //    and preload libvlccore.dll before loading libvlc.dll.
    #[cfg(target_os = "windows")]
    {
        if let Ok(current_path) = std::env::var("PATH") {
            let parent_str = parent.to_string_lossy();
            if !current_path.contains(parent_str.as_ref()) {
                std::env::set_var("PATH", format!("{};{}", parent_str, current_path));
            }
        }
        let vlccore = parent.join("libvlccore.dll");
        if vlccore.exists() {
            if let Ok(lib) = unsafe { libloading::Library::new(&vlccore) } {
                companions.push(lib);
            }
        }
    }

    // 3. On Linux, preload companion libraries (e.g. libidn.so.12, libvlccore.so.9) with RTLD_GLOBAL
    #[cfg(target_os = "linux")]
    {
        let preload_names = [
            "libidn.so.12",
            "libvlccore.so.9",
            "vlc/libvlc_pulse.so.0",
            "vlc/libvlc_vdpau.so.0",
            "vlc/libvlc_xcb_events.so.0",
        ];
        for name in &preload_names {
            let candidate = parent.join(name);
            if candidate.exists() {
                // RTLD_NOW (2) | RTLD_GLOBAL (0x100) so dependent symbols/SONAMEs resolve in process
                if let Ok(unix_lib) = unsafe {
                    libloading::os::unix::Library::open(Some(&candidate), 0x00002 | 0x00100)
                } {
                    companions.push(libloading::Library::from(unix_lib));
                }
            }
        }
    }

    companions
}

impl LibVlcSymbols {
    pub unsafe fn load(lib_path: &Path) -> Result<Self, EngineError> {
        let companions = prepare_vlc_environment(lib_path);

        let lib = libloading::Library::new(lib_path)
            .map_err(|e| EngineError::LibraryLoadError(format!("{}: {}", lib_path.display(), e)))?;

        macro_rules! sym {
            ($name:ident) => {
                *lib.get(stringify!($name).as_bytes()).map_err(|e| {
                    EngineError::SymbolError(stringify!($name).to_string(), e.to_string())
                })?
            };
        }
        macro_rules! opt_sym {
            ($name:ident) => {
                lib.get(stringify!($name).as_bytes()).ok().map(|s| *s)
            };
        }

        Ok(Self {
            libvlc_new: sym!(libvlc_new),
            libvlc_release: sym!(libvlc_release),
            libvlc_get_version: sym!(libvlc_get_version),
            libvlc_media_player_new: sym!(libvlc_media_player_new),
            libvlc_media_player_release: sym!(libvlc_media_player_release),
            libvlc_media_new_path: sym!(libvlc_media_new_path),
            libvlc_media_new_location: sym!(libvlc_media_new_location),
            libvlc_media_release: sym!(libvlc_media_release),
            libvlc_media_add_option: sym!(libvlc_media_add_option),
            libvlc_media_player_set_media: sym!(libvlc_media_player_set_media),
            libvlc_media_player_play: sym!(libvlc_media_player_play),
            libvlc_media_player_pause: sym!(libvlc_media_player_pause),
            libvlc_media_player_set_pause: sym!(libvlc_media_player_set_pause),
            libvlc_media_player_stop: sym!(libvlc_media_player_stop),
            libvlc_media_player_is_playing: sym!(libvlc_media_player_is_playing),
            libvlc_media_player_get_state: sym!(libvlc_media_player_get_state),
            libvlc_media_player_get_length: sym!(libvlc_media_player_get_length),
            libvlc_media_player_get_time: sym!(libvlc_media_player_get_time),
            libvlc_media_player_set_time: sym!(libvlc_media_player_set_time),
            libvlc_media_player_get_position: sym!(libvlc_media_player_get_position),
            libvlc_media_player_set_position: sym!(libvlc_media_player_set_position),
            libvlc_media_player_set_rate: sym!(libvlc_media_player_set_rate),
            libvlc_media_player_get_rate: sym!(libvlc_media_player_get_rate),
            libvlc_media_player_next_frame: sym!(libvlc_media_player_next_frame),
            libvlc_audio_get_volume: sym!(libvlc_audio_get_volume),
            libvlc_audio_set_volume: sym!(libvlc_audio_set_volume),
            libvlc_audio_get_mute: sym!(libvlc_audio_get_mute),
            libvlc_audio_set_mute: sym!(libvlc_audio_set_mute),
            libvlc_audio_toggle_mute: sym!(libvlc_audio_toggle_mute),
            libvlc_audio_get_track_count: sym!(libvlc_audio_get_track_count),
            libvlc_audio_get_track_description: sym!(libvlc_audio_get_track_description),
            libvlc_audio_get_track: sym!(libvlc_audio_get_track),
            libvlc_audio_set_track: sym!(libvlc_audio_set_track),
            libvlc_audio_get_delay: sym!(libvlc_audio_get_delay),
            libvlc_audio_set_delay: sym!(libvlc_audio_set_delay),
            libvlc_video_get_spu_count: sym!(libvlc_video_get_spu_count),
            libvlc_video_get_spu_description: sym!(libvlc_video_get_spu_description),
            libvlc_video_get_spu: sym!(libvlc_video_get_spu),
            libvlc_video_set_spu: sym!(libvlc_video_set_spu),
            libvlc_video_get_spu_delay: sym!(libvlc_video_get_spu_delay),
            libvlc_video_set_spu_delay: sym!(libvlc_video_set_spu_delay),
            libvlc_video_set_subtitle_file: sym!(libvlc_video_set_subtitle_file),
            libvlc_track_description_list_release: sym!(libvlc_track_description_list_release),
            libvlc_media_player_set_xwindow: opt_sym!(libvlc_media_player_set_xwindow),
            libvlc_media_player_set_hwnd: opt_sym!(libvlc_media_player_set_hwnd),
            libvlc_media_player_set_nsobject: opt_sym!(libvlc_media_player_set_nsobject),
            libvlc_video_set_aspect_ratio: sym!(libvlc_video_set_aspect_ratio),
            libvlc_video_set_crop_geometry: sym!(libvlc_video_set_crop_geometry),
            libvlc_video_take_snapshot: sym!(libvlc_video_take_snapshot),
            libvlc_audio_output_device_enum: opt_sym!(libvlc_audio_output_device_enum),
            libvlc_audio_output_device_list_release: opt_sym!(
                libvlc_audio_output_device_list_release
            ),
            libvlc_audio_output_device_set: opt_sym!(libvlc_audio_output_device_set),
            libvlc_video_set_adjust_int: opt_sym!(libvlc_video_set_adjust_int),
            libvlc_video_set_adjust_float: opt_sym!(libvlc_video_set_adjust_float),
            libvlc_audio_equalizer_get_preset_count: opt_sym!(
                libvlc_audio_equalizer_get_preset_count
            ),
            libvlc_audio_equalizer_get_preset_name: opt_sym!(
                libvlc_audio_equalizer_get_preset_name
            ),
            libvlc_audio_equalizer_new: opt_sym!(libvlc_audio_equalizer_new),
            libvlc_audio_equalizer_new_from_preset: opt_sym!(
                libvlc_audio_equalizer_new_from_preset
            ),
            libvlc_audio_equalizer_release: opt_sym!(libvlc_audio_equalizer_release),
            libvlc_audio_equalizer_set_preamp: opt_sym!(libvlc_audio_equalizer_set_preamp),
            libvlc_audio_equalizer_get_preamp: opt_sym!(libvlc_audio_equalizer_get_preamp),
            libvlc_audio_equalizer_set_amp_at_index: opt_sym!(
                libvlc_audio_equalizer_set_amp_at_index
            ),
            libvlc_audio_equalizer_get_amp_at_index: opt_sym!(
                libvlc_audio_equalizer_get_amp_at_index
            ),
            libvlc_media_player_set_equalizer: opt_sym!(libvlc_media_player_set_equalizer),
            libvlc_video_set_callbacks: opt_sym!(libvlc_video_set_callbacks),
            libvlc_video_set_format_callbacks: opt_sym!(libvlc_video_set_format_callbacks),
            libvlc_media_player_get_media: opt_sym!(libvlc_media_player_get_media),
            libvlc_media_get_meta: opt_sym!(libvlc_media_get_meta),
            libvlc_free: opt_sym!(libvlc_free),
            ext: ExtSymbols {
                add_intf: opt_sym!(libvlc_add_intf),
                audio_set_channel: opt_sym!(libvlc_audio_set_channel),
                video_set_deinterlace: opt_sym!(libvlc_video_set_deinterlace),
                player_event_manager: opt_sym!(libvlc_media_player_event_manager),
                event_attach: opt_sym!(libvlc_event_attach),
                rd_new: opt_sym!(libvlc_renderer_discoverer_new),
                rd_start: opt_sym!(libvlc_renderer_discoverer_start),
                rd_stop: opt_sym!(libvlc_renderer_discoverer_stop),
                rd_release: opt_sym!(libvlc_renderer_discoverer_release),
                rd_event_manager: opt_sym!(libvlc_renderer_discoverer_event_manager),
                ri_hold: opt_sym!(libvlc_renderer_item_hold),
                ri_release: opt_sym!(libvlc_renderer_item_release),
                ri_name: opt_sym!(libvlc_renderer_item_name),
                ri_type: opt_sym!(libvlc_renderer_item_type),
                player_set_renderer: opt_sym!(libvlc_media_player_set_renderer),
                dialog_set_callbacks: opt_sym!(libvlc_dialog_set_callbacks),
                dialog_post_login: opt_sym!(libvlc_dialog_post_login),
                dialog_post_action: opt_sym!(libvlc_dialog_post_action),
                dialog_dismiss: opt_sym!(libvlc_dialog_dismiss),
            },
            _companion_libs: companions,
            _lib: lib,
        })
    }
}

// ============================================================================
// HIGH-LEVEL SAFE ENGINE & PLAYER WRAPPERS
// ============================================================================

/// Locate libVLC on Linux, Windows 10/11 x64, and macOS. The search touches
/// the disk (and the registry on Windows), so it only runs once.
pub fn find_libvlc() -> Option<PathBuf> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND.get_or_init(locate_libvlc).clone()
}

fn locate_libvlc() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("VLC_LIB_PATH") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Some(p);
        } else if p.is_dir() {
            for name in ["libvlc.dll", "libvlc.so.5", "libvlc.so", "libvlc.dylib"] {
                let candidate = p.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    // Check alongside current executable (portable / bundled layout)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for rel in [
                "libvlc.dll",
                "vlc/libvlc.dll",
                "libvlc.so.5",
                "lib/libvlc.so.5",
                "../lib/libvlc.so.5",
            ] {
                let candidate = exe_dir.join(rel);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut candidates = vec![
            PathBuf::from("/usr/lib/x86_64-linux-gnu/libvlc.so.5"),
            PathBuf::from("/usr/lib64/libvlc.so.5"),
            PathBuf::from("/lib64/libvlc.so.5"),
            PathBuf::from("/usr/lib/libvlc.so.5"),
            PathBuf::from("/snap/vlc/current/usr/lib/libvlc.so.5"),
            PathBuf::from(
                "/var/lib/flatpak/app/org.videolan.VLC/current/active/files/lib/libvlc.so.5",
            ),
        ];
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            candidates.push(home.join(".local/lib/vlc/usr/lib/x86_64-linux-gnu/libvlc.so.5"));
            candidates.push(home.join(".local/lib/libvlc.so.5"));
        }
        for p in candidates {
            if p.exists() {
                return Some(p);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates = vec![
            PathBuf::from(r"C:\Program Files\VideoLAN\VLC\libvlc.dll"),
            PathBuf::from(r"C:\Program Files (x86)\VideoLAN\VLC\libvlc.dll"),
            PathBuf::from(r"C:\tools\vlc\libvlc.dll"),
        ];
        for env_key in [
            "ProgramW6432",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
            "APPDATA",
        ] {
            if let Some(dir) = std::env::var_os(env_key).map(PathBuf::from) {
                candidates.push(dir.join("VideoLAN").join("VLC").join("libvlc.dll"));
                candidates.push(
                    dir.join("Programs")
                        .join("VideoLAN")
                        .join("VLC")
                        .join("libvlc.dll"),
                );
                candidates.push(dir.join("Programs").join("Ondrane").join("libvlc.dll"));
            }
        }
        if let Some(user) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            candidates.push(user.join("ondrane_test").join("libvlc.dll"));
            candidates.push(user.join("Downloads").join("libvlc.dll"));
            candidates.push(user.join("Downloads").join("vlc").join("libvlc.dll"));
            candidates.push(
                user.join("scoop")
                    .join("apps")
                    .join("vlc")
                    .join("current")
                    .join("libvlc.dll"),
            );
        }
        for p in &candidates {
            if p.exists() {
                return Some(p.clone());
            }
        }
        // Custom VLC installation directory, read from the registry in-process
        // (spawning `reg.exe` costs hundreds of milliseconds per call).
        candidates.clear();
        for (hive, key) in [
            (crate::system::registry::HKLM, r"SOFTWARE\VideoLAN\VLC"),
            (crate::system::registry::HKCU, r"Software\VideoLAN\VLC"),
        ] {
            if let Some(dir) = crate::system::registry::get_string(hive, key, "InstallDir") {
                if !dir.is_empty() {
                    candidates.push(PathBuf::from(dir).join("libvlc.dll"));
                }
            }
        }
        for p in candidates {
            if p.exists() {
                return Some(p);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/VLC.app/Contents/MacOS/lib/libvlc.dylib",
            "libvlc.dylib",
        ];
        for c in candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Normalize a filesystem path for libVLC (especially stripping `\\?\` on Windows 10/11).
fn normalize_media_path(target: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let stripped = target.strip_prefix(r"\\?\").unwrap_or(target);
        stripped.replace('/', r"\")
    }
    #[cfg(not(target_os = "windows"))]
    {
        target.to_string()
    }
}

/// Check if PulseAudio / PipeWire server socket is reachable on Linux.
#[cfg(target_os = "linux")]
fn has_pulse_server() -> bool {
    if std::env::var_os("PULSE_SERVER").is_some() {
        return true;
    }
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        if xdg.join("pulse").join("native").exists() || xdg.join("pipewire-0").exists() {
            return true;
        }
    }
    false
}

/// The main VLC engine context, managing the shared native library and instance.
pub struct VlcEngine {
    symbols: Arc<LibVlcSymbols>,
    instance: *mut LibVlcInstance,
    inst: Arc<Instance>,
    /// False for headless instances: their players never touch the audio
    /// output, whose volume/mute is shared by the whole process on Windows
    /// (one WASAPI session), so muting a thumbnail player muted playback.
    audio: bool,
}

/// Owns the `libvlc_instance_t`; shared by the engine and every player,
/// renderer discoverer and dialog handler created from it so the instance is
/// released only after all of them.
struct Instance {
    symbols: Arc<LibVlcSymbols>,
    ptr: *mut LibVlcInstance,
    dialog_data: std::sync::Mutex<Option<*mut DialogHandler>>,
}

unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            (self.symbols.libvlc_release)(self.ptr);
            if let Ok(mut d) = self.dialog_data.lock() {
                if let Some(raw) = d.take() {
                    drop(Box::from_raw(raw));
                }
            }
        }
    }
}

unsafe impl Send for VlcEngine {}
unsafe impl Sync for VlcEngine {}

impl VlcEngine {
    /// Load libVLC from standard system search paths and initialize the instance.
    pub fn load() -> Result<Self, EngineError> {
        let path = find_libvlc().ok_or_else(|| {
            EngineError::LibraryLoadError(
                "libVLC was not found on standard system paths".to_string(),
            )
        })?;
        Self::load_with_path(&path)
    }

    /// Load libVLC from a specific dynamic library file path.
    pub fn load_with_path(path: &Path) -> Result<Self, EngineError> {
        Self::load_with_args(path, &[])
    }

    /// Load libVLC with extra command-line options (`--audio-filter=…`,
    /// `--freetype-color=…`, …) derived from the user's settings. If the
    /// options are rejected the instance is created without them.
    pub fn load_with_args(path: &Path, extra: &[String]) -> Result<Self, EngineError> {
        let symbols = unsafe { LibVlcSymbols::load(path)? };
        let symbols = Arc::new(symbols);

        // Core arguments for safe, clean media playback
        #[allow(unused_mut)]
        let mut raw_args = vec![
            CString::new("--no-video-title-show")?,
            CString::new("--no-osd")?,
            CString::new("--no-snapshot-preview")?,
        ];

        // Prevent PipeWire audio server deadlock / buffer underflow crashes on Linux when Pulse/PipeWire is running
        #[cfg(target_os = "linux")]
        {
            if has_pulse_server() {
                raw_args.push(CString::new("--aout=pulse")?);
            }
        }

        let base_len = raw_args.len();
        for a in extra {
            if let Ok(c) = CString::new(a.as_str()) {
                raw_args.push(c);
            }
        }
        let c_ptrs: Vec<*const c_char> = raw_args.iter().map(|s| s.as_ptr()).collect();
        let mut instance = unsafe { (symbols.libvlc_new)(c_ptrs.len() as c_int, c_ptrs.as_ptr()) };
        if instance.is_null() && raw_args.len() > base_len {
            let c_ptrs: Vec<*const c_char> =
                raw_args[..base_len].iter().map(|s| s.as_ptr()).collect();
            instance = unsafe { (symbols.libvlc_new)(c_ptrs.len() as c_int, c_ptrs.as_ptr()) };
        }

        // Fallback with minimal arguments if hardware/audio flags were rejected by a stripped plugin set
        if instance.is_null() {
            let fallback_args = [CString::new("--no-osd")?];
            let fallback_ptrs: Vec<*const c_char> =
                fallback_args.iter().map(|s| s.as_ptr()).collect();
            instance = unsafe {
                (symbols.libvlc_new)(fallback_ptrs.len() as c_int, fallback_ptrs.as_ptr())
            };
        }

        if instance.is_null() {
            return Err(EngineError::InstanceInitError);
        }

        Ok(Self::wrap(symbols, instance))
    }

    fn wrap(symbols: Arc<LibVlcSymbols>, instance: *mut LibVlcInstance) -> Self {
        let inst = Arc::new(Instance {
            symbols: symbols.clone(),
            ptr: instance,
            dialog_data: std::sync::Mutex::new(None),
        });
        Self {
            symbols,
            instance,
            inst,
            audio: true,
        }
    }

    /// Load a silent, headless libVLC instance for background thumbnail extraction.
    pub fn load_headless() -> Result<Self, EngineError> {
        let path = find_libvlc().ok_or_else(|| {
            EngineError::LibraryLoadError(
                "libVLC was not found on standard system paths".to_string(),
            )
        })?;
        let symbols = unsafe { LibVlcSymbols::load(&path)? };
        let symbols = Arc::new(symbols);

        let raw_args = [
            CString::new("--no-video-title-show")?,
            CString::new("--no-osd")?,
            CString::new("--no-audio")?,
            CString::new("--no-spu")?,
            CString::new("--no-stats")?,
            CString::new("--avcodec-hw=none")?,
        ];
        let c_ptrs: Vec<*const c_char> = raw_args.iter().map(|s| s.as_ptr()).collect();
        let instance = unsafe { (symbols.libvlc_new)(c_ptrs.len() as c_int, c_ptrs.as_ptr()) };
        let mut engine = if instance.is_null() {
            Self::load_with_path(&path)?
        } else {
            Self::wrap(symbols, instance)
        };
        engine.audio = false;
        Ok(engine)
    }
}

/// Receives (width, height, RGBA pixels) for every decoded video frame.
pub type FrameCallback = Arc<dyn Fn(u32, u32, &[u8]) + Send + Sync + 'static>;

pub struct VideoBufferState {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub buffer: Vec<u8>,
    decoding: Vec<u8>,
    pub callback: Option<FrameCallback>,
}

unsafe extern "C" fn video_format_cb(
    opaque: *mut *mut c_void,
    chroma: *mut c_char,
    width: *mut c_uint,
    height: *mut c_uint,
    pitches: *mut c_uint,
    lines: *mut c_uint,
) -> c_uint {
    if opaque.is_null()
        || (*opaque).is_null()
        || chroma.is_null()
        || width.is_null()
        || height.is_null()
        || pitches.is_null()
        || lines.is_null()
    {
        return 0;
    }
    // Memory callbacks require CPU pixels. Reject opaque hardware surfaces
    // before VLC builds recursive converters while probing decoder formats.
    let input_chroma = std::slice::from_raw_parts(chroma.cast::<u8>(), 4);
    if matches!(
        input_chroma,
        b"VAOP" | b"VAO0" | b"VDV0" | b"VDV2" | b"VDV4" | b"VDOR"
    ) {
        return 0;
    }
    let w = *width;
    let h = *height;
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return 0;
    }

    let state_mutex = &*((*opaque) as *const std::sync::Mutex<VideoBufferState>);

    let c_slice = std::slice::from_raw_parts_mut(chroma as *mut u8, 4);
    c_slice[0] = b'R';
    c_slice[1] = b'G';
    c_slice[2] = b'B';
    c_slice[3] = b'A';

    let pitch = (w * 4 + 31) & !31;
    *pitches = pitch;
    *lines = h;

    if let Ok(mut state) = state_mutex.lock() {
        let len = w as usize * h as usize * 4;
        let additional = len.saturating_sub(state.buffer.len());
        if state.buffer.try_reserve(additional).is_err() {
            return 0;
        }
        let decoding_len = pitch as usize * h as usize + 31;
        let additional = decoding_len.saturating_sub(state.decoding.len());
        if state.decoding.try_reserve(additional).is_err() {
            return 0;
        }
        state.width = w;
        state.height = h;
        state.pitch = pitch;
        state.buffer.resize(len, 0);
        state.decoding.resize(decoding_len, 0);
        1
    } else {
        0
    }
}

unsafe extern "C" fn video_cleanup_cb(_opaque: *mut c_void) {}

unsafe extern "C" fn video_lock_cb(opaque: *mut c_void, planes: *mut *mut c_void) -> *mut c_void {
    if planes.is_null() {
        return std::ptr::null_mut();
    }
    *planes = std::ptr::null_mut();
    if opaque.is_null() {
        return std::ptr::null_mut();
    }
    let state_mutex = &*(opaque as *const std::sync::Mutex<VideoBufferState>);
    if let Ok(mut state) = state_mutex.lock() {
        if !state.decoding.is_empty() {
            let ptr = state.decoding.as_mut_ptr();
            *planes = ptr.add(ptr.align_offset(32)).cast();
        }
    }
    std::ptr::null_mut()
}

unsafe extern "C" fn video_unlock_cb(
    opaque: *mut c_void,
    _picture: *mut c_void,
    _planes: *const *mut c_void,
) {
    if opaque.is_null() {
        return;
    }
    let state_mutex = &*(opaque as *const std::sync::Mutex<VideoBufferState>);
    if let Ok(mut state) = state_mutex.lock() {
        // Publish only after VLC has finished writing; snapshots and thumbnails
        // may read the published buffer while the next frame is being decoded.
        let offset = state.decoding.as_ptr().align_offset(32);
        let row = state.width as usize * 4;
        let pitch = state.pitch as usize;
        let height = state.height as usize;
        let VideoBufferState {
            buffer, decoding, ..
        } = &mut *state;
        if pitch == row {
            buffer.copy_from_slice(&decoding[offset..offset + row * height]);
        } else {
            for y in 0..height {
                buffer[y * row..(y + 1) * row]
                    .copy_from_slice(&decoding[offset + y * pitch..offset + y * pitch + row]);
            }
        }
    }
}

unsafe extern "C" fn video_display_cb(opaque: *mut c_void, _picture: *mut c_void) {
    if opaque.is_null() {
        return;
    }
    let state_mutex = &*(opaque as *const std::sync::Mutex<VideoBufferState>);
    if let Ok(state) = state_mutex.lock() {
        if let Some(ref cb) = state.callback {
            cb(state.width, state.height, &state.buffer);
        }
    }
}

impl VlcEngine {
    /// Get the native libVLC version string.
    pub fn version(&self) -> String {
        unsafe {
            let ptr = (self.symbols.libvlc_get_version)();
            if ptr.is_null() {
                "Unknown".to_string()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    /// Create a new, isolated media player instance with real-time video buffer streaming.
    pub fn create_player(&self) -> Result<VlcPlayer, EngineError> {
        self.create_player_with(true)
    }

    /// Create a player. With `render_frames` the video is decoded into memory
    /// and handed to the frame callback (drawn inside the Slint window);
    /// without it libVLC opens its own video window (or none, for transcoding).
    pub fn create_player_with(&self, render_frames: bool) -> Result<VlcPlayer, EngineError> {
        let player = unsafe { (self.symbols.libvlc_media_player_new)(self.instance) };
        if player.is_null() {
            return Err(EngineError::PlayerInitError);
        }

        let video_buffer_state = Arc::new(std::sync::Mutex::new(VideoBufferState {
            width: 0,
            height: 0,
            pitch: 0,
            buffer: Vec::new(),
            decoding: Vec::new(),
            callback: None,
        }));

        if let (true, Some(set_cb), Some(set_fmt)) = (
            render_frames,
            self.symbols.libvlc_video_set_callbacks,
            self.symbols.libvlc_video_set_format_callbacks,
        ) {
            let raw_ptr = Arc::as_ptr(&video_buffer_state) as *mut c_void;
            unsafe {
                set_fmt(player, video_format_cb, video_cleanup_cb);
                set_cb(
                    player,
                    video_lock_cb,
                    video_unlock_cb,
                    video_display_cb,
                    raw_ptr,
                );
            }
        }

        let events = Arc::new(PlayerEvents::default());
        if let (Some(em_of), Some(attach)) = (
            self.symbols.ext.player_event_manager,
            self.symbols.ext.event_attach,
        ) {
            unsafe {
                let em = em_of(player);
                if !em.is_null() {
                    let data = Arc::as_ptr(&events) as *mut c_void;
                    for ty in [EV_CORKED, EV_UNCORKED, EV_ERROR, EV_VOUT] {
                        attach(em, ty, player_event_cb, data);
                    }
                }
            }
        }

        Ok(VlcPlayer {
            symbols: Arc::clone(&self.symbols),
            engine_instance: self.instance,
            player,
            video_buffer_state,
            cached_volume: AtomicI32::new(80),
            volume_pending: AtomicBool::new(false),
            mute_pending: AtomicBool::new(false),
            cached_muted: AtomicBool::new(false),
            events,
            renders_frames: render_frames,
            frame_epoch: Arc::new(AtomicU64::new(0)),
            audio: self.audio,
            _inst: self.inst.clone(),
        })
    }
}

/// A media player instance with playback, volume, tracks, equalizer, and video buffer controls.
pub struct VlcPlayer {
    symbols: Arc<LibVlcSymbols>,
    #[allow(dead_code)]
    engine_instance: *mut LibVlcInstance,
    player: *mut LibVlcMediaPlayer,
    pub video_buffer_state: Arc<std::sync::Mutex<VideoBufferState>>,
    cached_volume: AtomicI32,
    volume_pending: AtomicBool,
    mute_pending: AtomicBool,
    cached_muted: AtomicBool,
    events: Arc<PlayerEvents>,
    renders_frames: bool,
    pub(crate) frame_epoch: Arc<AtomicU64>,
    audio: bool,
    _inst: Arc<Instance>,
}

unsafe impl Send for VlcPlayer {}
unsafe impl Sync for VlcPlayer {}

impl VlcPlayer {
    /// Register a real-time callback for RGBA video frames rendered by libVLC.
    pub fn set_video_callback<F>(&self, cb: F)
    where
        F: Fn(u32, u32, &[u8]) + Send + Sync + 'static,
    {
        if let Ok(mut state) = self.video_buffer_state.lock() {
            state.callback = Some(Arc::new(cb));
        }
    }

    /// Retrieve the most recently rendered video dimensions.
    pub fn video_dimensions(&self) -> (u32, u32) {
        if let Ok(state) = self.video_buffer_state.lock() {
            (state.width, state.height)
        } else {
            (0, 0)
        }
    }

    /// Open a media file or streaming URL (e.g. "https://...", "/path/to/video.mp4", "C:\video.mp4").
    pub fn open(&self, target: &str) -> Result<(), EngineError> {
        self.open_with_options(target, &[])
    }

    /// Open a media file or streaming URL with custom libVLC media options (e.g. `:network-caching=1000`).
    pub fn open_with_options(&self, target: &str, options: &[String]) -> Result<(), EngineError> {
        // any MRL (http://, dvd://, cdda://, v4l2://, screen://…) is a location
        let is_url = target.split_once("://").is_some_and(|(scheme, _)| {
            scheme.len() > 1
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+')
        });

        let normalized = if is_url {
            target.to_string()
        } else {
            normalize_media_path(target)
        };

        let c_target = CString::new(normalized.as_str())?;
        let media = unsafe {
            if is_url {
                (self.symbols.libvlc_media_new_location)(self.engine_instance, c_target.as_ptr())
            } else {
                (self.symbols.libvlc_media_new_path)(self.engine_instance, c_target.as_ptr())
            }
        };

        if media.is_null() {
            return Err(EngineError::MediaCreationError(target.to_string()));
        }

        unsafe {
            for opt in options {
                if let Ok(c_opt) = CString::new(opt.as_str()) {
                    (self.symbols.libvlc_media_add_option)(media, c_opt.as_ptr());
                }
            }
            // Stop the previous item first: switching media on a running
            // player reuses its video output, and with D3D11VA decoding into
            // our memory callbacks every following file of the same size
            // rendered as a flat green picture.
            (self.symbols.libvlc_media_player_stop)(self.player);
            self.frame_epoch.fetch_add(1, Ordering::SeqCst);
            (self.symbols.libvlc_media_player_set_media)(self.player, media);
            (self.symbols.libvlc_media_release)(media);
        }
        Ok(())
    }

    /// Start or resume playback and synchronize cached audio state.
    pub fn play(&self) {
        let vol = self.cached_volume.load(Ordering::SeqCst);
        let muted = self.cached_muted.load(Ordering::SeqCst);
        unsafe {
            (self.symbols.libvlc_media_player_play)(self.player);
            if self.audio {
                (self.symbols.libvlc_audio_set_volume)(self.player, vol);
                (self.symbols.libvlc_audio_set_mute)(self.player, if muted { 1 } else { 0 });
            }
        }
    }

    /// Pause playback.
    pub fn pause(&self) {
        unsafe {
            (self.symbols.libvlc_media_player_set_pause)(self.player, 1);
        }
    }

    /// Toggle play / pause state.
    pub fn toggle_play_pause(&self) {
        unsafe {
            (self.symbols.libvlc_media_player_pause)(self.player);
        }
    }

    /// Stop playback.
    pub fn stop(&self) {
        unsafe {
            (self.symbols.libvlc_media_player_stop)(self.player);
        }
        self.frame_epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Check if currently playing.
    pub fn is_playing(&self) -> bool {
        unsafe { (self.symbols.libvlc_media_player_is_playing)(self.player) != 0 }
    }

    /// Get current state.
    pub fn state(&self) -> PlaybackState {
        let raw = unsafe { (self.symbols.libvlc_media_player_get_state)(self.player) };
        PlaybackState::from(raw)
    }

    /// Get playback time in milliseconds.
    pub fn time_ms(&self) -> i64 {
        unsafe { (self.symbols.libvlc_media_player_get_time)(self.player) }
    }

    /// Set playback time in milliseconds.
    pub fn seek_ms(&self, time_ms: i64) {
        unsafe {
            (self.symbols.libvlc_media_player_set_time)(self.player, time_ms);
        }
    }

    /// Jump relative to current position in milliseconds (e.g. +10000 or -15000).
    pub fn seek_relative_ms(&self, delta_ms: i64) {
        let cur = self.time_ms();
        let len = self.length_ms();
        let target = (cur + delta_ms).max(0);
        let target = if len > 0 {
            target.min(len - 250)
        } else {
            target
        };
        self.seek_ms(target);
    }

    /// Get total media length in milliseconds.
    pub fn length_ms(&self) -> i64 {
        unsafe { (self.symbols.libvlc_media_player_get_length)(self.player) }
    }

    /// Get fractional position from 0.0 to 1.0.
    pub fn position(&self) -> f32 {
        unsafe { (self.symbols.libvlc_media_player_get_position)(self.player) }
    }

    /// Seek to fractional position from 0.0 to 1.0.
    pub fn seek_position(&self, pos: f32) {
        unsafe {
            (self.symbols.libvlc_media_player_set_position)(self.player, pos.clamp(0.0, 1.0));
        }
    }

    /// Get playback speed rate (1.0 = normal).
    pub fn rate(&self) -> f32 {
        unsafe { (self.symbols.libvlc_media_player_get_rate)(self.player) }
    }

    /// Set playback speed rate (0.25 .. 4.0).
    pub fn set_rate(&self, rate: f32) {
        unsafe {
            (self.symbols.libvlc_media_player_set_rate)(self.player, rate.clamp(0.2, 8.0));
        }
    }

    /// Advance video by one single frame.
    pub fn next_frame(&self) {
        unsafe {
            (self.symbols.libvlc_media_player_next_frame)(self.player);
        }
    }

    // ------------------------------------------------------------------------
    // Audio controls
    // ------------------------------------------------------------------------

    /// Get current volume (0 to 125, or 200).
    pub fn volume(&self) -> i32 {
        let raw = unsafe { (self.symbols.libvlc_audio_get_volume)(self.player) };
        let requested = self.cached_volume.load(Ordering::SeqCst);
        if self.volume_pending.load(Ordering::SeqCst) && raw != requested {
            return requested;
        }
        if raw >= 0 {
            self.volume_pending.store(false, Ordering::SeqCst);
            self.cached_volume.store(raw, Ordering::SeqCst);
            raw
        } else {
            self.cached_volume.load(Ordering::SeqCst)
        }
    }

    /// Set volume (0 to 125, or up to 200).
    pub fn set_volume(&self, volume: i32) {
        let clamped = volume.clamp(0, 200);
        // PulseAudio applies requests asynchronously; keep the requested value
        // until the native getter acknowledges it instead of restoring stale state.
        self.cached_volume.store(clamped, Ordering::SeqCst);
        self.volume_pending.store(true, Ordering::SeqCst);
        if !self.audio {
            return;
        }
        unsafe {
            (self.symbols.libvlc_audio_set_volume)(self.player, clamped);
        }
    }

    /// Check if audio is muted.
    pub fn is_muted(&self) -> bool {
        let raw = unsafe { (self.symbols.libvlc_audio_get_mute)(self.player) };
        let requested = self.cached_muted.load(Ordering::SeqCst);
        if self.mute_pending.load(Ordering::SeqCst) && (raw < 0 || (raw != 0) != requested) {
            return requested;
        }
        if raw >= 0 {
            self.mute_pending.store(false, Ordering::SeqCst);
            let m = raw != 0;
            self.cached_muted.store(m, Ordering::SeqCst);
            m
        } else {
            self.cached_muted.load(Ordering::SeqCst)
        }
    }

    /// Set mute state.
    pub fn set_muted(&self, muted: bool) {
        self.cached_muted.store(muted, Ordering::SeqCst);
        self.mute_pending.store(true, Ordering::SeqCst);
        if !self.audio {
            return;
        }
        unsafe {
            (self.symbols.libvlc_audio_set_mute)(self.player, if muted { 1 } else { 0 });
        }
    }

    /// Toggle mute state.
    pub fn toggle_mute(&self) {
        let next = !self.is_muted();
        self.set_muted(next);
    }

    /// Retrieve all available audio tracks.
    pub fn audio_tracks(&self) -> Vec<TrackInfo> {
        let mut list = Vec::new();
        unsafe {
            let mut head = (self.symbols.libvlc_audio_get_track_description)(self.player);
            let original_head = head;
            while !head.is_null() {
                let node = &*head;
                let name = if node.psz_name.is_null() {
                    format!("Track {}", node.i_id)
                } else {
                    CStr::from_ptr(node.psz_name).to_string_lossy().into_owned()
                };
                list.push(TrackInfo {
                    id: node.i_id,
                    name,
                });
                head = node.p_next;
            }
            if !original_head.is_null() {
                (self.symbols.libvlc_track_description_list_release)(original_head);
            }
        }
        list
    }

    /// Get current audio track ID.
    pub fn current_audio_track(&self) -> i32 {
        unsafe { (self.symbols.libvlc_audio_get_track)(self.player) }
    }

    /// Select audio track by ID.
    pub fn set_audio_track(&self, track_id: i32) -> bool {
        unsafe { (self.symbols.libvlc_audio_set_track)(self.player, track_id) == 0 }
    }

    /// Get audio synchronization delay in milliseconds.
    pub fn audio_delay_ms(&self) -> i64 {
        unsafe { (self.symbols.libvlc_audio_get_delay)(self.player) / 1000 }
    }

    /// Set audio synchronization delay in milliseconds.
    pub fn set_audio_delay_ms(&self, delay_ms: i64) {
        unsafe {
            (self.symbols.libvlc_audio_set_delay)(self.player, delay_ms * 1000);
        }
    }

    // ------------------------------------------------------------------------
    // Subtitle controls
    // ------------------------------------------------------------------------

    /// Retrieve all available subtitle tracks.
    pub fn subtitle_tracks(&self) -> Vec<TrackInfo> {
        let mut list = Vec::new();
        unsafe {
            let mut head = (self.symbols.libvlc_video_get_spu_description)(self.player);
            let original_head = head;
            while !head.is_null() {
                let node = &*head;
                let name = if node.psz_name.is_null() {
                    format!("Subtitle {}", node.i_id)
                } else {
                    CStr::from_ptr(node.psz_name).to_string_lossy().into_owned()
                };
                list.push(TrackInfo {
                    id: node.i_id,
                    name,
                });
                head = node.p_next;
            }
            if !original_head.is_null() {
                (self.symbols.libvlc_track_description_list_release)(original_head);
            }
        }
        list
    }

    /// Get current subtitle track ID (-1 = disabled).
    pub fn current_subtitle_track(&self) -> i32 {
        unsafe { (self.symbols.libvlc_video_get_spu)(self.player) }
    }

    /// Select subtitle track by ID.
    pub fn set_subtitle_track(&self, spu_id: i32) -> bool {
        unsafe { (self.symbols.libvlc_video_set_spu)(self.player, spu_id) == 0 }
    }

    /// Add an external subtitle file (.srt, .ass, .vtt).
    pub fn add_subtitle_file(&self, path: &str) -> Result<bool, EngineError> {
        let norm = normalize_media_path(path);
        let c_path = CString::new(norm)?;
        let ret =
            unsafe { (self.symbols.libvlc_video_set_subtitle_file)(self.player, c_path.as_ptr()) };
        Ok(ret != 0)
    }

    /// Get subtitle delay in milliseconds.
    pub fn subtitle_delay_ms(&self) -> i64 {
        unsafe { (self.symbols.libvlc_video_get_spu_delay)(self.player) / 1000 }
    }

    /// Set subtitle delay in milliseconds.
    pub fn set_subtitle_delay_ms(&self, delay_ms: i64) {
        unsafe {
            (self.symbols.libvlc_video_set_spu_delay)(self.player, delay_ms * 1000);
        }
    }

    // ------------------------------------------------------------------------
    // Video display & window embedding
    // ------------------------------------------------------------------------

    /// Embed video inside an X11 Window ID.
    pub fn set_xwindow(&self, xid: u32) {
        if let Some(set_x) = self.symbols.libvlc_media_player_set_xwindow {
            unsafe {
                set_x(self.player, xid);
            }
        }
    }

    /// Embed video inside a Win32 HWND.
    pub fn set_hwnd(&self, hwnd: usize) {
        if let Some(set_h) = self.symbols.libvlc_media_player_set_hwnd {
            unsafe {
                set_h(self.player, hwnd as *mut c_void);
            }
        }
    }

    /// Set video aspect ratio (e.g. "16:9", "4:3", or "" for original).
    pub fn set_aspect_ratio(&self, aspect: &str) -> Result<(), EngineError> {
        let c_aspect = if aspect.is_empty() || aspect == "default" {
            std::ptr::null()
        } else {
            CString::new(aspect)?.into_raw()
        };
        unsafe {
            (self.symbols.libvlc_video_set_aspect_ratio)(self.player, c_aspect);
            if !c_aspect.is_null() {
                let _ = CString::from_raw(c_aspect as *mut _);
            }
        }
        Ok(())
    }

    /// Take a screenshot snapshot of the video frame.
    /// Falls back to encoding the active RGBA video buffer directly if libVLC vmem snapshot fails.
    pub fn take_snapshot(
        &self,
        filepath: &Path,
        width: u32,
        height: u32,
    ) -> Result<(), EngineError> {
        // First try encoding directly from our real-time RGBA buffer (instant & reliable with vmem)
        if let Ok(state) = self.video_buffer_state.lock() {
            if state.width > 0
                && state.height > 0
                && state.buffer.len() == (state.width * state.height * 4) as usize
            {
                if let Some(mut img) =
                    image::RgbaImage::from_raw(state.width, state.height, state.buffer.clone())
                {
                    if width > 0 && width != state.width {
                        let h = if height > 0 {
                            height
                        } else {
                            (state.height as u64 * width as u64 / state.width as u64).max(1) as u32
                        };
                        img = image::imageops::resize(
                            &img,
                            width,
                            h,
                            image::imageops::FilterType::Triangle,
                        );
                    }
                    let saved = if filepath
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
                    {
                        // JPEG has no alpha channel
                        image::DynamicImage::ImageRgba8(img)
                            .to_rgb8()
                            .save(filepath)
                    } else {
                        img.save(filepath)
                    };
                    if saved.is_ok() {
                        return Ok(());
                    }
                }
            }
        }

        let norm = normalize_media_path(&filepath.to_string_lossy());
        let c_path = CString::new(norm.as_bytes())?;
        let ret = unsafe {
            (self.symbols.libvlc_video_take_snapshot)(
                self.player,
                0,
                c_path.as_ptr(),
                width as c_int,
                height as c_int,
            )
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(EngineError::SnapshotError(filepath.display().to_string()))
        }
    }

    /// Adjust video picture controls (enable, brightness 0..1, contrast 0..1, saturation 0..1, gamma 0..1).
    pub fn set_adjust(
        &self,
        enable: bool,
        brightness: f32,
        contrast: f32,
        saturation: f32,
        gamma: f32,
    ) {
        if let (Some(set_i), Some(set_f)) = (
            self.symbols.libvlc_video_set_adjust_int,
            self.symbols.libvlc_video_set_adjust_float,
        ) {
            unsafe {
                set_i(self.player, ADJUST_ENABLE, if enable { 1 } else { 0 });
                if enable {
                    set_f(self.player, ADJUST_BRIGHTNESS, brightness * 2.0);
                    set_f(self.player, ADJUST_CONTRAST, contrast * 2.0);
                    set_f(self.player, ADJUST_SATURATION, saturation * 3.0);
                    set_f(self.player, ADJUST_GAMMA, gamma * 2.0);
                }
            }
        }
    }

    /// Adjust the picture with real libVLC ranges: brightness/contrast 0..2,
    /// saturation 0..3, gamma 0.01..10 and hue in degrees (-180..180).
    pub fn set_picture(
        &self,
        enable: bool,
        brightness: f32,
        contrast: f32,
        saturation: f32,
        gamma: f32,
        hue: f32,
    ) {
        if let (Some(set_i), Some(set_f)) = (
            self.symbols.libvlc_video_set_adjust_int,
            self.symbols.libvlc_video_set_adjust_float,
        ) {
            unsafe {
                set_i(self.player, ADJUST_ENABLE, if enable { 1 } else { 0 });
                if enable {
                    set_f(self.player, ADJUST_BRIGHTNESS, brightness.clamp(0.0, 2.0));
                    set_f(self.player, ADJUST_CONTRAST, contrast.clamp(0.0, 2.0));
                    set_f(self.player, ADJUST_SATURATION, saturation.clamp(0.0, 3.0));
                    set_f(self.player, ADJUST_GAMMA, gamma.clamp(0.01, 10.0));
                    set_f(self.player, ADJUST_HUE, hue.clamp(-180.0, 180.0));
                }
            }
        }
    }

    /// Set the crop geometry (e.g. "16:9", "4:3"); an empty string removes cropping.
    pub fn set_crop(&self, geometry: &str) {
        let c = CString::new(geometry).unwrap_or_default();
        unsafe {
            (self.symbols.libvlc_video_set_crop_geometry)(
                self.player,
                if geometry.is_empty() {
                    std::ptr::null()
                } else {
                    c.as_ptr()
                },
            );
        }
    }

    /// Read a metadata field of the current media (libvlc_meta_t index:
    /// 0 = title, 1 = artist, 4 = album, 15 = artwork URL).
    pub fn media_meta(&self, field: i32) -> Option<String> {
        let (Some(get_media), Some(get_meta)) = (
            self.symbols.libvlc_media_player_get_media,
            self.symbols.libvlc_media_get_meta,
        ) else {
            return None;
        };
        unsafe {
            let media = get_media(self.player);
            if media.is_null() {
                return None;
            }
            let raw = get_meta(media, field);
            (self.symbols.libvlc_media_release)(media);
            if raw.is_null() {
                return None;
            }
            let value = CStr::from_ptr(raw).to_string_lossy().into_owned();
            if let Some(free) = self.symbols.libvlc_free {
                free(raw as *mut c_void);
            }
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        }
    }

    /// Apply an equalizer preset by index and return its normalized 10-band values (0.0 .. 1.0, where 0.5 = 0 dB).
    pub fn set_equalizer_preset(&self, preset_idx: u32) -> [f32; 10] {
        let mut bands = [0.5f32; 10];
        if let (Some(new_eq), Some(set_eq), Some(rel_eq)) = (
            self.symbols.libvlc_audio_equalizer_new_from_preset,
            self.symbols.libvlc_media_player_set_equalizer,
            self.symbols.libvlc_audio_equalizer_release,
        ) {
            unsafe {
                let eq = new_eq(preset_idx);
                if !eq.is_null() {
                    if let Some(get_amp) = self.symbols.libvlc_audio_equalizer_get_amp_at_index {
                        for (i, band) in bands.iter_mut().enumerate() {
                            let db = get_amp(eq, i as c_uint); // -20.0 .. +20.0 dB
                            *band = ((db + 20.0) / 40.0).clamp(0.0, 1.0);
                        }
                    }
                    set_eq(self.player, eq);
                    rel_eq(eq);
                }
            }
        }
        bands
    }

    /// Apply custom 10-band equalizer values (normalized 0.0 .. 1.0 -> -20dB .. +20dB).
    pub fn set_equalizer_bands(&self, bands: &[f32; 10], preamp_db: f32) {
        if let (Some(new_eq), Some(set_amp), Some(set_eq), Some(rel_eq)) = (
            self.symbols.libvlc_audio_equalizer_new,
            self.symbols.libvlc_audio_equalizer_set_amp_at_index,
            self.symbols.libvlc_media_player_set_equalizer,
            self.symbols.libvlc_audio_equalizer_release,
        ) {
            unsafe {
                let eq = new_eq();
                if !eq.is_null() {
                    if let Some(set_pre) = self.symbols.libvlc_audio_equalizer_set_preamp {
                        set_pre(eq, preamp_db.clamp(-20.0, 20.0));
                    }
                    for (i, &val) in bands.iter().enumerate() {
                        let db = (val.clamp(0.0, 1.0) * 40.0) - 20.0;
                        set_amp(eq, db, i as c_uint);
                    }
                    set_eq(self.player, eq);
                    rel_eq(eq);
                }
            }
        }
    }
}

impl Drop for VlcPlayer {
    fn drop(&mut self) {
        if !self.player.is_null() {
            if let Some(set_cb) = self.symbols.libvlc_video_set_callbacks {
                unsafe {
                    set_cb(
                        self.player,
                        video_lock_cb,
                        video_unlock_cb,
                        video_display_cb,
                        std::ptr::null_mut(),
                    );
                }
            }
            unsafe {
                (self.symbols.libvlc_media_player_stop)(self.player);
                (self.symbols.libvlc_media_player_release)(self.player);
            }
            self.player = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_libvlc() {
        // libVLC is a runtime dependency; machines without it simply skip.
        if let Some(p) = find_libvlc() {
            assert!(p.exists());
        }
    }

    #[test]
    fn test_engine_initialization_and_player() {
        if find_libvlc().is_none() {
            eprintln!("libVLC not installed: skipping engine test");
            return;
        }
        let engine = VlcEngine::load();
        assert!(
            engine.is_ok(),
            "Engine should load cleanly: {:?}",
            engine.err()
        );
        let engine = engine.unwrap();

        let ver = engine.version();
        assert!(!ver.is_empty(), "libVLC version string should not be empty");

        let player = engine.create_player();
        assert!(player.is_ok(), "Player should be created without errors");
        let player = player.unwrap();

        assert_eq!(player.state(), PlaybackState::Idle);

        player.set_volume(85);
        assert_eq!(player.volume(), 85);

        player.set_muted(true);
        assert!(player.is_muted());
        player.set_muted(false);
        assert!(!player.is_muted());
    }

    #[test]
    fn test_video_format_cb_guard_and_buffer_allocation() {
        let state = std::sync::Mutex::new(VideoBufferState {
            width: 0,
            height: 0,
            pitch: 0,
            buffer: Vec::new(),
            decoding: Vec::new(),
            callback: None,
        });
        let mut opaque_ptr = &state as *const _ as *mut c_void;
        let mut chroma = [0u8; 4];
        let mut width = 1280u32;
        let mut height = 720u32;
        let mut pitches = 0u32;
        let mut lines = 0u32;

        // 1. Valid allocation
        let res = unsafe {
            video_format_cb(
                &mut opaque_ptr,
                chroma.as_mut_ptr() as *mut c_char,
                &mut width,
                &mut height,
                &mut pitches,
                &mut lines,
            )
        };
        assert_eq!(res, 1);
        assert_eq!(&chroma, b"RGBA");
        assert_eq!(pitches, 1280 * 4);
        assert_eq!(lines, 720);
        let locked = state.lock().unwrap();
        assert_eq!(locked.width, 1280);
        assert_eq!(locked.height, 720);
        assert_eq!(locked.buffer.len(), 1280 * 720 * 4);
        drop(locked);

        // Hardware surfaces cannot be interpreted as CPU RGBA planes.
        for mut opaque_chroma in [*b"VAOP", *b"VAO0", *b"VDV0", *b"VDV2", *b"VDV4", *b"VDOR"] {
            assert_eq!(
                unsafe {
                    video_format_cb(
                        &mut opaque_ptr,
                        opaque_chroma.as_mut_ptr().cast(),
                        &mut width,
                        &mut height,
                        &mut pitches,
                        &mut lines,
                    )
                },
                0
            );
        }

        // 2. Null pointers must be safely rejected without crash
        let res_null = unsafe {
            video_format_cb(
                std::ptr::null_mut(),
                chroma.as_mut_ptr() as *mut c_char,
                &mut width,
                &mut height,
                &mut pitches,
                &mut lines,
            )
        };
        assert_eq!(res_null, 0);

        let res_null_pitches = unsafe {
            video_format_cb(
                &mut opaque_ptr,
                chroma.as_mut_ptr() as *mut c_char,
                &mut width,
                &mut height,
                std::ptr::null_mut(),
                &mut lines,
            )
        };
        assert_eq!(res_null_pitches, 0);

        // 3. Zero dimensions must be rejected
        let mut zero_w = 0u32;
        let res_zero = unsafe {
            video_format_cb(
                &mut opaque_ptr,
                chroma.as_mut_ptr() as *mut c_char,
                &mut zero_w,
                &mut height,
                &mut pitches,
                &mut lines,
            )
        };
        assert_eq!(res_zero, 0);
        for value in [16385, u32::MAX] {
            let mut width = value;
            assert_eq!(
                unsafe {
                    video_format_cb(
                        &mut opaque_ptr,
                        chroma.as_mut_ptr().cast(),
                        &mut width,
                        &mut height,
                        &mut pitches,
                        &mut lines,
                    )
                },
                0
            );
        }
    }

    #[test]
    fn video_publish_strips_aligned_row_padding() {
        let state = std::sync::Mutex::new(VideoBufferState {
            width: 0,
            height: 0,
            pitch: 0,
            buffer: Vec::new(),
            decoding: Vec::new(),
            callback: None,
        });
        let mut opaque = &state as *const _ as *mut c_void;
        let (mut width, mut height, mut pitch, mut lines) = (3, 2, 0, 0);
        let mut chroma = [0u8; 4];
        let mut plane = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                video_format_cb(
                    &mut opaque,
                    chroma.as_mut_ptr().cast(),
                    &mut width,
                    &mut height,
                    &mut pitch,
                    &mut lines
                ),
                1
            );
            assert_eq!(pitch, 32);
            video_lock_cb(opaque, &mut plane);
            std::ptr::write_bytes(plane.cast::<u8>(), 7, 12);
            std::ptr::write_bytes(plane.cast::<u8>().add(12), 99, 20);
            std::ptr::write_bytes(plane.cast::<u8>().add(32), 8, 12);
            video_unlock_cb(opaque, std::ptr::null_mut(), &plane);
        }
        let state = state.lock().unwrap();
        assert_eq!(state.buffer.len(), 24);
        assert_eq!(&state.buffer[..12], &[7; 12]);
        assert_eq!(&state.buffer[12..], &[8; 12]);
    }

    #[test]
    fn test_video_lock_cb_safety() {
        let state = std::sync::Mutex::new(VideoBufferState {
            width: 10,
            height: 10,
            pitch: 40,
            buffer: vec![0u8; 400],
            decoding: vec![0u8; 431],
            callback: None,
        });
        let opaque_ptr = &state as *const _ as *mut c_void;
        let mut planes: *mut c_void = std::ptr::null_mut();

        // 1. Valid lock sets planes pointer
        unsafe {
            video_lock_cb(opaque_ptr, &mut planes);
        }
        assert!(!planes.is_null());
        assert_eq!(planes as usize % 32, 0);
        unsafe {
            planes.cast::<u8>().write(123);
        }
        assert_eq!(state.lock().unwrap().buffer[0], 0);
        unsafe {
            video_unlock_cb(opaque_ptr, std::ptr::null_mut(), &planes);
        }
        assert_eq!(state.lock().unwrap().buffer[0], 123);
        unsafe {
            planes.cast::<u8>().write(42);
        }
        assert_eq!(state.lock().unwrap().buffer[0], 123);

        // 2. Null opaque safely nullifies planes
        planes = 0x1234 as *mut c_void;
        unsafe {
            video_lock_cb(std::ptr::null_mut(), &mut planes);
        }
        assert!(planes.is_null());

        // 3. Null planes pointer does not crash
        unsafe {
            video_lock_cb(opaque_ptr, std::ptr::null_mut());
        }
    }
}
