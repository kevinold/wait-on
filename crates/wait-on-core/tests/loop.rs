//! `waiter::wait` end to end through the public API, against real resources under a
//! paused clock. Scenarios already pinned by `src/waiter/tests.rs` are not repeated.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::advance;
use wait_on_core::http::{HttpOptions, NoValidate};
use wait_on_core::waiter::{Kind, LogFn, Resource, Sink, WaitSpec, wait};

const MS: Duration = Duration::from_millis(1);
const NONE: Option<Arc<NoValidate>> = None;
const OK_CLOSE: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

type Lines = Arc<Mutex<Vec<String>>>;

fn recorder(verbose: bool) -> (Sink, Lines) {
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

fn text(lines: &Lines) -> Vec<String> {
    lines.lock().unwrap().clone()
}

fn has(lines: &Lines, line: &str) -> bool {
    text(lines).iter().any(|l| l == line)
}

fn spec(resources: Vec<Resource>) -> WaitSpec {
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

fn temp(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("wait-on-loop-{}-{name}", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

/// Yields (the paused clock stays put) until `done`, so real I/O lands before `advance`.
async fn settle(mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "never settled");
        tokio::task::yield_now().await;
    }
}

/// Keeps the runtime busy for `ms` of real time, giving stray I/O a chance to show up.
async fn breathe(ms: u64) {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        tokio::task::yield_now().await;
    }
}

#[derive(Debug, PartialEq)]
enum Seen {
    Head(usize),
    Closed(usize),
}

/// An http server answering every request with `reply`, or holding it open when `None`.
/// Reports each request head and each client-side close, numbered by connection.
fn server(reply: Option<&'static str>) -> (String, Receiver<Seen>) {
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

async fn next(seen: &Receiver<Seen>) -> Seen {
    let mut got = None;
    settle(|| {
        got = seen.try_recv().ok();
        got.is_some()
    })
    .await;
    got.unwrap()
}

fn http(url: &str, method: &str) -> Resource {
    Resource {
        name: url.to_string(),
        kind: Kind::Http(HttpOptions {
            url: url.to_string(),
            method: method.into(),
            follow_redirect: true,
            ..Default::default()
        }),
    }
}

fn result_line(url: &str, ok: bool) -> String {
    format!("  HTTP(S) result for {url}: {{ status: 200, statusText: 'OK', ok: {ok} }}")
}

#[tokio::test(start_paused = true)]
async fn http_head_and_get_print_result_lines_and_a_refused_url_times_out() {
    let (base, _seen) = server(Some(OK_CLOSE));
    let (head, get) = (format!("{base}/head"), format!("{base}/get"));
    let refused = format!(
        "http://127.0.0.1:{}/",
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    );
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![
        http(&head, "HEAD"),
        http(&get, "GET"),
        http(&refused, "GET"),
    ]);
    s.timeout = Some(300 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    let error = format!("  HTTP(S) error for {refused} ");
    settle(|| {
        let t = text(&lines);
        t.iter()
            .filter(|l| l.starts_with("  HTTP(S) result"))
            .count()
            == 2
            && t.iter().any(|l| l.starts_with(&error))
    })
    .await;
    advance(300 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {refused}"))
    );
    let for_url = |u: &str| -> Vec<String> {
        text(&lines)
            .into_iter()
            .filter(|l| l.contains(&format!("url:{u} ")) || l.contains(&format!("for {u}:")))
            .collect()
    };
    assert_eq!(
        for_url(&head),
        [
            format!("making HTTP(S) HEAD request to  url:{head} ..."),
            result_line(&head, true),
        ]
    );
    assert_eq!(
        for_url(&get),
        [
            format!("making HTTP(S) GET request to  url:{get} ..."),
            result_line(&get, true),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn validate_false_keeps_waiting_until_it_answers_true() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let validate = Arc::new(move |status: u16| {
        let n = counted.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok::<_, ()>(status == 200 && n > 0))
    });
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http(&url, "HEAD")]);
    s.interval = 100 * MS;
    let run = tokio::spawn(wait(s, sink, Some(validate)));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| has(&lines, &result_line(&url, false))).await;
    assert!(!run.is_finished(), "latched on a false verdict");
    advance(100 * MS).await;
    // settle the second check before awaiting, or auto-advance can start a third
    settle(|| has(&lines, &result_line(&url, true))).await;
    assert_eq!(run.await.unwrap(), Ok(()));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let results: Vec<String> = text(&lines)
        .into_iter()
        .filter(|l| l.starts_with("  HTTP(S) result"))
        .collect();
    assert_eq!(results, [result_line(&url, false), result_line(&url, true)]);
}

#[tokio::test(start_paused = true)]
async fn validate_error_is_not_ready() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let validate = Arc::new(|_: u16| std::future::ready(Err::<bool, _>("boom")));
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http(&url, "GET")]);
    s.timeout = Some(300 * MS);
    let run = tokio::spawn(wait(s, sink, Some(validate)));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| has(&lines, &result_line(&url, false))).await;
    advance(300 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {url}"))
    );
}

#[tokio::test(start_paused = true)]
async fn settle_aborts_in_flight_checks_and_stops_ticking() {
    let (url, seen) = server(None);
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![http(&url, "HEAD")]);
    s.interval = 100 * MS;
    s.timeout = Some(150 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    assert_eq!(next(&seen).await, Seen::Head(0));
    advance(100 * MS).await;
    assert_eq!(next(&seen).await, Seen::Head(1));
    advance(49 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {url}"))
    );
    // both hung requests are dropped client-side at the settle
    let mut closed = vec![next(&seen).await, next(&seen).await];
    closed.sort_by_key(|s| format!("{s:?}"));
    assert_eq!(closed, [Seen::Closed(0), Seen::Closed(1)]);
    // and no tick runs after it
    advance(300 * MS).await;
    breathe(100).await;
    assert!(seen.try_recv().is_err(), "a check ran after the settle");
}

const WIN: bool = cfg!(windows);
const SLEEP5: &str = if WIN {
    "ping -n 6 127.0.0.1 >nul"
} else {
    "sleep 5"
};

#[tokio::test(start_paused = true)]
async fn command_runs_once_across_five_ticks_and_is_killed_at_command_timeout() {
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![Resource {
        name: format!("command:{SLEEP5}"),
        kind: Kind::Command(SLEEP5.into()),
    }]);
    s.interval = 20 * MS;
    s.command_timeout = 200 * MS;
    s.timeout = Some(500 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| text(&lines).len() == 2).await;
    for _ in 0..5 {
        advance(20 * MS).await;
    }
    let killed = format!("  Command error: \"Command failed: {SLEEP5}\nkilled after 200ms\"");
    settle(|| has(&lines, &killed)).await;
    assert_eq!(
        text(&lines)[1..],
        [format!("executing command \"{SLEEP5}\" ..."), killed]
    );
    advance(500 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: command:{SLEEP5}"))
    );
}

/// A listening socket path (unix socket, or named pipe on Windows) and its guard.
#[cfg(unix)]
fn listening_socket(name: &str) -> (String, impl Drop) {
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

#[cfg(windows)]
fn listening_socket(name: &str) -> (String, impl Drop) {
    struct Guard(#[allow(dead_code)] tokio::net::windows::named_pipe::NamedPipeServer);
    impl Drop for Guard {
        fn drop(&mut self) {}
    }
    let path = format!(r"\\.\pipe\wait-on-loop-{}-{name}", std::process::id());
    let server = tokio::net::windows::named_pipe::ServerOptions::new()
        .create(&path)
        .unwrap();
    (path, Guard(server))
}

#[tokio::test(start_paused = true)]
async fn mixed_wait_logs_one_waiting_line_per_flip_except_the_last() {
    let file = temp("mixed");
    std::fs::write(&file, "x").unwrap();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let (url, _seen) = server(Some(OK_CLOSE));
    let (sock, _guard) = listening_socket("mixed.sock");
    let resources = vec![
        Resource {
            name: file.clone(),
            kind: Kind::File(file.clone()),
        },
        Resource {
            name: format!("tcp:127.0.0.1:{port}"),
            kind: Kind::Tcp {
                path: format!("127.0.0.1:{port}"),
                host: "127.0.0.1".into(),
                port,
            },
        },
        http(&url, "HEAD"),
        Resource {
            name: format!("socket:{sock}"),
            kind: Kind::Socket(sock.clone()),
        },
        Resource {
            name: "command:echo hi".into(),
            kind: Kind::Command("echo hi".into()),
        },
    ];
    let names: Vec<String> = resources.iter().map(|r| r.name.clone()).collect();
    let (sink, lines) = recorder(false);
    let out = wait(spec(resources), sink, NONE).await;
    let _ = std::fs::remove_file(&file);
    assert_eq!(out, Ok(()));
    let lines = text(&lines);
    assert_eq!(lines.len(), 5, "{lines:?}");
    // each line names the still-unready resources in order, one fewer each flip
    let mut left = names;
    for (k, line) in lines.iter().enumerate() {
        assert_eq!(
            line,
            &format!("waiting for {} resources: {}", 5 - k, left.join(", "))
        );
        if k < 4 {
            let after: Vec<&str> = lines[k + 1]
                .split_once(": ")
                .unwrap()
                .1
                .split(", ")
                .collect();
            left.retain(|n| after.contains(&n.as_str()));
            assert_eq!(left.len(), 4 - k, "{lines:?}");
        }
    }
}
