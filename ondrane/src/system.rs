//! Desktop integration: keeping the screen awake during playback, desktop
//! notifications, the system-tray icon and the update check.

use std::process::{Child, Command, Stdio};

/// The desktop session remains Wayland even when this window uses XWayland.
pub fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
}

// ------------------------------------------------------------ screensaver

/// Keeps the screen saver / display sleep away while it is alive.
pub struct Inhibitor {
    #[cfg(target_os = "linux")]
    dbus: Option<(zbus::blocking::Connection, u32)>,
    #[cfg(target_os = "linux")]
    portal: Option<(zbus::blocking::Connection, zbus::zvariant::OwnedObjectPath)>,
    child: Option<Child>,
}

impl Inhibitor {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            let portal = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.method_timeout(std::time::Duration::from_secs(2)).build())
                .ok()
                .and_then(|c| {
                    let options = std::collections::HashMap::from([(
                        "reason",
                        zbus::zvariant::Value::from("Playing a video"),
                    )]);
                    let reply = c
                        .call_method(
                            Some("org.freedesktop.portal.Desktop"),
                            "/org/freedesktop/portal/desktop",
                            Some("org.freedesktop.portal.Inhibit"),
                            "Inhibit",
                            &("", 8u32, options),
                        )
                        .ok()?;
                    let handle = reply.body().deserialize().ok()?;
                    Some((c, handle))
                });
            let dbus = if portal.is_none() {
                zbus::blocking::Connection::session().ok().and_then(|c| {
                    let reply = c
                        .call_method(
                            Some("org.freedesktop.ScreenSaver"),
                            "/org/freedesktop/ScreenSaver",
                            Some("org.freedesktop.ScreenSaver"),
                            "Inhibit",
                            &("Ondrane", "Playing a video"),
                        )
                        .ok()?;
                    let cookie: u32 = reply.body().deserialize().ok()?;
                    Some((c, cookie))
                })
            } else {
                None
            };
            // X11 sessions without the D-Bus service: xdg-screensaver
            let child = if portal.is_none() && dbus.is_none() && !is_wayland_session() {
                Command::new("sh")
                    .args([
                        "-c",
                        "trap 'exit 0' TERM INT; while true; do xdg-screensaver reset; sleep 30 & wait $!; done",
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()
            } else {
                None
            };
            Self {
                dbus,
                portal,
                child,
            }
        }
        #[cfg(target_os = "windows")]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn SetThreadExecutionState(flags: u32) -> u32;
            }
            // ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
            unsafe { SetThreadExecutionState(0x8000_0000 | 0x2 | 0x1) };
            Self { child: None }
        }
        #[cfg(target_os = "macos")]
        {
            let child = Command::new("caffeinate")
                .args(["-d", "-i", "-w", &std::process::id().to_string()])
                .stdout(Stdio::null())
                .spawn()
                .ok();
            Self { child }
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        Self { child: None }
    }
}

impl Default for Inhibitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some((c, handle)) = self.portal.take() {
            let _ = c.call_method(
                Some("org.freedesktop.portal.Desktop"),
                handle.as_str(),
                Some("org.freedesktop.portal.Request"),
                "Close",
                &(),
            );
        }
        #[cfg(target_os = "linux")]
        if let Some((c, cookie)) = self.dbus.take() {
            let _ = c.call_method(
                Some("org.freedesktop.ScreenSaver"),
                "/org/freedesktop/ScreenSaver",
                Some("org.freedesktop.ScreenSaver"),
                "UnInhibit",
                &(cookie,),
            );
        }
        #[cfg(target_os = "windows")]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn SetThreadExecutionState(flags: u32) -> u32;
            }
            unsafe { SetThreadExecutionState(0x8000_0000) };
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

// ---------------------------------------------------------- notifications

/// Show a desktop notification (track changes, finished conversions).
pub fn notify(title: &str, body: &str, timeout_ms: u32) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let _ = notify_rust::Notification::new()
                .appname("Ondrane")
                .summary(&title)
                .body(&body)
                .icon("ondrane")
                .timeout(notify_rust::Timeout::Milliseconds(timeout_ms))
                .show();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = timeout_ms;
            let _ = Command::new("osascript")
                .args([
                    "-e",
                    "on run argv\n\
                     display notification (item 1 of argv) with title (item 2 of argv)\n\
                     end run",
                    &body,
                    &title,
                ])
                .status();
        }
    });
}

// ------------------------------------------------------------------ tray

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCmd {
    Toggle,
    PlayPause,
    Next,
    Previous,
    Quit,
}

/// Labels of the tray menu, translated by the caller.
#[derive(Clone)]
pub struct TrayLabels {
    pub show: String,
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub quit: String,
}

fn icon_rgba() -> Option<(u32, u32, Vec<u8>)> {
    let png = include_bytes!("../../packaging/linux/icons/64x64/apps/ondrane.png");
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    Some((img.width(), img.height(), img.into_raw()))
}

#[cfg(target_os = "linux")]
mod tray_impl {
    use super::*;
    use ksni::blocking::TrayMethods;

    struct Item {
        labels: TrayLabels,
        send: Box<dyn Fn(TrayCmd) + Send>,
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "ondrane".into()
        }
        fn title(&self) -> String {
            "Ondrane".into()
        }
        fn icon_name(&self) -> String {
            "ondrane".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let Some((w, h, rgba)) = icon_rgba() else {
                return vec![];
            };
            let mut argb = Vec::with_capacity(rgba.len());
            for px in rgba.as_chunks::<4>().0 {
                argb.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
            }
            vec![ksni::Icon {
                width: w as i32,
                height: h as i32,
                data: argb,
            }]
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            (self.send)(TrayCmd::Toggle);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            let item = |label: &str, cmd: TrayCmd| {
                StandardItem {
                    label: label.to_string(),
                    activate: Box::new(move |t: &mut Self| (t.send)(cmd)),
                    ..Default::default()
                }
                .into()
            };
            vec![
                item(&self.labels.show, TrayCmd::Toggle),
                ksni::MenuItem::Separator,
                item(&self.labels.play_pause, TrayCmd::PlayPause),
                item(&self.labels.previous, TrayCmd::Previous),
                item(&self.labels.next, TrayCmd::Next),
                ksni::MenuItem::Separator,
                item(&self.labels.quit, TrayCmd::Quit),
            ]
        }
    }

    pub struct Tray(ksni::blocking::Handle<Item>);

    impl Tray {
        pub fn new(labels: TrayLabels, send: impl Fn(TrayCmd) + Send + 'static) -> Option<Self> {
            Item {
                labels,
                send: Box::new(send),
            }
            .spawn()
            .ok()
            .map(Tray)
        }
        /// Commands arrive through the callback on Linux.
        pub fn poll(&self) -> Option<TrayCmd> {
            None
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            self.0.shutdown().wait();
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod tray_impl {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        _icon: tray_icon::TrayIcon,
        ids: Vec<(MenuId, TrayCmd)>,
    }

    impl Tray {
        /// Must be called on the UI thread (inside the event loop).
        pub fn new(labels: TrayLabels, _send: impl Fn(TrayCmd) + Send + 'static) -> Option<Self> {
            let menu = Menu::new();
            let mut ids = Vec::new();
            for (label, cmd) in [
                (&labels.show, TrayCmd::Toggle),
                (&labels.play_pause, TrayCmd::PlayPause),
                (&labels.previous, TrayCmd::Previous),
                (&labels.next, TrayCmd::Next),
                (&labels.quit, TrayCmd::Quit),
            ] {
                if cmd == TrayCmd::Quit || cmd == TrayCmd::PlayPause {
                    let _ = menu.append(&PredefinedMenuItem::separator());
                }
                let item = MenuItem::new(label, true, None);
                ids.push((item.id().clone(), cmd));
                let _ = menu.append(&item);
            }
            let (w, h, rgba) = icon_rgba()?;
            let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Ondrane")
                .with_icon(icon)
                .build()
                .ok()?;
            Some(Self { _icon: tray, ids })
        }

        pub fn poll(&self) -> Option<TrayCmd> {
            if let Ok(ev) = MenuEvent::receiver().try_recv() {
                return self.ids.iter().find(|(id, _)| *id == ev.id).map(|x| x.1);
            }
            if let Ok(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }) = TrayIconEvent::receiver().try_recv()
            {
                return Some(TrayCmd::Toggle);
            }
            None
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod tray_impl {
    use super::*;
    pub struct Tray;
    impl Tray {
        pub fn new(_: TrayLabels, _: impl Fn(TrayCmd) + Send + 'static) -> Option<Self> {
            None
        }
        pub fn poll(&self) -> Option<TrayCmd> {
            None
        }
    }
}

pub use tray_impl::Tray;

// ---------------------------------------------------------------- updates

pub const RELEASES_API: &str = "https://api.github.com/repos/Lib-LOCALE/ondrane/releases/latest";

fn version_tuple(v: &str) -> Vec<u64> {
    let mut t: Vec<u64> = v
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-'])
        .map_while(|p| p.parse::<u64>().ok())
        .collect();
    while t.last() == Some(&0) {
        t.pop();
    }
    t
}

/// True when `latest` (a release tag) is newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    version_tuple(latest) > version_tuple(current)
}

/// Latest published release tag, fetched with the system `curl` (present on
/// Windows 10+, macOS and Linux desktops). Blocking: call from a thread.
pub fn latest_release() -> Option<String> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            "-A",
            "Ondrane",
            RELEASES_API,
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("tag_name")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore = "requires a live XDG Inhibit portal"]
    fn desktop_inhibit_portal() {
        let inhibitor = Inhibitor::new();
        assert!(
            inhibitor.portal.is_some(),
            "XDG Inhibit request must be acquired"
        );
        drop(inhibitor);
    }

    #[test]
    fn versions() {
        assert!(is_newer("v2.1.0", "2.0.0"));
        assert!(is_newer("2.0.10", "2.0.9"));
        assert!(!is_newer("v2.0.0", "2.0.0"));
        assert!(!is_newer("1.9", "2.0.0"));

        // Regression: trailing zero equivalence
        assert!(!is_newer("v2.0.0", "2.0"));
        assert!(!is_newer("2.0", "2.0.0"));
        assert!(is_newer("2.0.1", "2.0"));
        assert!(is_newer("v2.1", "2.0.3"));
    }
}

// -------------------------------------------------------------- registry

/// In-process Windows registry reads (no `reg.exe` child process, which is
/// slow to spawn and flashes a console window).
#[cfg(target_os = "windows")]
pub mod registry {
    use std::ffi::c_void;

    pub type Hive = isize;
    pub const HKCU: Hive = 0x8000_0001u32 as i32 as isize;
    pub const HKLM: Hive = 0x8000_0002u32 as i32 as isize;

    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            ty: *mut u32,
            data: *mut c_void,
            len: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn get_string(hive: Hive, key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut buf = [0u16; 1024];
        let mut len = std::mem::size_of_val(&buf) as u32;
        let rc = unsafe {
            RegGetValueW(
                hive,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if rc != 0 {
            return None;
        }
        let units = (len as usize / 2).min(buf.len());
        let text = String::from_utf16_lossy(&buf[..units]);
        Some(text.trim_end_matches('\0').trim().to_string())
    }

    pub fn get_dword(hive: Hive, key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let mut data = 0u32;
        let mut len = 4u32;
        let rc = unsafe {
            RegGetValueW(
                hive,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut len,
            )
        };
        (rc == 0).then_some(data)
    }
}
