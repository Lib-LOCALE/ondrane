//! Wayland ScreenCast portal -> PipeWire -> software MJPEG/Matroska pipe -> libVLC.
//! No listening network socket, temporary recording or desktop permission bypass.
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value},
};

const DEST: &str = "org.freedesktop.portal.Desktop";
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.ScreenCast";
static NEXT: AtomicU64 = AtomicU64::new(0);
type Results = HashMap<String, OwnedValue>;

fn request<B: serde::Serialize + zbus::zvariant::DynamicType>(
    c: &Connection,
    method: &str,
    token: &str,
    body: &B,
) -> Result<Option<Results>, String> {
    let sender = c
        .unique_name()
        .ok_or("Missing D-Bus name")?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("{DESKTOP}/request/{sender}/{token}");
    let proxy = Proxy::new(c, DEST, path.as_str(), "org.freedesktop.portal.Request")
        .map_err(|e| e.to_string())?;
    let mut responses = proxy
        .receive_signal("Response")
        .map_err(|e| e.to_string())?;
    let reply = c
        .call_method(Some(DEST), DESKTOP, Some(INTERFACE), method, body)
        .map_err(|e| e.to_string())?;
    let handle: OwnedObjectPath = reply.body().deserialize().map_err(|e| e.to_string())?;
    if handle.as_str() != path {
        return Err("Unexpected ScreenCast request handle".into());
    }
    let signal = responses.next().ok_or("ScreenCast portal disconnected")?;
    let (code, results): (u32, Results) = signal.body().deserialize().map_err(|e| e.to_string())?;
    match code {
        0 => Ok(Some(results)),
        1 => Ok(None),
        _ => Err(format!("ScreenCast portal failed ({code})")),
    }
}

struct Session {
    connection: Connection,
    path: OwnedObjectPath,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.connection.call_method(
            Some(DEST),
            self.path.as_str(),
            Some("org.freedesktop.portal.Session"),
            "Close",
            &(),
        );
    }
}

pub struct Capture {
    child: Child,
    output: ChildStdout,
    session: Session,
    node: u32,
    fps: u32,
    audio: bool,
}
impl Capture {
    pub fn mrl(&self) -> String {
        format!("fd://{}", self.output.as_raw_fd())
    }
    pub fn restart(&mut self) -> Result<String, String> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let fd = remote(&self.session)?;
        let (child, output) = launch(self.fps, self.audio, self.node, fd)?;
        self.child = child;
        self.output = output;
        Ok(self.mrl())
    }
    pub fn has_exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pipeline(fps: u32, audio: bool, node: u32) -> Vec<String> {
    let mut args: Vec<String> = [
        "-q",
        "pipewiresrc",
        "fd=0",
        &format!("path={node}"),
        "do-timestamp=true",
        "keepalive-time=1000",
        "!",
        "queue",
        "max-size-buffers=2",
        "leaky=downstream",
        "!",
        "videoconvert",
        "!",
        "videorate",
        "drop-only=true",
        "!",
        &format!("video/x-raw,format=I420,framerate={}/1", fps.clamp(1, 60)),
        "!",
        "jpegenc",
        "quality=80",
        "!",
        "queue",
        "!",
        "mux.",
        "matroskamux",
        "name=mux",
        "streamable=true",
        "!",
        "fdsink",
        "fd=1",
        "sync=false",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    if audio {
        args.extend(
            [
                "pulsesrc",
                "do-timestamp=true",
                "!",
                "audioconvert",
                "!",
                "audioresample",
                "!",
                "audio/x-raw,rate=48000,channels=2",
                "!",
                "queue",
                "!",
                "mux.",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    args
}

fn gst(program: &str) -> Command {
    let mut command = Command::new(program);
    if let Some(lib) = std::env::var_os("VLC_LIB_PATH") {
        if let Some(parent) = std::path::Path::new(&lib).parent() {
            let plugins = parent.join("gstreamer-1.0");
            if plugins.is_dir() {
                let mut paths = vec![plugins];
                if let Some(current) = std::env::var_os("GST_PLUGIN_PATH") {
                    paths.extend(std::env::split_paths(&current));
                }
                if let Ok(path) = std::env::join_paths(paths) {
                    command.env("GST_PLUGIN_PATH", path);
                }
            }
        }
    }
    command
}

/// Blocks only the caller's worker thread while the desktop asks for a source.
/// Cancellation returns None, failures return a diagnostic suitable for a dialog.
pub fn start(fps: u32, audio: bool) -> Result<Option<Capture>, String> {
    for element in ["pipewiresrc", "jpegenc", "matroskamux"] {
        if !gst("gst-inspect-1.0")
            .arg(element)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return Err(format!("Missing GStreamer element: {element}. Install gstreamer1.0-tools, gstreamer1.0-pipewire, gstreamer1.0-plugins-base and gstreamer1.0-plugins-good."));
        }
    }
    let Some((session, node, fd)) = open_portal(Connection::session().map_err(|e| e.to_string())?)?
    else {
        return Ok(None);
    };
    let (child, output) = launch(fps, audio, node, fd)?;
    Ok(Some(Capture {
        child,
        output,
        session,
        node,
        fps,
        audio,
    }))
}

fn launch(
    fps: u32,
    audio: bool,
    node: u32,
    fd: std::os::fd::OwnedFd,
) -> Result<(Child, ChildStdout), String> {
    let mut args = pipeline(fps, audio, node);
    let keepalive = gst("gst-inspect-1.0")
        .arg("pipewiresrc")
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains("keepalive-time"));
    if !keepalive {
        args.retain(|a| a != "keepalive-time=1000");
    }
    let mut child = gst("gst-launch-1.0")
        .args(args)
        .stdin(Stdio::from(fd))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    let output = child.stdout.take().ok_or("Missing capture pipe")?;
    Ok((child, output))
}

fn remote(session: &Session) -> Result<std::os::fd::OwnedFd, String> {
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let reply = session
        .connection
        .call_method(
            Some(DEST),
            DESKTOP,
            Some(INTERFACE),
            "OpenPipeWireRemote",
            &(&session.path, options),
        )
        .map_err(|e| e.to_string())?;
    let fd: OwnedFd = reply.body().deserialize().map_err(|e| e.to_string())?;
    Ok(fd.into())
}

fn open_portal(c: Connection) -> Result<Option<(Session, u32, std::os::fd::OwnedFd)>, String> {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let token = format!("vlc_capture_{n}");
    let session_token = format!("vlc_session_{n}");
    let options = HashMap::from([
        ("handle_token", Value::from(token.as_str())),
        ("session_handle_token", Value::from(session_token.as_str())),
    ]);
    let Some(mut results) = request(&c, "CreateSession", &token, &options)? else {
        return Ok(None);
    };
    // ScreenCast's session_handle result is a string, unlike method handles.
    let handle = String::try_from(
        results
            .remove("session_handle")
            .ok_or("Missing ScreenCast session")?,
    )
    .map_err(|e| e.to_string())?;
    let path = OwnedObjectPath::try_from(handle).map_err(|e| e.to_string())?;
    let session = Session {
        connection: c.clone(),
        path,
    };
    let token = format!("vlc_select_{n}");
    let options = HashMap::from([
        ("handle_token", Value::from(token.as_str())),
        ("types", Value::from(3u32)),
        ("multiple", Value::from(false)),
    ]);
    if request(&c, "SelectSources", &token, &(&session.path, options))?.is_none() {
        return Ok(None);
    }
    let token = format!("vlc_start_{n}");
    let options = HashMap::from([("handle_token", Value::from(token.as_str()))]);
    let Some(mut results) = request(&c, "Start", &token, &(&session.path, "", options))? else {
        return Ok(None);
    };
    let streams = Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(
        results
            .remove("streams")
            .ok_or("Missing PipeWire streams")?,
    )
    .map_err(|e| e.to_string())?;
    let node = streams.first().ok_or("No source selected")?.0;
    let fd = remote(&session)?;
    Ok(Some((session, node, fd)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pipeline_is_bounded_and_has_no_network_or_shell() {
        let p = pipeline(0, false, 42);
        assert!(p.contains(&"path=42".into()));
        assert!(p.contains(&"video/x-raw,format=I420,framerate=1/1".into()));
        assert!(p.contains(&"max-size-buffers=2".into()));
        assert!(!p.contains(&"pulsesrc".into()));
        assert!(pipeline(999, true, 42).contains(&"pulsesrc".into()));
        assert!(!p
            .iter()
            .any(|s| s.contains("tcp") || s.contains("filesink")));
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    };
    struct Portal {
        cancel: Arc<AtomicBool>,
    }
    fn response(
        c: &zbus::Connection,
        header: zbus::message::Header<'_>,
        options: Results,
        code: u32,
        results: HashMap<&'static str, Value<'static>>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = <&str>::try_from(&options["handle_token"]).unwrap();
        let sender = header.sender().unwrap().as_str().to_owned();
        let path = OwnedObjectPath::try_from(format!(
            "{DESKTOP}/request/{}/{token}",
            sender.trim_start_matches(':').replace('.', "_")
        ))
        .unwrap();
        let target = path.clone();
        let c = Connection::from(c.clone());
        std::thread::spawn(move || {
            c.emit_signal(
                Some(sender.as_str()),
                target.as_str(),
                "org.freedesktop.portal.Request",
                "Response",
                &(code, results),
            )
            .unwrap();
        });
        Ok(path)
    }
    #[zbus::interface(name = "org.freedesktop.portal.ScreenCast")]
    impl Portal {
        fn create_session(
            &self,
            options: Results,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] c: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            response(
                c,
                header,
                options,
                0,
                HashMap::from([(
                    "session_handle",
                    Value::from("/org/freedesktop/portal/desktop/session/mock"),
                )]),
            )
        }
        fn select_sources(
            &self,
            _session: OwnedObjectPath,
            options: Results,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] c: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            assert_eq!(u32::try_from(&options["types"]).unwrap(), 3);
            assert!(!bool::try_from(&options["multiple"]).unwrap());
            response(c, header, options, 0, HashMap::new())
        }
        fn start(
            &self,
            _session: OwnedObjectPath,
            parent: &str,
            options: Results,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] c: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            assert!(parent.is_empty());
            let streams: Vec<(u32, HashMap<&str, Value<'static>>)> = vec![(42, HashMap::new())];
            response(
                c,
                header,
                options,
                if self.cancel.load(Ordering::SeqCst) {
                    1
                } else {
                    0
                },
                HashMap::from([("streams", Value::from(streams))]),
            )
        }
        fn open_pipe_wire_remote(
            &self,
            _session: OwnedObjectPath,
            _options: Results,
        ) -> zbus::fdo::Result<OwnedFd> {
            let (s, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
            Ok(OwnedFd::from(std::os::fd::OwnedFd::from(s)))
        }
    }
    struct PortalSession(Arc<AtomicUsize>);
    #[zbus::interface(name = "org.freedesktop.portal.Session")]
    impl PortalSession {
        fn close(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    #[ignore = "run under dbus-run-session; owns the ScreenCast portal name"]
    fn screencast_response_fd_cancel_and_close() {
        let cancel = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicUsize::new(0));
        let _service = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(DEST)
            .unwrap()
            .serve_at(
                DESKTOP,
                Portal {
                    cancel: cancel.clone(),
                },
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/portal/desktop/session/mock",
                PortalSession(closed.clone()),
            )
            .unwrap()
            .build()
            .unwrap();
        let source = open_portal(Connection::session().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(source.1, 42);
        assert!(source.2.as_raw_fd() >= 0);
        drop(source);
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        cancel.store(true, Ordering::SeqCst);
        assert!(open_portal(Connection::session().unwrap())
            .unwrap()
            .is_none());
        assert_eq!(closed.load(Ordering::SeqCst), 2);
    }
}
