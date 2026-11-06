//! "Only allow one Ondrane window" (Settings › Playlists, Folders & Files): a
//! second launch hands its files to the running window over a local socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 47_214;
const MAGIC: &str = "ONDRANE-OPEN-1";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenRequest {
    pub targets: Vec<String>,
    /// Add to the queue instead of replacing it.
    pub enqueue: bool,
}

/// Send `req` to a running instance; true when it accepted it.
pub fn forward(req: &OpenRequest) -> bool {
    forward_to(PORT, req)
}

fn forward_to(port: u16, req: &OpenRequest) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_millis(400),
    ) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let Ok(json) = serde_json::to_string(req) else {
        return false;
    };
    if writeln!(s, "{MAGIC}\n{json}").is_err() {
        return false;
    }
    let mut ack = String::new();
    BufReader::new(s).take(64).read_line(&mut ack).is_ok() && ack.trim() == "OK"
}

/// Accept requests from later launches; `on_open` runs on a server thread.
pub fn listen(on_open: impl Fn(OpenRequest) + Send + 'static) -> bool {
    listen_on(PORT, on_open)
}

fn listen_on(port: u16, on_open: impl Fn(OpenRequest) + Send + 'static) -> bool {
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)) else {
        return false;
    };
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let Ok(mut w) = conn.try_clone() else {
                continue;
            };
            let mut r = BufReader::new(conn);
            let (mut magic, mut json) = (String::new(), String::new());
            // Bounded read to prevent memory exhaustion DoS
            if r.by_ref().take(256).read_line(&mut magic).is_err() || magic.trim() != MAGIC {
                continue;
            }
            if r.by_ref().take(65_536).read_line(&mut json).is_err() {
                continue;
            }
            if let Ok(mut req) = serde_json::from_str::<OpenRequest>(json.trim()) {
                req.targets
                    .retain(|t| !t.trim().is_empty() && t.len() <= 4096);
                let _ = writeln!(w, "OK");
                on_open(req);
            }
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn forwards_to_the_first_instance() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let (tx, rx) = mpsc::channel();
        assert!(listen_on(port, move |r| tx.send(r).unwrap()));
        let req = OpenRequest {
            targets: vec!["/tmp/a.mkv".into()],
            enqueue: true,
        };
        assert!(forward_to(port, &req));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), req);

        // Blank target filtered out
        let req_with_empty = OpenRequest {
            targets: vec!["   ".into(), "/tmp/valid.mp4".into()],
            enqueue: false,
        };
        assert!(forward_to(port, &req_with_empty));
        let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(received.targets, vec!["/tmp/valid.mp4".to_string()]);
    }
}
