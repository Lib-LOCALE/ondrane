//! Native file pickers: Win32 common dialogs on Windows, XDG Desktop Portal /
//! zenity / kdialog on Linux. All functions block, so the GUI calls them from
//! a worker thread.

use std::path::PathBuf;
use std::process::Command;

pub const MEDIA_PATTERNS: &str = "*.mp4 *.mkv *.avi *.mov *.webm *.wmv *.flv *.m4v *.mpg *.mpeg *.ts *.m2ts *.3gp *.ogv *.mp3 *.flac *.wav *.ogg *.opus *.m4a *.aac *.wma *.aiff *.m3u *.m3u8 *.pls *.xspf";
pub const SUBTITLE_PATTERNS: &str = "*.srt *.ass *.ssa *.vtt *.sub *.idx *.smi";

/// Prefer the desktop portal (COSMIC/GNOME/KDE, including native Wayland).
/// `None` means unavailable; cancellation is a successful empty selection.
#[cfg(target_os = "linux")]
fn portal_pick(
    title: &str,
    multiple: bool,
    directory: bool,
    save_name: Option<&str>,
    filter: Option<(&str, &str)>,
) -> Option<Vec<PathBuf>> {
    use std::collections::HashMap;
    use zbus::{
        blocking::{Connection, Proxy},
        zvariant::{OwnedObjectPath, OwnedValue, Value},
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let c = Connection::session().ok()?;
    let token = format!(
        "ondrane_{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let sender = c
        .unique_name()?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = Proxy::new(
        &c,
        "org.freedesktop.portal.Desktop",
        path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .ok()?;
    // Subscribe before opening: the response may arrive before the method returns.
    let mut responses = request.receive_signal("Response").ok()?;
    let mut options = HashMap::from([
        ("handle_token", Value::from(token.as_str())),
        ("multiple", Value::from(multiple)),
        ("directory", Value::from(directory)),
    ]);
    if let Some(name) = save_name {
        options.insert("current_name", Value::from(name));
    }
    if let Some((label, patterns)) = filter {
        let patterns: Vec<(u32, &str)> = patterns.split_whitespace().map(|p| (0, p)).collect();
        options.insert(
            "filters",
            Value::from(vec![(label, patterns), ("All Files", vec![(0, "*")])]),
        );
    }
    let reply = c
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.FileChooser"),
            if save_name.is_some() {
                "SaveFile"
            } else {
                "OpenFile"
            },
            &("", title, options),
        )
        .ok()?;
    let handle: OwnedObjectPath = reply.body().deserialize().ok()?;
    if handle.as_str() != path {
        return None;
    }
    let response = responses.next()?;
    let (code, mut results): (u32, HashMap<String, OwnedValue>) =
        response.body().deserialize().ok()?;
    if code == 1 {
        return Some(vec![]);
    }
    if code != 0 {
        return None;
    }
    let uris = Vec::<String>::try_from(results.remove("uris")?).ok()?;
    Some(portal_paths(uris))
}

#[cfg(target_os = "linux")]
fn portal_paths(uris: Vec<String>) -> Vec<PathBuf> {
    uris.iter()
        .filter_map(|uri| url::Url::parse(uri).ok()?.to_file_path().ok())
        .collect()
}

/// Native Win32 OPENFILENAMEW dialog via `comdlg32.dll` (zero external dependencies on Windows 10/11 x64).
#[cfg(any(target_os = "windows", test))]
fn parse_win32_file_buffer(buf: &[u16]) -> Vec<PathBuf> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, &unit) in buf.iter().enumerate() {
        if unit == 0 {
            if i == start {
                break;
            }
            parts.push(String::from_utf16_lossy(&buf[start..i]));
            start = i + 1;
        }
    }
    if parts.len() == 1 {
        vec![PathBuf::from(&parts[0])]
    } else if parts.len() > 1 {
        let dir = PathBuf::from(&parts[0]);
        parts[1..].iter().map(|f| dir.join(f)).collect()
    } else {
        Vec::new()
    }
}

#[cfg(any(target_os = "windows", test))]
fn save_dialog_spec(file_name: &str) -> (String, Option<String>) {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::trim)
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()));
    if let Some(ext) = ext {
        let upper = ext.to_ascii_uppercase();
        let filter = format!("{upper} Files (*.{ext})\0*.{ext}\0All Files\0*.*\0\0");
        (filter, Some(ext.to_string()))
    } else {
        ("All Files\0*.*\0\0".to_string(), None)
    }
}

#[cfg(any(target_os = "windows", test))]
fn win32_explorer_select_arg(path: &std::path::Path) -> String {
    let clean: String = path
        .display()
        .to_string()
        .chars()
        .filter(|&c| c != '"')
        .collect();
    format!("/select,\"{clean}\"")
}

#[cfg(target_os = "windows")]
mod win32 {
    use super::*;
    use std::ffi::c_void;

    #[repr(C)]
    struct OpenFileNameW {
        l_struct_size: u32,
        hwnd_owner: usize,
        h_instance: usize,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *mut u16,
        n_max_cust_filter: u32,
        n_filter_index: u32,
        lpstr_file: *mut u16,
        n_max_file: u32,
        lpstr_file_title: *mut u16,
        n_max_file_title: u32,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: u32,
        n_file_offset: u16,
        n_file_extension: u16,
        lpstr_def_ext: *const u16,
        l_cust_data: usize,
        lpfn_hook: usize,
        lp_template_name: *const u16,
        pv_reserved: usize,
        dw_reserved: u32,
        flags_ex: u32,
    }

    #[repr(C)]
    struct BrowseInfoW {
        hwnd_owner: usize,
        pidl_root: *const c_void,
        psz_display_name: *mut u16,
        lpsz_title: *const u16,
        ul_flags: u32,
        lpfn: usize,
        l_param: isize,
        i_image: i32,
    }

    type FnGetFileNameW = unsafe extern "system" fn(*mut OpenFileNameW) -> i32;
    type FnSHBrowseForFolderW = unsafe extern "system" fn(*mut BrowseInfoW) -> *mut c_void;
    type FnSHGetPathFromIDListW = unsafe extern "system" fn(*const c_void, *mut u16) -> i32;
    type FnSHGetPathFromIDListEx =
        unsafe extern "system" fn(*const c_void, *mut u16, u32, u32) -> i32;
    type FnCoInitializeEx = unsafe extern "system" fn(*mut c_void, u32) -> i32;
    type FnCoUninitialize = unsafe extern "system" fn();
    type FnCoTaskMemFree = unsafe extern "system" fn(*mut c_void);

    fn wide_z(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn base_ofn(
        filter_utf16: &[u16],
        title_utf16: &[u16],
        file_buf: &mut [u16],
        def_ext_ptr: *const u16,
        flags: u32,
    ) -> OpenFileNameW {
        OpenFileNameW {
            l_struct_size: std::mem::size_of::<OpenFileNameW>() as u32,
            hwnd_owner: 0,
            h_instance: 0,
            lpstr_filter: filter_utf16.as_ptr(),
            lpstr_custom_filter: std::ptr::null_mut(),
            n_max_cust_filter: 0,
            n_filter_index: 1,
            lpstr_file: file_buf.as_mut_ptr(),
            n_max_file: file_buf.len() as u32,
            lpstr_file_title: std::ptr::null_mut(),
            n_max_file_title: 0,
            lpstr_initial_dir: std::ptr::null(),
            lpstr_title: title_utf16.as_ptr(),
            flags,
            n_file_offset: 0,
            n_file_extension: 0,
            lpstr_def_ext: def_ext_ptr,
            l_cust_data: 0,
            lpfn_hook: 0,
            lp_template_name: std::ptr::null(),
            pv_reserved: 0,
            dw_reserved: 0,
            flags_ex: 0,
        }
    }

    pub fn open_file_dialog(title: &str, filter: &str, allow_multiple: bool) -> Vec<PathBuf> {
        let Ok(lib) = (unsafe { libloading::Library::new("comdlg32.dll") }) else {
            return Vec::new();
        };
        let Ok(get_open) = (unsafe { lib.get::<FnGetFileNameW>(b"GetOpenFileNameW\0") }) else {
            return Vec::new();
        };

        let filter_utf16: Vec<u16> = filter.encode_utf16().collect();
        let title_utf16 = wide_z(title);
        let mut file_buf = vec![0u16; 8192];

        // OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        let mut flags = 0x0008_0000 | 0x0000_1000 | 0x0000_0800 | 0x0000_0008;
        if allow_multiple {
            flags |= 0x0000_0200; // OFN_ALLOWMULTISELECT
        }

        let mut ofn = base_ofn(
            &filter_utf16,
            &title_utf16,
            &mut file_buf,
            std::ptr::null(),
            flags,
        );
        if unsafe { get_open(&mut ofn) } != 0 {
            parse_win32_file_buffer(&file_buf)
        } else {
            Vec::new()
        }
    }

    pub fn save_file_dialog(title: &str, file_name: &str) -> Option<PathBuf> {
        let lib = unsafe { libloading::Library::new("comdlg32.dll") }.ok()?;
        let get_save = unsafe { lib.get::<FnGetFileNameW>(b"GetSaveFileNameW\0") }.ok()?;

        let (filter, def_ext) = save_dialog_spec(file_name);
        let filter_utf16: Vec<u16> = filter.encode_utf16().collect();
        let title_utf16 = wide_z(title);
        let def_ext_utf16 = def_ext.as_deref().map(wide_z);
        let def_ext_ptr = def_ext_utf16
            .as_ref()
            .map_or(std::ptr::null(), |v| v.as_ptr());

        let mut file_buf = vec![0u16; 4096];
        let initial: Vec<u16> = file_name.encode_utf16().collect();
        let copy_len = initial.len().min(file_buf.len().saturating_sub(1));
        file_buf[..copy_len].copy_from_slice(&initial[..copy_len]);

        // OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        let flags = 0x0008_0000 | 0x0000_0002 | 0x0000_0800 | 0x0000_0008;
        let mut ofn = base_ofn(
            &filter_utf16,
            &title_utf16,
            &mut file_buf,
            def_ext_ptr,
            flags,
        );
        if unsafe { get_save(&mut ofn) } != 0 {
            let end = file_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_buf.len());
            let s = String::from_utf16_lossy(&file_buf[..end]);
            (!s.is_empty()).then(|| PathBuf::from(s))
        } else {
            None
        }
    }

    pub fn pick_folder_dialog(title: &str) -> Option<PathBuf> {
        let shell32 = unsafe { libloading::Library::new("shell32.dll") }.ok()?;
        let ole32 = unsafe { libloading::Library::new("ole32.dll") }.ok()?;
        let browse =
            unsafe { shell32.get::<FnSHBrowseForFolderW>(b"SHBrowseForFolderW\0") }.ok()?;
        let co_free = unsafe { ole32.get::<FnCoTaskMemFree>(b"CoTaskMemFree\0") }.ok()?;

        // BIF_NEWDIALOGSTYLE requires COM STA; drop it only if the thread is already MTA.
        let mut co_init_ok = false;
        let mut use_new_style = true;
        if let Ok(co_init) = unsafe { ole32.get::<FnCoInitializeEx>(b"CoInitializeEx\0") } {
            const COINIT_APARTMENTTHREADED: u32 = 0x2;
            let hr = unsafe { co_init(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) };
            if hr == 0 || hr == 1 {
                co_init_ok = true;
            } else if hr < 0 {
                use_new_style = false;
            }
        }

        let title_utf16 = wide_z(title);
        let mut display_buf = vec![0u16; 512];
        // BIF_RETURNONLYFSDIRS | BIF_EDITBOX | (optional BIF_NEWDIALOGSTYLE)
        let flags = 0x0000_0001 | 0x0000_0010 | if use_new_style { 0x0000_0040 } else { 0 };
        let mut bi = BrowseInfoW {
            hwnd_owner: 0,
            pidl_root: std::ptr::null(),
            psz_display_name: display_buf.as_mut_ptr(),
            lpsz_title: title_utf16.as_ptr(),
            ul_flags: flags,
            lpfn: 0,
            l_param: 0,
            i_image: 0,
        };

        let pidl = unsafe { browse(&mut bi) };
        let result = if !pidl.is_null() {
            let mut path_buf = vec![0u16; 4096];
            let ok = if let Ok(get_ex) =
                unsafe { shell32.get::<FnSHGetPathFromIDListEx>(b"SHGetPathFromIDListEx\0") }
            {
                unsafe { get_ex(pidl, path_buf.as_mut_ptr(), path_buf.len() as u32, 0) != 0 }
            } else if let Ok(get_w) =
                unsafe { shell32.get::<FnSHGetPathFromIDListW>(b"SHGetPathFromIDListW\0") }
            {
                unsafe { get_w(pidl, path_buf.as_mut_ptr()) != 0 }
            } else {
                false
            };
            unsafe { co_free(pidl) };
            if ok {
                let end = path_buf.iter().position(|&c| c == 0).unwrap_or(0);
                let s = String::from_utf16_lossy(&path_buf[..end]);
                (!s.is_empty()).then(|| PathBuf::from(s))
            } else {
                None
            }
        } else {
            None
        };

        if co_init_ok {
            if let Ok(co_uninit) = unsafe { ole32.get::<FnCoUninitialize>(b"CoUninitialize\0") } {
                unsafe { co_uninit() };
            }
        }
        result
    }
}

#[cfg(target_os = "windows")]
pub fn win32_open_file_dialog(title: &str, filter: &str, allow_multiple: bool) -> Vec<PathBuf> {
    win32::open_file_dialog(title, filter, allow_multiple)
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn run_lines(cmd: &mut Command, sep: char) -> Option<Vec<PathBuf>> {
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return Some(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Some(Vec::new());
    }
    Some(
        text.split(sep)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

/// Pick one or more files matching space-separated glob `patterns`.
pub fn pick_files(title: &str, label: &str, patterns: &str, multiple: bool) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(paths) = portal_pick(title, multiple, false, None, Some((label, patterns))) {
        return paths;
    }
    #[cfg(target_os = "windows")]
    {
        let win_patterns = patterns.replace(' ', ";");
        let filter = format!("{label}\0{win_patterns}\0All Files\0*.*\0\0");
        return win32_open_file_dialog(title, &filter, multiple);
    }
    #[allow(unreachable_code)]
    {
        let mut z = Command::new("zenity");
        z.args([
            "--file-selection",
            &format!("--title={title}"),
            &format!("--file-filter={label} | {patterns}"),
            "--file-filter=All Files | *",
        ]);
        if multiple {
            z.args(["--multiple", "--separator=\n"]);
        }
        if let Some(v) = run_lines(&mut z, '\n') {
            return v;
        }
        let mut k = Command::new("kdialog");
        if multiple {
            k.args(["--multiple", "--separate-output"]);
        }
        k.args(["--getopenfilename", ".", &format!("{label} ({patterns})")]);
        run_lines(&mut k, '\n').unwrap_or_default()
    }
}

pub fn pick_media(multiple: bool) -> Vec<PathBuf> {
    pick_files("Open Media", "Media Files", MEDIA_PATTERNS, multiple)
}

pub fn pick_subtitles() -> Vec<PathBuf> {
    pick_files("Add Subtitles", "Subtitles", SUBTITLE_PATTERNS, true)
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(paths) = portal_pick(title, false, true, None, None) {
        return paths.into_iter().next();
    }
    #[cfg(target_os = "windows")]
    {
        return win32::pick_folder_dialog(title);
    }
    #[allow(unreachable_code)]
    {
        if let Some(v) = run_lines(
            Command::new("zenity").args([
                "--file-selection",
                "--directory",
                &format!("--title={title}"),
            ]),
            '\n',
        ) {
            return v.into_iter().next();
        }
        run_lines(
            Command::new("kdialog").args(["--getexistingdirectory", "."]),
            '\n',
        )
        .and_then(|v| v.into_iter().next())
    }
}

pub fn pick_save(title: &str, file_name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(paths) = portal_pick(title, false, false, Some(file_name), None) {
        return paths.into_iter().next();
    }
    #[cfg(target_os = "windows")]
    {
        return win32::save_file_dialog(title, file_name);
    }
    #[allow(unreachable_code)]
    {
        if let Some(v) = run_lines(
            Command::new("zenity").args([
                "--file-selection",
                "--save",
                "--confirm-overwrite",
                &format!("--title={title}"),
                &format!("--filename={file_name}"),
            ]),
            '\n',
        ) {
            return v.into_iter().next();
        }
        run_lines(
            Command::new("kdialog").args(["--getsavefilename", file_name]),
            '\n',
        )
        .and_then(|v| v.into_iter().next())
    }
}

/// Open a folder (or highlight a file on Windows) in the system file manager.
pub fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("explorer");
        cmd.creation_flags(CREATE_NO_WINDOW);
        if path.is_file() {
            cmd.raw_arg(win32_explorer_select_arg(path));
        } else {
            let target = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
            };
            cmd.arg(&target);
        }
        let _ = cmd.spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let target = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };
        let _ = Command::new("xdg-open").arg(&target).spawn();
    }
}

pub fn pick_media_files() -> Vec<PathBuf> {
    pick_media(true)
}

pub fn pick_lua() -> Option<PathBuf> {
    pick_files("Install Add-on", "Lua scripts", "*.lua *.luac", false)
        .into_iter()
        .next()
}

pub fn pick_any_file(title: &str) -> Option<PathBuf> {
    pick_files(title, "All Files", "*", false)
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_multi_select_buffer_parsing() {
        let single: Vec<u16> = "C:\\Videos\\movie.mkv\0\0".encode_utf16().collect();
        assert_eq!(
            parse_win32_file_buffer(&single),
            vec![PathBuf::from("C:\\Videos\\movie.mkv")]
        );

        let multi: Vec<u16> = "C:\\Music\0track1.flac\0track2.mp3\0\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            parse_win32_file_buffer(&multi),
            vec![
                PathBuf::from("C:\\Music").join("track1.flac"),
                PathBuf::from("C:\\Music").join("track2.mp3"),
            ]
        );

        let empty = [0u16; 4];
        assert!(parse_win32_file_buffer(&empty).is_empty());
    }

    #[test]
    fn win32_save_spec_and_explorer_select() {
        let (filter, ext) = save_dialog_spec("playlist.m3u");
        assert_eq!(ext.as_deref(), Some("m3u"));
        assert!(filter.starts_with("M3U Files (*.m3u)\0*.m3u\0"));
        assert!(filter.ends_with("\0All Files\0*.*\0\0"));

        let (plain, no_ext) = save_dialog_spec("untitled");
        assert!(no_ext.is_none());
        assert_eq!(plain, "All Files\0*.*\0\0");

        let arg = win32_explorer_select_arg(std::path::Path::new(r#"C:\Videos\clip "1".mp4"#));
        assert_eq!(arg, r#"/select,"C:\Videos\clip 1.mp4""#);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod portal_tests {
    #[test]
    fn decodes_local_uris_without_treating_network_urls_as_paths() {
        assert_eq!(
            super::portal_paths(vec![
                "file:///tmp/film%20%C3%A9t%C3%A9.mkv".into(),
                "https://example.org/movie.mp4".into(),
                "invalid".into(),
            ]),
            vec![std::path::PathBuf::from("/tmp/film été.mkv")]
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod portal_protocol_tests {
    use super::*;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    struct Chooser;

    #[zbus::interface(name = "org.freedesktop.portal.FileChooser")]
    impl Chooser {
        fn save_file(
            &self,
            parent: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            assert_eq!(
                <&str>::try_from(&options["current_name"]).unwrap(),
                "playlist.m3u"
            );
            self.open_file(parent, title, options, header, connection)
        }

        fn open_file(
            &self,
            _parent: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let token = <&str>::try_from(&options["handle_token"]).unwrap();
            let sender = header.sender().unwrap().as_str();
            let path = OwnedObjectPath::try_from(format!(
                "/org/freedesktop/portal/desktop/request/{}/{token}",
                sender.trim_start_matches(':').replace('.', "_")
            ))
            .unwrap();
            let code = if title == "cancel" { 1u32 } else { 0 };
            let c = zbus::blocking::Connection::from(connection.clone());
            let reply_path = path.clone();
            let destination = sender.to_owned();
            std::thread::spawn(move || {
                let results = HashMap::from([(
                    "uris",
                    Value::from(vec!["file:///tmp/film%20%C3%A9t%C3%A9.mkv"]),
                )]);
                c.emit_signal(
                    Some(destination.as_str()),
                    reply_path.as_str(),
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(code, results),
                )
                .unwrap();
            });
            Ok(path)
        }
    }

    #[test]
    #[ignore = "run under dbus-run-session: owns the mock FileChooser portal name"]
    fn filechooser_response_and_cancel() {
        let _service = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name("org.freedesktop.portal.Desktop")
            .unwrap()
            .serve_at("/org/freedesktop/portal/desktop", Chooser)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            portal_pick("open", true, false, None, Some(("Media", "*.mkv *.mp4"))),
            Some(vec![PathBuf::from("/tmp/film été.mkv")])
        );
        assert_eq!(portal_pick("cancel", false, true, None, None), Some(vec![]));
        assert_eq!(
            portal_pick("save", false, false, Some("playlist.m3u"), None),
            Some(vec![PathBuf::from("/tmp/film été.mkv")])
        );
    }
}
