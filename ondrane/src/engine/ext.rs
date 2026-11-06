//! libVLC 3 features beyond basic playback: player events (cork / errors),
//! audio devices and channels, deinterlacing, renderer discovery (Chromecast
//! casting), interactive dialogs (logins, certificate questions, errors) and
//! extra interface modules.

use super::*;

pub(super) type FnEventCb = unsafe extern "C" fn(event: *const LibVlcEvent, data: *mut c_void);

#[repr(C)]
pub(super) struct LibVlcEvent {
    ty: c_int,
    obj: *mut c_void,
    /// First member of the event union (a pointer for every event we read).
    u0: *mut c_void,
}

pub(super) enum LibVlcEventManager {}
pub(super) enum LibVlcRendererDiscoverer {}
pub(super) enum LibVlcRendererItem {}
pub(super) enum LibVlcDialogId {}

type FnAddIntf = unsafe extern "C" fn(*mut LibVlcInstance, *const c_char) -> c_int;
type FnAudioSetChannel = unsafe extern "C" fn(*mut LibVlcMediaPlayer, c_int) -> c_int;
type FnVideoSetDeinterlace = unsafe extern "C" fn(*mut LibVlcMediaPlayer, *const c_char);
type FnPlayerEventManager = unsafe extern "C" fn(*mut LibVlcMediaPlayer) -> *mut LibVlcEventManager;
type FnEventAttach =
    unsafe extern "C" fn(*mut LibVlcEventManager, c_int, FnEventCb, *mut c_void) -> c_int;
type FnRdNew =
    unsafe extern "C" fn(*mut LibVlcInstance, *const c_char) -> *mut LibVlcRendererDiscoverer;
type FnRdStart = unsafe extern "C" fn(*mut LibVlcRendererDiscoverer) -> c_int;
type FnRdVoid = unsafe extern "C" fn(*mut LibVlcRendererDiscoverer);
type FnRdEventManager =
    unsafe extern "C" fn(*mut LibVlcRendererDiscoverer) -> *mut LibVlcEventManager;
type FnRiHold = unsafe extern "C" fn(*mut LibVlcRendererItem) -> *mut LibVlcRendererItem;
type FnRiRelease = unsafe extern "C" fn(*mut LibVlcRendererItem);
type FnRiStr = unsafe extern "C" fn(*const LibVlcRendererItem) -> *const c_char;
type FnPlayerSetRenderer =
    unsafe extern "C" fn(*mut LibVlcMediaPlayer, *mut LibVlcRendererItem) -> c_int;
type FnDialogSetCallbacks =
    unsafe extern "C" fn(*mut LibVlcInstance, *const DialogCbs, *mut c_void);
type FnDialogPostLogin =
    unsafe extern "C" fn(*mut LibVlcDialogId, *const c_char, *const c_char, bool) -> c_int;
type FnDialogPostAction = unsafe extern "C" fn(*mut LibVlcDialogId, c_int) -> c_int;
type FnDialogDismiss = unsafe extern "C" fn(*mut LibVlcDialogId) -> c_int;

/// Optional symbols (all present in libVLC 3.0).
pub(super) struct ExtSymbols {
    pub add_intf: Option<FnAddIntf>,
    pub audio_set_channel: Option<FnAudioSetChannel>,
    pub video_set_deinterlace: Option<FnVideoSetDeinterlace>,
    pub player_event_manager: Option<FnPlayerEventManager>,
    pub event_attach: Option<FnEventAttach>,
    pub rd_new: Option<FnRdNew>,
    pub rd_start: Option<FnRdStart>,
    pub rd_stop: Option<FnRdVoid>,
    pub rd_release: Option<FnRdVoid>,
    pub rd_event_manager: Option<FnRdEventManager>,
    pub ri_hold: Option<FnRiHold>,
    pub ri_release: Option<FnRiRelease>,
    pub ri_name: Option<FnRiStr>,
    pub ri_type: Option<FnRiStr>,
    pub player_set_renderer: Option<FnPlayerSetRenderer>,
    pub dialog_set_callbacks: Option<FnDialogSetCallbacks>,
    pub dialog_post_login: Option<FnDialogPostLogin>,
    pub dialog_post_action: Option<FnDialogPostAction>,
    pub dialog_dismiss: Option<FnDialogDismiss>,
}

// libvlc_event_e values (libvlc_events.h, 3.0)
pub(super) const EV_ERROR: c_int = 0x10a;
pub(super) const EV_VOUT: c_int = 0x112;
pub(super) const EV_CORKED: c_int = 0x117;
pub(super) const EV_UNCORKED: c_int = 0x118;
const EV_RENDERER_ADDED: c_int = 0x502;
const EV_RENDERER_DELETED: c_int = 0x503;

/// Asynchronous player notifications, latched by libVLC threads and read by
/// the UI timer.
#[derive(Default)]
pub struct PlayerEvents {
    corked: AtomicI32, // 0 nothing new, 1 corked, 2 uncorked
    error: AtomicBool,
    vout: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerEvent {
    /// Another application took the audio output (phone call, …).
    Corked,
    Uncorked,
    /// The input failed (libVLC "EncounteredError").
    Error,
    /// A video output was created (the video size is known).
    VideoStarted,
}

pub(super) unsafe extern "C" fn player_event_cb(event: *const LibVlcEvent, data: *mut c_void) {
    if event.is_null() || data.is_null() {
        return;
    }
    let ev = &*(data as *const PlayerEvents);
    match (*event).ty {
        EV_CORKED => ev.corked.store(1, Ordering::SeqCst),
        EV_UNCORKED => ev.corked.store(2, Ordering::SeqCst),
        EV_ERROR => ev.error.store(true, Ordering::SeqCst),
        EV_VOUT => ev.vout.store(true, Ordering::SeqCst),
        _ => {}
    }
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

impl VlcPlayer {
    /// Pop the next pending player event.
    pub fn take_event(&self) -> Option<PlayerEvent> {
        let e = &self.events;
        if e.error.swap(false, Ordering::SeqCst) {
            return Some(PlayerEvent::Error);
        }
        if e.vout.swap(false, Ordering::SeqCst) {
            return Some(PlayerEvent::VideoStarted);
        }
        match e.corked.swap(0, Ordering::SeqCst) {
            1 => Some(PlayerEvent::Corked),
            2 => Some(PlayerEvent::Uncorked),
            _ => None,
        }
    }

    /// True when frames are rendered into the Slint window (false when libVLC
    /// owns a separate video window).
    pub fn renders_frames(&self) -> bool {
        self.renders_frames
    }

    /// Audio output devices of the current audio output module.
    pub fn audio_devices(&self) -> Vec<AudioDevice> {
        let (Some(enum_fn), Some(release)) = (
            self.symbols.libvlc_audio_output_device_enum,
            self.symbols.libvlc_audio_output_device_list_release,
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        unsafe {
            let head = enum_fn(self.player);
            let mut cur = head;
            while !cur.is_null() {
                let d = &*cur;
                out.push(AudioDevice {
                    id: cstr(d.psz_device),
                    description: cstr(d.psz_description),
                });
                cur = d.p_next;
            }
            if !head.is_null() {
                release(head);
            }
        }
        out
    }

    /// Switch the audio output device (empty id = system default).
    pub fn set_audio_device(&self, id: &str) {
        let Some(set) = self.symbols.libvlc_audio_output_device_set else {
            return;
        };
        let Ok(c) = CString::new(id) else { return };
        unsafe { set(self.player, std::ptr::null(), c.as_ptr()) };
    }

    /// libvlc_audio_output_channel_t: 1 stereo, 2 reverse stereo, 3 left,
    /// 4 right, 5 Dolby surround.
    pub fn set_audio_channel(&self, channel: i32) {
        if let Some(f) = self.symbols.ext.audio_set_channel {
            unsafe { f(self.player, channel) };
        }
    }

    /// Enable a deinterlacing filter by name ("blend", "yadif", …) or disable it.
    pub fn set_deinterlace(&self, mode: Option<&str>) {
        let Some(f) = self.symbols.ext.video_set_deinterlace else {
            return;
        };
        match mode.and_then(|m| CString::new(m).ok()) {
            Some(c) => unsafe { f(self.player, c.as_ptr()) },
            None => unsafe { f(self.player, std::ptr::null()) },
        }
    }

    /// Send the next playback to a renderer (Chromecast) or back to this
    /// computer. Must be called before `play`.
    pub fn set_renderer(&self, item: Option<&RendererItem>) -> bool {
        let Some(f) = self.symbols.ext.player_set_renderer else {
            return false;
        };
        let raw = item.map(|i| i.ptr).unwrap_or(std::ptr::null_mut());
        unsafe { f(self.player, raw) == 0 }
    }
}

// --------------------------------------------------------------- renderers

/// A cast target found on the local network.
pub struct RendererItem {
    ptr: *mut LibVlcRendererItem,
    release: FnRiRelease,
    _inst: Arc<Instance>,
    pub name: String,
    pub kind: String,
}

unsafe impl Send for RendererItem {}
unsafe impl Sync for RendererItem {}

impl Drop for RendererItem {
    fn drop(&mut self) {
        unsafe { (self.release)(self.ptr) };
    }
}

struct DiscoveryShared {
    inst: Arc<Instance>,
    items: std::sync::Mutex<Vec<Arc<RendererItem>>>,
    changed: AtomicBool,
}

/// Running `libvlc_renderer_discoverer_t` (mDNS Chromecast discovery).
pub struct RendererDiscovery {
    rd: *mut LibVlcRendererDiscoverer,
    shared: Arc<DiscoveryShared>,
}

unsafe impl Send for RendererDiscovery {}
unsafe impl Sync for RendererDiscovery {}

unsafe extern "C" fn renderer_event_cb(event: *const LibVlcEvent, data: *mut c_void) {
    if event.is_null() || data.is_null() {
        return;
    }
    let shared = &*(data as *const DiscoveryShared);
    let item = (*event).u0 as *mut LibVlcRendererItem;
    if item.is_null() {
        return;
    }
    let x = &shared.inst.symbols.ext;
    let (Some(hold), Some(release), Some(name), Some(kind)) =
        (x.ri_hold, x.ri_release, x.ri_name, x.ri_type)
    else {
        return;
    };
    let Ok(mut items) = shared.items.lock() else {
        return;
    };
    match (*event).ty {
        EV_RENDERER_ADDED => {
            if items.iter().any(|i| i.ptr == item) {
                return;
            }
            let held = hold(item);
            items.push(Arc::new(RendererItem {
                ptr: held,
                release,
                _inst: shared.inst.clone(),
                name: cstr(name(held)),
                kind: cstr(kind(held)),
            }));
        }
        EV_RENDERER_DELETED => items.retain(|i| i.ptr != item),
        _ => return,
    }
    shared.changed.store(true, Ordering::SeqCst);
}

impl RendererDiscovery {
    /// Devices found so far.
    pub fn items(&self) -> Vec<Arc<RendererItem>> {
        self.shared
            .items
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// True once after the device list changed.
    pub fn take_changed(&self) -> bool {
        self.shared.changed.swap(false, Ordering::SeqCst)
    }
}

impl Drop for RendererDiscovery {
    fn drop(&mut self) {
        let x = &self.shared.inst.symbols.ext;
        unsafe {
            if let Some(stop) = x.rd_stop {
                stop(self.rd);
            }
            if let Some(release) = x.rd_release {
                release(self.rd);
            }
        }
        if let Ok(mut v) = self.shared.items.lock() {
            v.clear();
        }
    }
}

// ----------------------------------------------------------------- dialogs

/// Opaque id of a pending libVLC dialog, answered with
/// [`VlcEngine::dialog_login`], [`VlcEngine::dialog_action`] or
/// [`VlcEngine::dialog_dismiss`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DialogId(usize);

#[derive(Debug, Clone)]
pub enum DialogEvent {
    Error {
        title: String,
        text: String,
    },
    Login {
        id: DialogId,
        title: String,
        text: String,
        username: String,
        ask_store: bool,
    },
    Question {
        id: DialogId,
        title: String,
        text: String,
        /// 0 normal, 1 warning, 2 critical
        severity: i32,
        cancel: String,
        action1: String,
        action2: String,
    },
    Progress {
        id: DialogId,
        title: String,
        text: String,
        position: f32,
        cancel: String,
    },
    Update {
        id: DialogId,
        position: f32,
        text: String,
    },
    Cancel {
        id: DialogId,
    },
}

pub(super) type DialogHandler = Box<dyn Fn(DialogEvent) + Send + Sync>;

#[repr(C)]
pub(super) struct DialogCbs {
    display_error: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char),
    display_login: unsafe extern "C" fn(
        *mut c_void,
        *mut LibVlcDialogId,
        *const c_char,
        *const c_char,
        *const c_char,
        bool,
    ),
    display_question: unsafe extern "C" fn(
        *mut c_void,
        *mut LibVlcDialogId,
        *const c_char,
        *const c_char,
        c_int,
        *const c_char,
        *const c_char,
        *const c_char,
    ),
    display_progress: unsafe extern "C" fn(
        *mut c_void,
        *mut LibVlcDialogId,
        *const c_char,
        *const c_char,
        bool,
        c_float,
        *const c_char,
    ),
    cancel: unsafe extern "C" fn(*mut c_void, *mut LibVlcDialogId),
    update_progress: unsafe extern "C" fn(*mut c_void, *mut LibVlcDialogId, c_float, *const c_char),
}

unsafe fn emit(data: *mut c_void, ev: DialogEvent) {
    if !data.is_null() {
        (*(data as *const DialogHandler))(ev);
    }
}

unsafe extern "C" fn dlg_error(data: *mut c_void, title: *const c_char, text: *const c_char) {
    emit(
        data,
        DialogEvent::Error {
            title: cstr(title),
            text: cstr(text),
        },
    );
}

unsafe extern "C" fn dlg_login(
    data: *mut c_void,
    id: *mut LibVlcDialogId,
    title: *const c_char,
    text: *const c_char,
    user: *const c_char,
    ask_store: bool,
) {
    emit(
        data,
        DialogEvent::Login {
            id: DialogId(id as usize),
            title: cstr(title),
            text: cstr(text),
            username: cstr(user),
            ask_store,
        },
    );
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn dlg_question(
    data: *mut c_void,
    id: *mut LibVlcDialogId,
    title: *const c_char,
    text: *const c_char,
    severity: c_int,
    cancel: *const c_char,
    action1: *const c_char,
    action2: *const c_char,
) {
    emit(
        data,
        DialogEvent::Question {
            id: DialogId(id as usize),
            title: cstr(title),
            text: cstr(text),
            severity,
            cancel: cstr(cancel),
            action1: cstr(action1),
            action2: cstr(action2),
        },
    );
}

unsafe extern "C" fn dlg_progress(
    data: *mut c_void,
    id: *mut LibVlcDialogId,
    title: *const c_char,
    text: *const c_char,
    _indeterminate: bool,
    position: c_float,
    cancel: *const c_char,
) {
    emit(
        data,
        DialogEvent::Progress {
            id: DialogId(id as usize),
            title: cstr(title),
            text: cstr(text),
            position,
            cancel: cstr(cancel),
        },
    );
}

unsafe extern "C" fn dlg_cancel(data: *mut c_void, id: *mut LibVlcDialogId) {
    emit(
        data,
        DialogEvent::Cancel {
            id: DialogId(id as usize),
        },
    );
}

unsafe extern "C" fn dlg_update(
    data: *mut c_void,
    id: *mut LibVlcDialogId,
    position: c_float,
    text: *const c_char,
) {
    emit(
        data,
        DialogEvent::Update {
            id: DialogId(id as usize),
            position,
            text: cstr(text),
        },
    );
}

static DIALOG_CBS: DialogCbs = DialogCbs {
    display_error: dlg_error,
    display_login: dlg_login,
    display_question: dlg_question,
    display_progress: dlg_progress,
    cancel: dlg_cancel,
    update_progress: dlg_update,
};

impl VlcEngine {
    /// Load an extra interface module into the instance (e.g. "audioscrobbler").
    pub fn add_intf(&self, name: &str) -> bool {
        let (Some(f), Ok(c)) = (self.symbols.ext.add_intf, CString::new(name)) else {
            return false;
        };
        unsafe { f(self.instance, c.as_ptr()) == 0 }
    }

    /// Route libVLC's interactive dialogs (network logins, certificate
    /// questions, errors) to `handler`, called from libVLC threads.
    pub fn set_dialog_handler(&self, handler: impl Fn(DialogEvent) + Send + Sync + 'static) {
        let Some(set) = self.symbols.ext.dialog_set_callbacks else {
            return;
        };
        let raw: *mut DialogHandler = Box::into_raw(Box::new(Box::new(handler)));
        unsafe { set(self.instance, &DIALOG_CBS, raw as *mut c_void) };
        if let Ok(mut d) = self.inst.dialog_data.lock() {
            if let Some(old) = d.replace(raw) {
                unsafe { drop(Box::from_raw(old)) };
            }
        }
    }

    pub fn dialog_login(&self, id: DialogId, user: &str, password: &str, store: bool) {
        let (Some(f), Ok(u), Ok(p)) = (
            self.symbols.ext.dialog_post_login,
            CString::new(user),
            CString::new(password),
        ) else {
            return;
        };
        unsafe { f(id.0 as *mut LibVlcDialogId, u.as_ptr(), p.as_ptr(), store) };
    }

    /// Answer a question dialog with action 1 or 2.
    pub fn dialog_action(&self, id: DialogId, action: i32) {
        if let Some(f) = self.symbols.ext.dialog_post_action {
            unsafe { f(id.0 as *mut LibVlcDialogId, action) };
        }
    }

    pub fn dialog_dismiss(&self, id: DialogId) {
        if let Some(f) = self.symbols.ext.dialog_dismiss {
            unsafe { f(id.0 as *mut LibVlcDialogId) };
        }
    }

    /// Start discovering cast targets ("microdns_renderer" finds Chromecasts).
    pub fn renderer_discovery(&self, name: &str) -> Option<RendererDiscovery> {
        let x = &self.symbols.ext;
        let (new, start, em_of, attach) =
            (x.rd_new?, x.rd_start?, x.rd_event_manager?, x.event_attach?);
        let c = CString::new(name).ok()?;
        let rd = unsafe { new(self.instance, c.as_ptr()) };
        if rd.is_null() {
            return None;
        }
        let shared = Arc::new(DiscoveryShared {
            inst: self.inst.clone(),
            items: std::sync::Mutex::new(Vec::new()),
            changed: AtomicBool::new(false),
        });
        let disc = RendererDiscovery { rd, shared };
        unsafe {
            let em = em_of(rd);
            if em.is_null() {
                return None;
            }
            let data = Arc::as_ptr(&disc.shared) as *mut c_void;
            attach(em, EV_RENDERER_ADDED, renderer_event_cb, data);
            attach(em, EV_RENDERER_DELETED, renderer_event_cb, data);
            if start(rd) != 0 {
                return None;
            }
        }
        Some(disc)
    }
}
