//! Native file pickers: Win32 common dialogs on Windows, zenity / kdialog on
//! Linux, osascript on macOS. All functions block, so the GUI calls them from
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
#[cfg(target_os = "windows")]
pub fn win32_open_file_dialog(title: &str, filter: &str, allow_multiple: bool) -> Vec<PathBuf> {
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

    type FnGetOpenFileNameW = unsafe extern "system" fn(*mut OpenFileNameW) -> i32;

    if let Ok(lib) = unsafe { libloading::Library::new("comdlg32.dll") } {
        if let Ok(get_open) = unsafe { lib.get::<FnGetOpenFileNameW>(b"GetOpenFileNameW\0") } {
            let filter_utf16: Vec<u16> = filter.encode_utf16().collect();
            let title_utf16: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            let mut file_buf = vec![0u16; 8192];

            // OFN_EXPLORER (0x00080000) | OFN_FILEMUSTEXIST (0x00001000) | OFN_PATHMUSTEXIST (0x00000800) | OFN_NOCHANGEDIR (0x00000008)
            let mut flags = 0x0008_0000 | 0x0000_1000 | 0x0000_0800 | 0x0000_0008;
            if allow_multiple {
                flags |= 0x0000_0200; // OFN_ALLOWMULTISELECT
            }

            let mut ofn = OpenFileNameW {
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
                lpstr_def_ext: std::ptr::null(),
                l_cust_data: 0,
                lpfn_hook: 0,
                lp_template_name: std::ptr::null(),
                pv_reserved: 0,
                dw_reserved: 0,
                flags_ex: 0,
            };

            let ok = unsafe { get_open(&mut ofn) };
            if ok != 0 {
                // Parse double-NUL terminated UTF-16 string list
                let mut parts = Vec::new();
                let mut start = 0;
                for i in 0..file_buf.len() {
                    if file_buf[i] == 0 {
                        if i == start {
                            break;
                        }
                        parts.push(String::from_utf16_lossy(&file_buf[start..i]));
                        start = i + 1;
                    }
                }
                if parts.len() == 1 {
                    return vec![PathBuf::from(&parts[0])];
                } else if parts.len() > 1 {
                    let dir = PathBuf::from(&parts[0]);
                    return parts[1..].iter().map(|f| dir.join(f)).collect();
                }
            }
            return Vec::new();
        }
    }

    // PowerShell fallback if comdlg32 was unavailable
    let multi_str = if allow_multiple { "$true" } else { "$false" };
    let ps_script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Title = '{}'; $f.Multiselect = {}; if ($f.ShowDialog() -eq 'OK') {{ $f.FileNames -join '|' }}",
        title.replace('\'', ""),
        multi_str
    );
    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", &ps_script])
        .output()
    {
        if output.status.success() {
            let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !out.is_empty() {
                return out.split('|').map(PathBuf::from).collect();
            }
        }
    }
    Vec::new()
}

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

#[cfg(any(target_os = "windows", test))]
fn ps_escape(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\'' | '\"' | '$' | '`' | ';' | '\n' | '\r'))
        .collect()
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
    #[cfg(target_os = "macos")]
    {
        let _ = (label, patterns); // the AppleScript chooser has no filter
        let script = if multiple {
            "on run argv\n\
             set fs to choose file with prompt (item 1 of argv) with multiple selections allowed\n\
             set out to \"\"\n\
             repeat with f in fs\n\
             set out to out & POSIX path of f & \"\\n\"\n\
             end repeat\n\
             return out\n\
             end run"
        } else {
            "on run argv\n\
             POSIX path of (choose file with prompt (item 1 of argv))\n\
             end run"
        };
        return run_lines(Command::new("osascript").args(["-e", script, title]), '\n')
            .unwrap_or_default();
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
        let ps = format!("Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = '{}'; if ($f.ShowDialog() -eq 'OK') {{ $f.SelectedPath }}", ps_escape(title));
        return run_lines(
            Command::new("powershell").args(["-NoProfile", "-STA", "-Command", &ps]),
            '\n',
        )
        .and_then(|v| v.into_iter().next());
    }
    #[cfg(target_os = "macos")]
    {
        let script = "on run argv\n\
                      POSIX path of (choose folder with prompt (item 1 of argv))\n\
                      end run";
        return run_lines(Command::new("osascript").args(["-e", script, title]), '\n')
            .and_then(|v| v.into_iter().next());
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
        let ps = format!("Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.SaveFileDialog; $f.Title = '{}'; $f.FileName = '{}'; if ($f.ShowDialog() -eq 'OK') {{ $f.FileName }}", ps_escape(title), ps_escape(file_name));
        return run_lines(
            Command::new("powershell").args(["-NoProfile", "-STA", "-Command", &ps]),
            '\n',
        )
        .and_then(|v| v.into_iter().next());
    }
    #[cfg(target_os = "macos")]
    {
        let script = "on run argv\n\
                      POSIX path of (choose file name with prompt (item 1 of argv) default name (item 2 of argv))\n\
                      end run";
        return run_lines(
            Command::new("osascript").args(["-e", script, title, file_name]),
            '\n',
        )
        .and_then(|v| v.into_iter().next());
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

/// Open a folder in the system file manager.
pub fn reveal(path: &std::path::Path) {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    };
    #[cfg(target_os = "windows")]
    let _ = Command::new("explorer").arg(&target).spawn();
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(&target).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(&target).spawn();
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
    fn test_ps_escape_sanitization() {
        let malicious = "test'; rm -rf /; $evil = `calc.exe`\n\"inject\"";
        let sanitized = ps_escape(malicious);
        assert!(!sanitized.contains('\''));
        assert!(!sanitized.contains('\"'));
        assert!(!sanitized.contains('$'));
        assert!(!sanitized.contains('`'));
        assert!(!sanitized.contains(';'));
        assert!(!sanitized.contains('\n'));
        assert_eq!(sanitized, "test rm -rf / evil = calc.exeinject");
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
