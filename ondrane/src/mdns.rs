//! Chromecast discovery over mDNS (`_googlecast._tcp.local`), used next to
//! libVLC's own renderer discoverer: many Linux builds of VLC 3 ship without
//! the microdns module, so it would never find anything there.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const SERVICE: &str = "_googlecast._tcp.local";

/// A Chromecast found on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    pub name: String,
    pub ip: Ipv4Addr,
    pub port: u16,
}

/// Ask the network for Chromecasts and collect the answers for `wait`.
pub fn discover(wait: Duration) -> Vec<CastDevice> {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return Vec::new();
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = sock.set_multicast_loop_v4(true);
    let _ = sock.set_multicast_ttl_v4(255);
    let dest: SocketAddr = (Ipv4Addr::new(224, 0, 0, 251), 5353).into();
    let mut found = Answers::default();
    let start = Instant::now();
    let mut sent = 0;
    let mut buf = [0u8; 9000];
    while start.elapsed() < wait {
        // a few queries in case the first one is lost
        if sent < 3 && start.elapsed() >= Duration::from_millis(400 * sent) {
            let _ = sock.send_to(&query(SERVICE), dest);
            sent += 1;
        }
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            found.parse(&buf[..n], from);
        }
    }
    found.devices()
}

/// A one-question PTR query, sent from a random port so responders answer
/// us directly ("legacy unicast", RFC 6762 §6.7).
fn query(service: &str) -> Vec<u8> {
    let mut q = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in service.split('.').filter(|s| !s.is_empty()) {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 12, 0, 1]);
    q
}

/// Records gathered from every answer, joined into devices at the end.
#[derive(Default)]
struct Answers {
    /// instance name → friendly name (TXT "fn=")
    names: BTreeMap<String, String>,
    /// instance name → (target host, port) (SRV)
    srv: BTreeMap<String, (String, u16)>,
    /// host → address (A)
    addr: BTreeMap<String, Ipv4Addr>,
    /// instance name → address the answer came from
    sender: BTreeMap<String, Ipv4Addr>,
}

impl Answers {
    fn parse(&mut self, msg: &[u8], from: SocketAddr) {
        let Some(count) = msg.get(4..12) else { return };
        let n = |i: usize| u16::from_be_bytes([count[i], count[i + 1]]) as usize;
        let (qd, records) = (n(0), n(2) + n(4) + n(6));
        let mut pos = 12;
        for _ in 0..qd {
            let Some((_, p)) = read_name(msg, pos) else {
                return;
            };
            pos = p + 4;
        }
        for _ in 0..records {
            let Some((name, p)) = read_name(msg, pos) else {
                return;
            };
            let Some(h) = msg.get(p..p + 10) else { return };
            let kind = u16::from_be_bytes([h[0], h[1]]);
            let len = u16::from_be_bytes([h[8], h[9]]) as usize;
            let data = p + 10;
            let Some(rdata) = msg.get(data..data + len) else {
                return;
            };
            match kind {
                12 => {
                    if let Some((inst, _)) = read_name(msg, data) {
                        if let (true, SocketAddr::V4(v4)) =
                            (name.eq_ignore_ascii_case(SERVICE), from)
                        {
                            self.sender.insert(inst, *v4.ip());
                        }
                    }
                }
                16 => {
                    let mut i = 0;
                    while i < rdata.len() {
                        let l = rdata[i] as usize;
                        let entry = rdata.get(i + 1..i + 1 + l).unwrap_or_default();
                        if let Some(v) = entry.strip_prefix(b"fn=") {
                            self.names
                                .insert(name.clone(), String::from_utf8_lossy(v).into_owned());
                        }
                        i += 1 + l;
                    }
                }
                33 if len > 6 => {
                    let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                    if let Some((target, _)) = read_name(msg, data + 6) {
                        self.srv.insert(name.clone(), (target, port));
                    }
                }
                1 if len == 4 => {
                    let ip = Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]);
                    self.addr.insert(name.to_ascii_lowercase(), ip);
                }
                _ => {}
            }
            pos = data + len;
        }
    }

    fn devices(&self) -> Vec<CastDevice> {
        let mut out: Vec<CastDevice> = Vec::new();
        let mut instances: Vec<String> =
            self.srv.keys().chain(self.sender.keys()).cloned().collect();
        instances.sort();
        instances.dedup();
        for inst in &instances {
            let (host, port) = self
                .srv
                .get(inst)
                .cloned()
                .unwrap_or_else(|| (String::new(), 8009));
            let Some(ip) = self
                .addr
                .get(&host.to_ascii_lowercase())
                .or_else(|| self.sender.get(inst))
                .copied()
            else {
                continue;
            };
            let name = self.names.get(inst).cloned().unwrap_or_else(|| {
                inst.split("._googlecast")
                    .next()
                    .unwrap_or(inst)
                    .to_string()
            });
            if !out.iter().any(|d| d.ip == ip && d.port == port) {
                out.push(CastDevice { name, ip, port });
            }
        }
        out
    }
}

/// Read a (possibly compressed) DNS name; returns it and the position after it.
fn read_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(pos + 1)));
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *msg.get(pos + 1)? as usize;
            end.get_or_insert(pos + 2);
            pos = ptr;
            continue;
        }
        if len & 0xC0 != 0 {
            // Reserved DNS label type, abort to prevent corrupt interpretation
            return None;
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos += 1 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(out: &mut Vec<u8>, n: &str) {
        for l in n.split('.') {
            out.push(l.len() as u8);
            out.extend_from_slice(l.as_bytes());
        }
        out.push(0);
    }

    fn record(out: &mut Vec<u8>, n: &str, kind: u16, rdata: &[u8]) {
        name(out, n);
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&[0, 1, 0, 0, 0, 120]);
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }

    #[test]
    fn parses_a_chromecast_answer() {
        let inst = "Chromecast-abc._googlecast._tcp.local";
        let mut msg = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 3];
        let mut ptr = Vec::new();
        name(&mut ptr, inst);
        record(&mut msg, SERVICE, 12, &ptr);
        let mut txt = vec![6];
        txt.extend_from_slice(b"id=abc");
        txt.push(11);
        txt.extend_from_slice(b"fn=Salon TV");
        record(&mut msg, inst, 16, &txt);
        let mut srv = vec![0, 0, 0, 0, 0x1F, 0x49];
        name(&mut srv, "abc.local");
        record(&mut msg, inst, 33, &srv);
        record(&mut msg, "abc.local", 1, &[192, 168, 1, 42]);
        let mut a = Answers::default();
        a.parse(&msg, "192.168.1.42:5353".parse().unwrap());
        assert_eq!(
            a.devices(),
            vec![CastDevice {
                name: "Salon TV".into(),
                ip: Ipv4Addr::new(192, 168, 1, 42),
                port: 8009
            }]
        );
        assert_eq!(&query(SERVICE)[12..24], b"\x0b_googlecast");
        // Trailing dot does not create duplicate null byte
        assert_eq!(
            &query("_googlecast._tcp.local.")[12..24],
            b"\x0b_googlecast"
        );
    }
}
