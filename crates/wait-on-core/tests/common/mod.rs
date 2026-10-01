//! Helpers shared by the integration suites. Each suite compiles its own copy and uses a
//! subset, hence the `dead_code` allow. Not in the coverage report (cargo-llvm-cov skips
//! `tests/`).
#![allow(dead_code)]

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};
use wait_on_core::http::{HttpOptions, NoValidate};
use wait_on_core::waiter::{Kind, LogFn, Resource, Sink, WaitSpec};

pub const MS: Duration = Duration::from_millis(1);
pub const NONE: Option<Arc<NoValidate>> = None;
pub const OK_CLOSE: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

pub type Lines = Arc<Mutex<Vec<String>>>;

pub fn recorder(verbose: bool) -> (Sink, Lines) {
    let lines = Lines::default();
    let rec = Arc::clone(&lines);
    let log: LogFn = Box::new(move |line| {
        rec.lock().unwrap().push(line);
        Box::pin(async {})
    });
    let sink = Sink {
        log: Some(log),
        verbose,
    };
    (sink, lines)
}

pub fn text(lines: &Lines) -> Vec<String> {
    lines.lock().unwrap().clone()
}

pub fn has(lines: &Lines, line: &str) -> bool {
    text(lines).iter().any(|l| l == line)
}

pub fn logged(lines: &Lines, prefix: &str) -> bool {
    text(lines).iter().any(|l| l.starts_with(prefix))
}

pub fn spec(resources: Vec<Resource>) -> WaitSpec {
    WaitSpec {
        delay: Duration::ZERO,
        interval: 250 * MS,
        window: Duration::ZERO,
        tcp_timeout: Duration::ZERO,
        command_timeout: Duration::ZERO,
        simultaneous: None,
        timeout: None,
        reverse: false,
        resources,
    }
}

pub fn temp(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("wait-on-loop-{}-{name}", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

/// A loopback port nothing listens on (bound, then dropped; another process could race it).
pub fn closed_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Yields (the paused clock stays put) until `done`, so real I/O lands before `advance`.
pub async fn settle(mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "never settled");
        tokio::task::yield_now().await;
    }
}

/// Keeps the runtime busy for `ms` of real time, giving stray I/O a chance to show up.
pub async fn breathe(ms: u64) {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        tokio::task::yield_now().await;
    }
}

#[derive(Debug, PartialEq)]
pub enum Seen {
    Head(usize),
    Closed(usize),
}

/// An http server answering every request with `reply`, or holding it open when `None`.
/// Reports each request head and each client-side close, numbered by connection.
pub fn server(reply: Option<&'static str>) -> (String, Receiver<Seen>) {
    let s = serve(reply, None);
    (s.url, s.seen)
}

/// What a test server saw: `Seen` events plus the raw request heads, in arrival order.
pub struct Served {
    pub url: String,
    pub seen: Receiver<Seen>,
    pub heads: Lines,
}

/// `server` over TLS when `tls` is set (`https://127.0.0.1:<port>`), keeping the heads.
pub fn serve(reply: Option<&'static str>, tls: Option<Arc<ServerConfig>>) -> Served {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let scheme = if tls.is_some() { "https" } else { "http" };
    let url = format!("{scheme}://{}", listener.local_addr().unwrap());
    let (tx, seen) = channel();
    let heads = Lines::default();
    let rec = Arc::clone(&heads);
    std::thread::spawn(move || {
        for (i, s) in listener.incoming().enumerate() {
            let (Ok(s), tx, rec, tls) = (s, tx.clone(), Arc::clone(&rec), tls.clone()) else {
                return;
            };
            std::thread::spawn(move || match tls {
                Some(cfg) => {
                    let conn = ServerConnection::new(cfg).unwrap();
                    respond(StreamOwned::new(conn, s), i, reply, &tx, &rec);
                }
                None => respond(s, i, reply, &tx, &rec),
            });
        }
    });
    Served { url, seen, heads }
}

/// Reads one request head, records it, writes `reply`, then reads until the client closes.
/// A failed TLS handshake surfaces as a read error: nothing is recorded.
pub fn respond(
    mut s: impl Read + Write,
    i: usize,
    reply: Option<&str>,
    tx: &Sender<Seen>,
    heads: &Lines,
) {
    let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    heads
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(&head).into_owned());
    let _ = tx.send(Seen::Head(i));
    if let Some(r) = reply {
        let _ = s.write_all(r.as_bytes());
        let _ = s.flush();
    }
    while let Ok(1..) = s.read(&mut buf) {}
    let _ = tx.send(Seen::Closed(i));
}

/// A KTD5 PEM fixture from `tests/fixtures/`.
pub fn pem(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// TLS config for `serve`: `server.pem` (SAN localhost, 127.0.0.1, ::1); with `mtls`, a
/// client certificate signed by `ca.pem` is required.
pub fn tls(mtls: bool) -> Arc<ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .unwrap();
    let builder = if mtls {
        let mut roots = RootCertStore::empty();
        for c in CertificateDer::pem_slice_iter(pem("ca.pem").as_bytes()) {
            roots.add(c.unwrap()).unwrap();
        }
        let verifier = WebPkiClientVerifier::builder_with_provider(roots.into(), provider)
            .build()
            .unwrap();
        builder.with_client_cert_verifier(verifier)
    } else {
        builder.with_no_client_auth()
    };
    let certs = CertificateDer::pem_slice_iter(pem("server.pem").as_bytes())
        .map(Result::unwrap)
        .collect();
    let key = PrivateKeyDer::from_pem_slice(pem("server-key.pem").as_bytes()).unwrap();
    Arc::new(builder.with_single_cert(certs, key).unwrap())
}

/// An http proxy (the Rust twin of `test/helpers/stub-proxy.js`): tunnels `CONNECT` and
/// forwards absolute-form requests to their authority. Records every request head.
pub fn proxy() -> (String, Lines) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let heads = Lines::default();
    let rec = Arc::clone(&heads);
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let (Ok(mut client), rec) = (s, Arc::clone(&rec)) else {
                return;
            };
            std::thread::spawn(move || {
                let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match client.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&head).into_owned();
                rec.lock().unwrap().push(text.clone());
                let target = text.split(' ').nth(1).unwrap();
                let connect = text.starts_with("CONNECT ");
                let authority = if connect {
                    target
                } else {
                    let rest = target.trim_start_matches("http://");
                    rest.split('/').next().unwrap()
                };
                let mut upstream = std::net::TcpStream::connect(authority).unwrap();
                if connect {
                    client
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .unwrap();
                } else {
                    upstream.write_all(&head).unwrap();
                }
                let (mut c2, mut u2) = (client.try_clone().unwrap(), upstream.try_clone().unwrap());
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut u2, &mut c2);
                    let _ = c2.shutdown(std::net::Shutdown::Write);
                });
                let _ = std::io::copy(&mut client, &mut upstream);
                let _ = upstream.shutdown(std::net::Shutdown::Write);
            });
        }
    });
    (url, heads)
}

pub async fn next(seen: &Receiver<Seen>) -> Seen {
    let mut got = None;
    settle(|| {
        got = seen.try_recv().ok();
        got.is_some()
    })
    .await;
    got.unwrap()
}

/// A listening socket path (unix socket, or named pipe on Windows) and its guard.
#[cfg(unix)]
pub fn listening_socket(name: &str) -> (String, impl Drop) {
    struct Guard(String, #[allow(dead_code)] std::os::unix::net::UnixListener);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path = temp(name);
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    (path.clone(), Guard(path, listener))
}

/// Each client open takes one pipe instance, so several are held for repeated checks.
#[cfg(windows)]
pub fn listening_socket(name: &str) -> (String, impl Drop) {
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    struct Guard(#[allow(dead_code)] Vec<NamedPipeServer>);
    impl Drop for Guard {
        fn drop(&mut self) {}
    }
    let path = format!(r"\\.\pipe\wait-on-loop-{}-{name}", std::process::id());
    // ponytail: 8 instances outlast every suite's check count; a pool refill if one ever needs more
    let servers = (0..8)
        .map(|_| ServerOptions::new().create(&path).unwrap())
        .collect();
    (path, Guard(servers))
}

/// A socket path nothing listens on (a temp path, or an unused pipe name on Windows).
pub fn missing_socket(name: &str) -> String {
    if cfg!(windows) {
        format!(
            r"\\.\pipe\wait-on-loop-missing-{}-{name}",
            std::process::id()
        )
    } else {
        temp(name)
    }
}

/// An http server on a unix socket answering every request with `reply`; records the heads.
#[cfg(unix)]
pub fn socket_server(name: &str, reply: &'static str) -> (String, Lines, impl Drop) {
    struct Rm(String);
    impl Drop for Rm {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path = temp(name);
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let heads = Lines::default();
    let rec = Arc::clone(&heads);
    let (tx, _) = channel();
    std::thread::spawn(move || {
        for (i, s) in listener.incoming().enumerate() {
            let (Ok(s), tx, rec) = (s, tx.clone(), Arc::clone(&rec)) else {
                return;
            };
            std::thread::spawn(move || respond(s, i, Some(reply), &tx, &rec));
        }
    });
    (path.clone(), heads, Rm(path))
}

/// The same on a named pipe: a tokio task hands each client its own pipe instance.
#[cfg(windows)]
pub fn socket_server(name: &str, reply: &'static str) -> (String, Lines, impl Drop) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let path = format!(r"\\.\pipe\wait-on-http-{}-{name}", std::process::id());
    let heads = Lines::default();
    let rec = Arc::clone(&heads);
    let p = path.clone();
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&path)
        .unwrap();
    let task = tokio::spawn(async move {
        while server.connect().await.is_ok() {
            let next = ServerOptions::new().create(&p).unwrap();
            let mut s = std::mem::replace(&mut server, next);
            let rec = Arc::clone(&rec);
            tokio::spawn(async move {
                let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                rec.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&head).into_owned());
                let _ = s.write_all(reply.as_bytes()).await;
                while let Ok(1..) = s.read(&mut buf).await {}
            });
        }
    });
    (path, heads, Abort(task))
}

/// A GET resource named by its url, following redirects; `f` sets the other options.
pub fn http_with(url: &str, f: impl FnOnce(&mut HttpOptions)) -> Resource {
    let mut o = HttpOptions {
        url: url.into(),
        method: "GET".into(),
        follow_redirect: true,
        ..Default::default()
    };
    f(&mut o);
    Resource {
        name: url.into(),
        kind: Kind::Http(o),
    }
}

/// The verbose result line for a response.
pub fn result_line(url: &str, status: u16, text: &str, ok: bool) -> String {
    format!("  HTTP(S) result for {url}: {{ status: {status}, statusText: '{text}', ok: {ok} }}")
}

/// The first verbose error line for `url`, once one is logged.
pub fn error_line(lines: &Lines, url: &str) -> Option<String> {
    let prefix = format!("  HTTP(S) error for {url} ");
    text(lines).into_iter().find(|l| l.starts_with(&prefix))
}

/// Spawns `wait(s)`, runs its first tick and `settle`s until `done`; the caller then
/// awaits the run (ready) or `advance`s to the timeout first.
pub async fn first_check(
    s: WaitSpec,
    sink: Sink,
    lines: &Lines,
    done: impl FnMut() -> bool,
) -> tokio::task::JoinHandle<Result<(), String>> {
    let run = tokio::spawn(wait_on_core::waiter::wait(s, sink, NONE));
    settle(|| text(lines).len() == 1).await;
    tokio::time::advance(MS).await;
    settle(done).await;
    run
}

/// Runs a not-ready wait to its 300 ms timeout once a line starting with `line` is logged.
pub async fn times_out(r: Resource, reverse: bool, line: &str) {
    let url = r.name.clone();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![r]);
    s.reverse = reverse;
    s.timeout = Some(300 * MS);
    let run = first_check(s, sink, &lines, || logged(&lines, line)).await;
    tokio::time::advance(300 * MS).await;
    let out = run.await.unwrap();
    assert_eq!(out, Err(format!("Timed out waiting for: {url}")));
}

/// Runs a wait to `Ok` once a line starting with `line` is logged.
pub async fn ready(r: Resource, reverse: bool, line: &str) {
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![r]);
    s.reverse = reverse;
    let run = first_check(s, sink, &lines, || logged(&lines, line)).await;
    assert_eq!(run.await.unwrap(), Ok(()));
}
