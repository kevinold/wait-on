//! Helpers shared by the integration suites. Each suite compiles its own copy and uses a
//! subset, hence the `dead_code` allow. Not in the coverage report (cargo-llvm-cov skips
//! `tests/`).
#![allow(dead_code)]

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wait_on_core::http::NoValidate;
use wait_on_core::waiter::{LogFn, Resource, Sink, WaitSpec};

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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, seen) = channel();
    std::thread::spawn(move || {
        for (i, s) in listener.incoming().enumerate() {
            let (Ok(mut s), tx) = (s, tx.clone()) else {
                return;
            };
            std::thread::spawn(move || {
                let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = tx.send(Seen::Head(i));
                if let Some(r) = reply {
                    let _ = s.write_all(r.as_bytes());
                }
                while let Ok(1..) = s.read(&mut buf) {}
                let _ = tx.send(Seen::Closed(i));
            });
        }
    });
    (url, seen)
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
