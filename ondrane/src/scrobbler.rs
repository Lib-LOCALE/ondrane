//! "Submit played tracks to last.fm" (Settings › Privacy): the Audioscrobbler
//! 1.2 submission protocol, also accepted by Libre.fm and other compatible
//! services ("Last.fm submission URL").

use md5::{Digest, Md5};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Track {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub length_s: u64,
    pub started_unix: u64,
}

enum Msg {
    NowPlaying(Track),
    Submit(Track),
}

pub struct Scrobbler {
    tx: mpsc::Sender<Msg>,
}

fn md5_hex(s: &str) -> String {
    Md5::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Split "http[s]://host[:port]/path" (scheme optional).
fn split_url(url: &str) -> (String, u16, String) {
    let trimmed = url.trim();
    let (rest, default_port) = if let Some(stripped) = trimmed.strip_prefix("https://") {
        (stripped, 443)
    } else if let Some(stripped) = trimmed.strip_prefix("http://") {
        (stripped, 80)
    } else {
        (trimmed, 80)
    };

    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };

    let (host, port) = if hostport.starts_with('[') {
        // IPv6 bracketed address: [::1]:port or [::1]
        if let Some(end_bracket) = hostport.find(']') {
            let h = &hostport[1..end_bracket];
            let remainder = &hostport[end_bracket + 1..];
            let p = remainder
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_port);
            (h.to_string(), p)
        } else {
            (hostport.to_string(), default_port)
        }
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(default_port)),
            None => (hostport.to_string(), default_port),
        }
    };
    (host, port, path)
}

/// Minimal blocking HTTP/1.0 request; returns the response body.
fn http(method: &str, url: &str, body: &str) -> Option<String> {
    let (host, port, path) = split_url(url);
    let mut addrs = (host.as_str(), port).to_socket_addrs().ok()?;
    let target_addr = addrs.next()?;
    let mut s = TcpStream::connect_timeout(&target_addr, Duration::from_secs(10)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(15))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(15))).ok()?;
    let req = if method == "POST" {
        format!(
            "POST {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: Ondrane\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    } else {
        format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: Ondrane\r\n\r\n")
    };
    s.write_all(req.as_bytes()).ok()?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).ok()?;
    resp.split_once("\r\n\r\n").map(|(_, b)| b.to_string())
}

struct Session {
    id: String,
    now_url: String,
    submit_url: String,
}

fn handshake(server: &str, user: &str, password: &str) -> Option<Session> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let token = md5_hex(&format!("{}{}", md5_hex(password), ts));
    let url = format!(
        "{}/?hs=true&p=1.2.1&c=tst&v=1.0&u={}&t={ts}&a={token}",
        server.trim_end_matches('/'),
        urlencode(user)
    );
    let body = http("GET", &url, "")?;
    let mut lines = body.lines();
    if lines.next()?.trim() != "OK" {
        return None;
    }
    Some(Session {
        id: lines.next()?.trim().to_string(),
        now_url: lines.next()?.trim().to_string(),
        submit_url: lines.next()?.trim().to_string(),
    })
}

impl Scrobbler {
    pub fn start(server: String, user: String, password: String) -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        std::thread::spawn(move || {
            let mut session: Option<Session> = None;
            for msg in rx {
                if session.is_none() {
                    session = handshake(&server, &user, &password);
                }
                let Some(s) = &session else { continue };
                let ok = match msg {
                    Msg::NowPlaying(t) => {
                        let body = format!(
                            "s={}&a={}&t={}&b={}&l={}&n=&m=",
                            s.id,
                            urlencode(&t.artist),
                            urlencode(&t.title),
                            urlencode(&t.album),
                            t.length_s
                        );
                        http("POST", &s.now_url, &body)
                    }
                    Msg::Submit(t) => {
                        let body = format!(
                            "s={}&a[0]={}&t[0]={}&i[0]={}&o[0]=P&r[0]=&l[0]={}&b[0]={}&n[0]=&m[0]=",
                            s.id,
                            urlencode(&t.artist),
                            urlencode(&t.title),
                            t.started_unix,
                            t.length_s,
                            urlencode(&t.album)
                        );
                        http("POST", &s.submit_url, &body)
                    }
                };
                // BADSESSION or network failure: handshake again next time
                if ok
                    .map(|b| !b.trim_start().starts_with("OK"))
                    .unwrap_or(true)
                {
                    session = None;
                }
            }
        });
        Self { tx }
    }

    pub fn now_playing(&self, t: Track) {
        let _ = self.tx.send(Msg::NowPlaying(t));
    }

    /// Submit a track that was listened to (half of it, or 4 minutes).
    pub fn submit(&self, t: Track) {
        let _ = self.tx.send(Msg::Submit(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
        assert_eq!(
            split_url("post.audioscrobbler.com"),
            ("post.audioscrobbler.com".into(), 80, "/".into())
        );
        assert_eq!(
            split_url("http://turtle.libre.fm:8080/x"),
            ("turtle.libre.fm".into(), 8080, "/x".into())
        );
        assert_eq!(
            split_url("https://post.audioscrobbler.com/path"),
            ("post.audioscrobbler.com".into(), 443, "/path".into())
        );
        assert_eq!(
            split_url("http://[::1]:8080/x"),
            ("::1".into(), 8080, "/x".into())
        );
    }
}
