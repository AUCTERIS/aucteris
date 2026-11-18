//! Selective egress proxy — engine-level, best-effort.
//!
//! A per-attempt HTTP CONNECT proxy on 127.0.0.1 with a domain
//! allowlist declared by the runtime adapter. The engine injects
//! HTTP_PROXY/HTTPS_PROXY/ALL_PROXY into the child environment;
//! runtimes built on standard HTTP stacks (reqwest etc.) route through
//! it and get allow/deny decisions per destination.
//!
//! HONEST LIMITATION: this is mediation, not enforcement. A runtime
//! that ignores proxy env vars (raw sockets, hardcoded clients) can
//! still reach the network directly — the sandbox shares the host
//! netns in filtered mode. Full enforcement needs a dedicated netns
//! plus veth plus nftables (root), tracked as future work. Deny
//! counts are recorded as evidence so bypass attempts are at least
//! visible when they go through the proxy.

use anyhow::Result;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Domains the attempt may reach. Matching is suffix-based:
/// `api.openai.com` allows `api.openai.com` and `*.api.openai.com`,
/// never `evilapi.openai.com` or `openai.com.evil.test`.
pub struct EgressProxy {
    pub addr: SocketAddr,
    allowed: Arc<AtomicU64>,
    denied: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl EgressProxy {
    /// Start a proxy allowing only the given domains. Binds an
    /// ephemeral loopback port. Empty allowlist = deny everything.
    pub fn start(allow: Vec<String>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let allow = Arc::new(allow.iter().map(|d| d.to_lowercase()).collect::<Vec<_>>());
        let allowed = Arc::new(AtomicU64::new(0));
        let denied = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        let (a2, d2, s2, al2) = (
            allowed.clone(),
            denied.clone(),
            shutdown.clone(),
            allow.clone(),
        );
        let thread = std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (a, d, al) = (a2.clone(), d2.clone(), al2.clone());
                        std::thread::spawn(move || handle_conn(stream, al, a, d));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            addr,
            allowed,
            denied,
            shutdown,
            thread: Some(thread),
        })
    }

    /// (allowed, denied) connection counts — evidence for the report.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.allowed.load(Ordering::Relaxed),
            self.denied.load(Ordering::Relaxed),
        )
    }
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Nudge the accept loop.
        let _ = TcpStream::connect(self.addr);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn domain_allowed(host: &str, allow: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_lowercase();
    allow.iter().any(|d| {
        let d = d.trim_end_matches('.');
        host == d || host.ends_with(&format!(".{d}"))
    })
}

/// Read the HTTP request head (bounded), dispatch CONNECT vs forward.
fn handle_conn(
    mut client: TcpStream,
    allow: Arc<Vec<String>>,
    allowed: Arc<AtomicU64>,
    denied: Arc<AtomicU64>,
) {
    let _ = client.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    let mut buf = Vec::with_capacity(2048);
    let mut tmp = [0u8; 2048];
    // Read until end of headers or cap.
    while buf.len() < 8192 {
        match client.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let lossy = String::from_utf8_lossy(&buf);
    let head = match lossy.split("\r\n\r\n").next() {
        Some(h) => h.to_string(),
        None => return,
    };
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));

    if method.eq_ignore_ascii_case("CONNECT") {
        // target = host:port — the TLS SNI host is what we filter on.
        let host = target.split(':').next().unwrap_or("");
        if !domain_allowed(host, &allow) {
            denied.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
            return;
        }
        let upstream = match TcpStream::connect(target) {
            Ok(s) => s,
            Err(_) => {
                denied.fetch_add(1, Ordering::Relaxed);
                let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
                return;
            }
        };
        allowed.fetch_add(1, Ordering::Relaxed);
        let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
        tunnel(client, upstream);
        return;
    }

    // Plain HTTP forward: absolute URI http://host/path.
    let host = target
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    if host.is_empty() || !domain_allowed(host, &allow) {
        denied.fetch_add(1, Ordering::Relaxed);
        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let port = target
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.rsplit(':').next())
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(80);
    let upstream = match TcpStream::connect((host, port)) {
        Ok(s) => s,
        Err(_) => {
            denied.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    allowed.fetch_add(1, Ordering::Relaxed);
    // Forward the original request bytes, then bidirectional copy.
    let mut up = upstream;
    let _ = up.write_all(&buf);
    tunnel(client, up);
}

fn tunnel(a: TcpStream, b: TcpStream) {
    let _ = a.set_read_timeout(None);
    let _ = b.set_read_timeout(None);
    let (Ok(mut a2), Ok(mut b2)) = (a.try_clone(), b.try_clone()) else {
        return;
    };
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut a2, &mut b2);
    });
    let (mut a, mut b) = (a, b);
    let _ = std::io::copy(&mut b, &mut a);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_matching_is_exact() {
        let allow = vec!["api.openai.com".to_string()];
        assert!(domain_allowed("api.openai.com", &allow));
        assert!(domain_allowed("eu.api.openai.com", &allow));
        assert!(!domain_allowed("evilapi.openai.com", &allow));
        assert!(!domain_allowed("api.openai.com.evil.test", &allow));
        assert!(!domain_allowed("openai.com", &allow));
    }

    #[test]
    fn denied_connect_gets_403() {
        let proxy = EgressProxy::start(vec!["allowed.example".into()]).unwrap();
        let mut c = TcpStream::connect(proxy.addr).unwrap();
        c.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        c.write_all(b"CONNECT evil.test:443 HTTP/1.1\r\nHost: evil.test\r\n\r\n")
            .unwrap();
        let mut resp = [0u8; 256];
        let n = c.read(&mut resp).unwrap_or(0);
        assert!(String::from_utf8_lossy(&resp[..n]).contains("403"));
        assert_eq!(proxy.counts(), (0, 1));
    }

    #[test]
    fn allowed_connect_tunnels_to_local_upstream() {
        // Upstream: a dummy TCP listener playing the role of an allowed host.
        let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
        let up_addr = upstream.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = upstream.accept() {
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi");
            }
        });
        let proxy = EgressProxy::start(vec!["localhost".into()]).unwrap();
        let mut c = TcpStream::connect(proxy.addr).unwrap();
        c.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        c.write_all(
            format!(
                "CONNECT localhost:{} HTTP/1.1\r\nHost: localhost\r\n\r\n",
                up_addr.port()
            )
            .as_bytes(),
        )
        .unwrap();
        let mut resp = [0u8; 512];
        let n = c.read(&mut resp).unwrap_or(0);
        let text = String::from_utf8_lossy(&resp[..n]).to_string();
        assert!(text.contains("200 Connection Established"), "got: {text}");
        assert_eq!(proxy.counts(), (1, 0));
    }
}
