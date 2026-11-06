//! "Web remote control" (Settings › Controls › Remotes): a small HTTP server
//! serving a phone-friendly remote page on the local network.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the remote page displays, refreshed by the UI timer.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RemoteStatus {
    pub title: String,
    pub playing: bool,
    pub time_ms: i64,
    pub length_ms: i64,
    pub volume: i32,
}

/// Commands accepted from the network (anything else is rejected).
const ALLOWED: [&str; 10] = [
    "play-pause",
    "stop",
    "next",
    "prev",
    "jump:-short",
    "jump:+short",
    "volume-up",
    "volume-down",
    "mute",
    "fullscreen",
];

pub struct RemoteServer {
    pub port: u16,
    stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<RemoteStatus>>,
}

impl RemoteServer {
    /// Listen on every interface; `on_command` receives validated commands
    /// ("play-pause", "seek:0.5", "volume:80", …) from the server threads.
    pub fn start(
        port: u16,
        password: String,
        on_command: impl Fn(String) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(RemoteStatus::default()));
        let on_command: Arc<dyn Fn(String) + Send + Sync> = Arc::new(on_command);
        let (stop2, status2) = (stop.clone(), status.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let (st, cb, pw) = (status2.clone(), on_command.clone(), password.clone());
                std::thread::spawn(move || {
                    let _ = handle(conn, &pw, &st, &*cb);
                });
            }
        });
        Ok(Self { port, stop, status })
    }
}

impl Drop for RemoteServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // unblock accept()
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn respond(
    s: &mut TcpStream,
    code: &str,
    ctype: &str,
    body: &[u8],
    extra: &str,
) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    )?;
    s.write_all(body)
}

/// Validate a command coming from the page.
pub fn validate(cmd: &str) -> Option<String> {
    if ALLOWED.contains(&cmd) {
        return Some(cmd.to_string());
    }
    if let Some(v) = cmd.strip_prefix("seek:") {
        let f: f32 = v.parse().ok()?;
        return (0.0..=1.0).contains(&f).then(|| format!("seek:{f}"));
    }
    if let Some(v) = cmd.strip_prefix("volume:") {
        let n: i32 = v.parse().ok()?;
        return (0..=200).contains(&n).then(|| format!("volume:{n}"));
    }
    None
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            if let (Some(high), Some(low)) = (hex(b[i + 1]), hex(b[i + 2])) {
                bytes.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn handle(
    mut conn: TcpStream,
    password: &str,
    status: &Mutex<RemoteStatus>,
    on_command: &(dyn Fn(String) + Send + Sync),
) -> std::io::Result<()> {
    conn.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut request = String::new();
    // Bounded line read to prevent memory exhaustion DoS
    reader.by_ref().take(8192).read_line(&mut request)?;
    if request.is_empty() {
        return Ok(());
    }

    let mut auth = String::new();
    let mut length = 0usize;
    let mut header_count = 0;
    loop {
        if header_count >= 64 {
            break;
        }
        let mut line = String::new();
        if reader.by_ref().take(2048).read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        header_count += 1;
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("authorization:") {
            auth = line[line.len() - v.len()..].trim().to_string();
        }
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0).min(4096);
        }
    }
    if length > 0 {
        let mut body = vec![0; length];
        let _ = reader.read_exact(&mut body);
    }
    if !password.is_empty() {
        let creds = if let Some(stripped) = auth.strip_prefix("Basic ") {
            stripped.trim()
        } else if let Some(stripped) = auth.strip_prefix("basic ") {
            stripped.trim()
        } else {
            ""
        };

        let expected = base64(format!("vlc:{password}").as_bytes());
        let user_empty = base64(format!(":{password}").as_bytes());
        let user_admin = base64(format!("admin:{password}").as_bytes());

        let ok = !creds.is_empty()
            && (constant_time_eq(creds.as_bytes(), expected.as_bytes())
                || constant_time_eq(creds.as_bytes(), user_empty.as_bytes())
                || constant_time_eq(creds.as_bytes(), user_admin.as_bytes()));

        if !ok {
            return respond(
                &mut conn,
                "401 Unauthorized",
                "text/plain",
                b"Password required (user: vlc)",
                "WWW-Authenticate: Basic realm=\"Ondrane\"\r\n",
            );
        }
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    if path.starts_with("/cmd?") || path.starts_with("/cmd") {
        if let Some(query) = path.split_once('?').map(|x| x.1) {
            for param in query.split('&') {
                if let Some(val) = param.strip_prefix("c=") {
                    let cmd = percent_decode(val);
                    return match validate(&cmd) {
                        Some(c) => {
                            on_command(c);
                            respond(&mut conn, "200 OK", "text/plain", b"ok", "")
                        }
                        None => respond(
                            &mut conn,
                            "400 Bad Request",
                            "text/plain",
                            b"unknown command",
                            "",
                        ),
                    };
                }
            }
        }
        return respond(
            &mut conn,
            "400 Bad Request",
            "text/plain",
            b"missing command",
            "",
        );
    }
    if path == "/status" {
        let json = serde_json::to_vec(&*status.lock().unwrap()).unwrap_or_default();
        return respond(&mut conn, "200 OK", "application/json", &json, "");
    }
    if path == "/" || path.starts_with("/?") {
        return respond(
            &mut conn,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
            "",
        );
    }
    respond(&mut conn, "404 Not Found", "text/plain", b"not found", "")
}

const PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ondrane remote</title>
<style>
body{margin:0;background:#0E0E0E;color:#EAEAEA;font-family:Inter,system-ui,sans-serif;display:flex;flex-direction:column;align-items:center;padding:24px 16px;gap:18px}
h1{font-size:18px;font-weight:500;margin:0;text-align:center;max-width:420px;overflow-wrap:anywhere}
.t{color:#969696;font-size:14px}
.row{display:flex;gap:12px;flex-wrap:wrap;justify-content:center}
button{background:#191919;color:#EAEAEA;border:0;border-radius:8px;min-width:64px;height:56px;font-size:20px;cursor:pointer}
button.main{background:#78E3CB;color:#000;min-width:96px}
input[type=range]{width:min(420px,90vw);accent-color:#78E3CB}
</style></head><body>
<h1 id="title">Ondrane</h1><div class="t" id="time">--:-- / --:--</div>
<input id="seek" type="range" min="0" max="1000" value="0">
<div class="row"><button onclick="c('prev')">&#9198;</button><button onclick="c('jump:-short')">&#8634;</button>
<button class="main" id="pp" onclick="c('play-pause')">&#9654;</button>
<button onclick="c('jump:+short')">&#8635;</button><button onclick="c('next')">&#9197;</button></div>
<div class="row"><button onclick="c('stop')">&#9632;</button><button onclick="c('mute')">&#128263;</button>
<button onclick="c('fullscreen')">&#9974;</button></div>
<input id="vol" type="range" min="0" max="200" value="80">
<script>
function c(x){fetch('/cmd?c='+encodeURIComponent(x))}
function f(ms){ms=Math.max(0,ms/1000|0);let h=ms/3600|0,m=(ms/60|0)%60,s=ms%60;return (h?h+':':'')+String(m).padStart(2,'0')+':'+String(s).padStart(2,'0')}
let busy=false;const seek=document.getElementById('seek'),vol=document.getElementById('vol');
seek.oninput=()=>busy=true;seek.onchange=()=>{c('seek:'+(seek.value/1000));busy=false};
vol.onchange=()=>c('volume:'+vol.value);
async function r(){try{const s=await (await fetch('/status')).json();
document.getElementById('title').textContent=s.title||'Ondrane';
document.getElementById('time').textContent=f(s.time_ms)+' / '+f(s.length_ms);
document.getElementById('pp').innerHTML=s.playing?'&#10074;&#10074;':'&#9654;';
if(!busy&&s.length_ms>0)seek.value=s.time_ms/s.length_ms*1000;if(document.activeElement!==vol)vol.value=s.volume}catch(e){}}
setInterval(r,1000);r();
</script></body></html>"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_unicode_percent_encoding_does_not_panic() {
        assert_eq!(percent_decode("%aé%🚀%"), "%aé%🚀%");
        assert_eq!(percent_decode("seek%3A0.5"), "seek:0.5");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn commands_and_auth() {
        assert_eq!(validate("play-pause").as_deref(), Some("play-pause"));
        assert_eq!(validate("seek:0.5").as_deref(), Some("seek:0.5"));
        assert!(validate("seek:2").is_none());
        assert!(validate("quit").is_none());
        assert_eq!(base64(b"vlc:pw"), "dmxjOnB3");
    }

    #[test]
    fn serves_page_and_commands() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let _server = RemoteServer::start(port, "pw".into(), {
            let g = got.clone();
            move |c| g.lock().unwrap().push(c)
        })
        .unwrap();
        let send = |req: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        assert!(send("GET / HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 401"));
        let auth = base64(b"vlc:pw");
        let r = send(&format!(
            "GET /cmd?c=next HTTP/1.1\r\nAuthorization: Basic {auth}\r\n\r\n"
        ));
        assert!(r.starts_with("HTTP/1.1 200"));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(got.lock().unwrap().as_slice(), ["next"]);

        // Test lowercase auth header and multi-param query
        let r2 = send(&format!(
            "GET /cmd?other=1&c=play-pause HTTP/1.1\r\nauthorization: basic {auth}\r\n\r\n"
        ));
        assert!(r2.starts_with("HTTP/1.1 200"));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(got.lock().unwrap().as_slice(), ["next", "play-pause"]);
    }
}
