use super::*;
use crate::http::NoValidate;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{Instant, advance};

const MS: Duration = Duration::from_millis(1);

type Lines = Arc<Mutex<Vec<(Duration, String)>>>;

/// A sink recording each line with the virtual time since `t0`.
fn recorder(verbose: bool) -> (Sink, Lines) {
    let lines = Lines::default();
    let rec = Arc::clone(&lines);
    let t0 = Instant::now();
    let log: LogFn = Box::new(move |line| {
        rec.lock().unwrap().push((t0.elapsed(), line));
        Box::pin(async {})
    });
    (
        Sink {
            log: Some(log),
            verbose,
        },
        lines,
    )
}

fn text(lines: &Lines) -> Vec<String> {
    lines
        .lock()
        .unwrap()
        .iter()
        .map(|(_, l)| l.clone())
        .collect()
}

fn spec(resources: Vec<Resource>) -> WaitSpec {
    WaitSpec {
        delay: Duration::ZERO,
        interval: 250 * MS,
        window: 750 * MS,
        tcp_timeout: 300 * MS,
        command_timeout: Duration::ZERO,
        simultaneous: None,
        timeout: None,
        reverse: false,
        resources,
    }
}

fn temp(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("wait-on-waiter-{}-{name}", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

fn file(path: &str) -> Resource {
    Resource {
        name: path.to_string(),
        kind: Kind::File(path.to_string()),
    }
}

#[tokio::test(start_paused = true)]
async fn missing_file_times_out_naming_it_with_the_stat_lines() {
    let path = temp("missing");
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&path)]);
    s.timeout = Some(300 * MS);
    let run = tokio::spawn(wait(s, sink, None::<Arc<NoValidate>>));
    advance(300 * MS).await;
    let out = run.await.unwrap();
    assert_eq!(out, Err(format!("Timed out waiting for: {path}")));
    let lines = text(&lines);
    assert_eq!(lines[0], format!("waiting for 1 resources: {path}"));
    assert!(lines.len() > 1, "{lines:?}");
    for l in &lines[1..] {
        assert_eq!(l, &format!("checking file stat for file:{path} ..."));
    }
}

const NONE: Option<Arc<NoValidate>> = None;

fn timed(lines: &Lines) -> Vec<(Duration, String)> {
    lines.lock().unwrap().clone()
}

fn tcp(port: u16) -> Resource {
    Resource {
        name: format!("tcp:127.0.0.1:{port}"),
        kind: Kind::Tcp {
            path: format!("127.0.0.1:{port}"),
            host: "127.0.0.1".into(),
            port,
        },
    }
}

fn closed_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A temp file holding `size` bytes, removed on drop.
struct TempFile(String);

impl TempFile {
    fn new(name: &str, size: usize) -> Self {
        let f = TempFile(temp(name));
        f.write(size);
        f
    }
    fn write(&self, size: usize) {
        std::fs::write(&self.0, "x".repeat(size)).unwrap();
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Yields (keeping the paused clock from auto-advancing) until `done`, for real I/O and
/// `spawn_blocking` work to land. Panics after 5 s of real time.
async fn settle(mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "never settled");
        tokio::task::yield_now().await;
    }
}

/// Keeps the runtime busy for `ms` of real time (the paused clock does not move) so a
/// `spawn_blocking` stat or a local socket finishes; headroom for the real blocking pool.
async fn breathe(ms: u64) {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        tokio::task::yield_now().await;
    }
}

fn has(lines: &Lines, line: &str) -> bool {
    text(lines).iter().any(|l| l == line)
}

#[tokio::test(start_paused = true)]
async fn two_ready_resources_log_one_line_per_flip_except_the_last() {
    let f = TempFile::new("two-ready", 3);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![file(&f.0), tcp(port)]);
    s.window = Duration::ZERO;
    s.tcp_timeout = Duration::ZERO;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let lines = text(&lines);
    let name = format!("tcp:127.0.0.1:{port}");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(
        lines[0],
        format!("waiting for 2 resources: {}, {name}", f.0)
    );
    assert!(
        lines[1] == format!("waiting for 1 resources: {}", f.0)
            || lines[1] == format!("waiting for 1 resources: {name}"),
        "{lines:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn missing_file_without_verbose_logs_only_the_waiting_line() {
    let path = temp("missing-quiet");
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![file(&path)]);
    s.timeout = Some(300 * MS);
    let out = wait(s, sink, NONE).await;
    assert_eq!(out, Err(format!("Timed out waiting for: {path}")));
    assert_eq!(
        text(&lines),
        vec![format!("waiting for 1 resources: {path}")]
    );
}

#[tokio::test(start_paused = true)]
async fn timeout_at_or_before_the_first_tick_times_out_before_any_check() {
    let f = TempFile::new("early-timeout", 1);
    for (delay, timeout) in [(0, 0), (0, 1), (50, 50)] {
        let (sink, lines) = recorder(true);
        let mut s = spec(vec![file(&f.0)]);
        s.delay = delay * MS;
        s.timeout = Some(timeout * MS);
        let out = wait(s, sink, NONE).await;
        assert_eq!(out, Err(format!("Timed out waiting for: {}", f.0)));
        assert_eq!(
            text(&lines),
            vec![format!("waiting for 1 resources: {}", f.0)]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn no_timeout_waits_until_ready() {
    let f = TempFile::new("no-timeout", 1);
    let (sink, _) = recorder(false);
    assert_eq!(wait(spec(vec![file(&f.0)]), sink, NONE).await, Ok(()));
}

#[tokio::test(start_paused = true)]
async fn without_a_log_function_the_outcome_is_unchanged() {
    let f = TempFile::new("no-log", 1);
    for verbose in [false, true] {
        let sink = Sink { log: None, verbose };
        assert_eq!(wait(spec(vec![file(&f.0)]), sink, NONE).await, Ok(()));
    }
}

#[tokio::test(start_paused = true)]
async fn first_tick_is_at_delay_then_every_interval() {
    let f = TempFile::new("delay", 2);
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&f.0)]);
    s.delay = 500 * MS;
    s.interval = 100 * MS;
    s.window = Duration::ZERO;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let p = &f.0;
    assert_eq!(
        timed(&lines),
        vec![
            (Duration::ZERO, format!("waiting for 1 resources: {p}")),
            (500 * MS, format!("checking file stat for file:{p} ...")),
            (
                500 * MS,
                format!("  file exists, checking for size changes, size:2 file:{p}")
            ),
            (600 * MS, format!("checking file stat for file:{p} ...")),
            (600 * MS, format!("  file stabilized at size:2 file:{p}")),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn interval_zero_ticks_every_millisecond() {
    let f = TempFile::new("interval-0", 1);
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&f.0)]);
    s.interval = Duration::ZERO;
    s.window = 5 * MS;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let stats: Vec<Duration> = timed(&lines)
        .into_iter()
        .filter(|(_, l)| l.starts_with("checking file stat"))
        .map(|(t, _)| t)
        .collect();
    assert_eq!(stats, (1..=6).map(|n| n * MS).collect::<Vec<_>>());
}

#[tokio::test(start_paused = true)]
async fn growing_file_stabilizes_one_window_after_its_last_change() {
    let f = TempFile::new("growing", 1);
    let p = f.0.clone();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&p)]);
    s.interval = 50 * MS;
    s.window = 200 * MS;
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    let changed = |n: usize| format!("  file exists, checking for size changes, size:{n} file:{p}");
    advance(MS).await;
    settle(|| has(&lines, &changed(1))).await;
    f.write(2);
    advance(50 * MS).await;
    settle(|| has(&lines, &changed(2))).await;
    f.write(3);
    advance(50 * MS).await;
    settle(|| has(&lines, &changed(3))).await;
    // a -1 read (file briefly gone) keeps the scan state silently
    std::fs::remove_file(&p).unwrap();
    let stats = || {
        text(&lines)
            .iter()
            .filter(|l| l.starts_with("checking"))
            .count()
    };
    advance(50 * MS).await;
    settle(|| stats() == 4).await;
    breathe(50).await;
    f.write(3);
    assert_eq!(run.await.unwrap(), Ok(()));
    let during =
        format!("  file exists, checking for size change during stability window, size:3 file:{p}");
    let stable = format!("  file stabilized at size:3 file:{p}");
    let verbose: Vec<(Duration, String)> = timed(&lines)
        .into_iter()
        .filter(|(_, l)| l.starts_with("  "))
        .collect();
    assert_eq!(
        verbose,
        vec![
            (MS, changed(1)),
            (51 * MS, changed(2)),
            (101 * MS, changed(3)),
            (201 * MS, during.clone()),
            (251 * MS, during),
            (301 * MS, stable),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn reverse_file_is_ready_once_removed_with_no_window() {
    let f = TempFile::new("reverse", 1);
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&f.0)]);
    s.reverse = true;
    s.interval = 100 * MS;
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| text(&lines).len() == 2).await;
    advance(100 * MS).await;
    settle(|| text(&lines).len() == 3).await;
    advance(49 * MS).await;
    std::fs::remove_file(&f.0).unwrap();
    assert_eq!(run.await.unwrap(), Ok(()));
    let lines = timed(&lines);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[3].0, 201 * MS);
    assert!(
        lines[1..]
            .iter()
            .all(|(_, l)| l.starts_with("checking file stat"))
    );
}

#[tokio::test(start_paused = true)]
async fn reverse_tcp_times_out_while_listening_and_is_ready_when_closed() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sink, _) = recorder(false);
    let mut s = spec(vec![tcp(port)]);
    s.reverse = true;
    s.tcp_timeout = Duration::ZERO;
    s.timeout = Some(300 * MS);
    let out = wait(s, sink, NONE).await;
    assert_eq!(
        out,
        Err(format!("Timed out waiting for: tcp:127.0.0.1:{port}"))
    );

    let port = closed_port();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![tcp(port)]);
    s.reverse = true;
    s.tcp_timeout = Duration::ZERO;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let lines = text(&lines);
    assert_eq!(
        lines[1],
        format!("making TCP connection to 127.0.0.1:{port} ...")
    );
    let refused = format!("  error connecting to TCP host:127.0.0.1 port:{port} ");
    assert!(lines.iter().any(|l| l.starts_with(&refused)), "{lines:?}");
}

#[tokio::test(start_paused = true)]
async fn tcp_verbose_lines_for_success_and_connect_timeout() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![tcp(port)]);
    s.tcp_timeout = Duration::ZERO;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    // real I/O under the paused clock: more ticks may start before the connect lands
    let ok = format!("  TCP connection successful to host:127.0.0.1 port:{port}");
    assert!(has(&lines, &ok), "{:?}", text(&lines));

    // A black-holed address: the paused clock fires tcpTimeout, unless the host has no
    // route and fails at once (same fallback as tcp::tests::times_out_within_bound).
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![Resource {
        name: "tcp:10.255.255.1:9".into(),
        kind: Kind::Tcp {
            path: "10.255.255.1:9".into(),
            host: "10.255.255.1".into(),
            port: 9,
        },
    }]);
    s.timeout = Some(400 * MS);
    assert!(wait(s, sink, NONE).await.is_err());
    let lines = text(&lines);
    assert!(
        lines.iter().any(|l| l
            == "  timed out connecting to TCP host:10.255.255.1 port:9 tcpTimeout:300ms"
            || l.starts_with("  error connecting to TCP host:10.255.255.1 port:9 ")),
        "{lines:?}"
    );
}

const OK_CLOSE: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const OK_KEEP: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
const FAIL_CLOSE: &str =
    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Per-connection script: answer at once, or hold until `gate` fires (then answer 500).
#[derive(Clone, Copy)]
enum Step {
    Now(&'static str),
    Hold,
}

#[derive(Debug, PartialEq)]
enum Seen {
    Head(usize),
    Closed(usize),
}

/// Accepts every connection on its own thread (so concurrent checks are observable).
/// Reports each request head and each client-side close.
struct Server {
    url: String,
    seen: std::sync::mpsc::Receiver<Seen>,
    gate: std::sync::mpsc::Sender<()>,
}

fn server(script: Vec<Step>) -> Server {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (seen_tx, seen) = std::sync::mpsc::channel();
    let (gate, gate_rx) = std::sync::mpsc::channel::<()>();
    let gate_rx = Arc::new(Mutex::new(gate_rx));
    std::thread::spawn(move || {
        for (i, step) in script.into_iter().enumerate() {
            let Ok((mut s, _)) = listener.accept() else {
                return;
            };
            let (seen_tx, gate_rx) = (seen_tx.clone(), Arc::clone(&gate_rx));
            std::thread::spawn(move || {
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = seen_tx.send(Seen::Head(i));
                let reply = match step {
                    Step::Now(r) => r,
                    Step::Hold => match gate_rx.lock().unwrap().recv() {
                        Ok(()) => FAIL_CLOSE,
                        Err(_) => return,
                    },
                };
                let _ = s.write_all(reply.as_bytes());
                while let Ok(1..) = s.read(&mut buf) {}
                let _ = seen_tx.send(Seen::Closed(i));
            });
        }
    });
    Server { url, seen, gate }
}

impl Server {
    async fn next(&self) -> Seen {
        let mut got = None;
        settle(|| {
            got = self.seen.try_recv().ok();
            got.is_some()
        })
        .await;
        got.unwrap()
    }
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

#[tokio::test(start_paused = true)]
async fn simultaneous_one_queues_ticks_and_starts_one_per_completion() {
    let srv = server(vec![Step::Hold, Step::Now(OK_CLOSE)]);
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![http(&srv.url, "HEAD")]);
    s.interval = 100 * MS;
    s.simultaneous = Some(1);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    assert_eq!(srv.next().await, Seen::Head(0));
    advance(100 * MS).await;
    advance(100 * MS).await;
    breathe(100).await;
    assert!(
        srv.seen.try_recv().is_err(),
        "a second check ran concurrently"
    );
    let before = Instant::now();
    srv.gate.send(()).unwrap();
    assert_eq!(srv.next().await, Seen::Closed(0));
    // the queued tick starts on the completion, not on the next tick
    assert_eq!(srv.next().await, Seen::Head(1));
    assert_eq!(Instant::now(), before);
    assert_eq!(run.await.unwrap(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn unlimited_simultaneous_starts_a_check_on_every_tick() {
    let srv = server(vec![Step::Hold, Step::Hold, Step::Hold]);
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![http(&srv.url, "HEAD")]);
    s.interval = 100 * MS;
    s.timeout = Some(250 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    assert_eq!(srv.next().await, Seen::Head(0));
    advance(100 * MS).await;
    assert_eq!(srv.next().await, Seen::Head(1));
    advance(100 * MS).await;
    assert_eq!(srv.next().await, Seen::Head(2));
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {}", srv.url))
    );
}

#[tokio::test(start_paused = true)]
async fn first_ready_check_latches_cancels_the_checker_and_stops_ticking() {
    let srv = server(vec![Step::Hold, Step::Now(OK_KEEP), Step::Now(OK_KEEP)]);
    let missing = temp("latch-missing");
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http(&srv.url, "HEAD"), file(&missing)]);
    s.interval = 100 * MS;
    s.timeout = Some(1000 * MS);
    let statuses = Arc::new(Mutex::new(vec![]));
    let seen = Arc::clone(&statuses);
    let validate = Arc::new(move |status: u16| {
        seen.lock().unwrap().push(status);
        std::future::ready(Ok::<_, ()>(status == 200))
    });
    let run = tokio::spawn(wait(s, sink, Some(validate)));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    assert_eq!(srv.next().await, Seen::Head(0));
    advance(100 * MS).await;
    assert_eq!(srv.next().await, Seen::Head(1));
    // the pooled keep-alive socket closes at the latch, while the file still waits
    assert_eq!(srv.next().await, Seen::Closed(1));
    let out = run.await.unwrap();
    // only the unready resource is named
    assert_eq!(out, Err(format!("Timed out waiting for: {missing}")));
    assert!(srv.seen.try_recv().is_err(), "a check ran after the latch");
    assert_eq!(*statuses.lock().unwrap(), vec![200]);
    let url = &srv.url;
    let http_lines: Vec<String> = text(&lines)
        .into_iter()
        .filter(|l| l.contains("HTTP(S)"))
        .collect();
    assert_eq!(
        http_lines,
        vec![
            format!("making HTTP(S) HEAD request to  url:{url} ..."),
            format!("making HTTP(S) HEAD request to  url:{url} ..."),
            format!("  HTTP(S) result for {url}: {{ status: 200, statusText: 'OK', ok: true }}"),
        ]
    );
    assert!(has(&lines, &format!("waiting for 1 resources: {missing}")));
}

#[tokio::test(start_paused = true)]
async fn reverse_http_is_ready_when_refused_with_the_error_line() {
    let url = format!("http://127.0.0.1:{}/", closed_port());
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http(&url, "GET")]);
    s.reverse = true;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let lines = text(&lines);
    assert_eq!(
        lines[1],
        format!("making HTTP(S) GET request to  url:{url} ...")
    );
    let error = format!("  HTTP(S) error for {url} ");
    assert!(lines.iter().any(|l| l.starts_with(&error)), "{lines:?}");
}

#[tokio::test(start_paused = true)]
async fn checker_construction_error_returns_before_any_line() {
    let mut r = http("http://127.0.0.1:1/", "HEAD");
    let Kind::Http(opts) = &mut r.kind else {
        unreachable!()
    };
    opts.proxy = Some("::nope".into());
    let expected = HttpChecker::new(opts.clone()).err().unwrap();
    let (sink, lines) = recorder(true);
    let out = wait(spec(vec![file(&temp("unused")), r]), sink, NONE).await;
    assert_eq!(out, Err(expected));
    assert!(text(&lines).is_empty());
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn http_over_a_unix_socket_names_the_socket_path() {
    use std::io::{Read, Write};
    let path = temp("http.sock");
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = s.write_all(OK_CLOSE.as_bytes());
        }
    });
    let url = "http://localhost/ready";
    let r = Resource {
        name: format!("http://unix:{path}:/ready"),
        kind: Kind::Http(HttpOptions {
            url: url.into(),
            method: "GET".into(),
            follow_redirect: true,
            socket_path: Some(path.clone()),
            ..Default::default()
        }),
    };
    let (sink, lines) = recorder(true);
    let out = wait(spec(vec![r]), sink, NONE).await;
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, Ok(()));
    // real I/O under the paused clock: more ticks may start before the first reply lands
    let making = format!("making HTTP(S) GET request to socketPath:{path} url:{url} ...");
    let result =
        format!("  HTTP(S) result for {url}: {{ status: 200, statusText: 'OK', ok: true }}");
    assert!(
        has(&lines, &making) && has(&lines, &result),
        "{:?}",
        text(&lines)
    );
}

fn socket(path: &str) -> Resource {
    Resource {
        name: format!("socket:{path}"),
        kind: Kind::Socket(path.to_string()),
    }
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
    let path = format!(r"\\.\pipe\wait-on-waiter-{}-{name}", std::process::id());
    let server = tokio::net::windows::named_pipe::ServerOptions::new()
        .create(&path)
        .unwrap();
    (path, Guard(server))
}

#[tokio::test(start_paused = true)]
async fn socket_lines_for_connected_and_missing() {
    let (path, _guard) = listening_socket("ready.sock");
    let (sink, lines) = recorder(true);
    assert_eq!(wait(spec(vec![socket(&path)]), sink, NONE).await, Ok(()));
    assert_eq!(
        text(&lines)[1..],
        [
            format!("making socket connection to {path} ..."),
            format!("  connected to socket:{path}"),
        ]
    );

    let missing = if cfg!(windows) {
        format!(r"\\.\pipe\wait-on-waiter-missing-{}", std::process::id())
    } else {
        temp("missing.sock")
    };
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![socket(&missing)]);
    s.reverse = true;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let error = format!("  error connecting to socket socket:{missing} ");
    let lines = text(&lines);
    assert!(lines.iter().any(|l| l.starts_with(&error)), "{lines:?}");
}

fn command(c: &str) -> Resource {
    Resource {
        name: format!("command:{c}"),
        kind: Kind::Command(c.to_string()),
    }
}

const WIN: bool = cfg!(windows);
const NL: &str = if WIN { "\r\n" } else { "\n" };
const FAIL3: &str = if WIN { "exit /b 3" } else { "exit 3" };
const SLOW_OK: &str = if WIN {
    "ping -n 2 127.0.0.1 >nul"
} else {
    "sleep 0.3"
};

#[tokio::test(start_paused = true)]
async fn command_lines_for_success_and_failure() {
    let (sink, lines) = recorder(true);
    assert_eq!(
        wait(spec(vec![command("echo hi")]), sink, NONE).await,
        Ok(())
    );
    assert_eq!(
        text(&lines)[1..],
        [
            "executing command \"echo hi\" ...".to_string(),
            format!("  Command \"echo hi\" success. stdout: \"hi{NL}\""),
        ]
    );

    let (sink, lines) = recorder(true);
    let mut s = spec(vec![command(FAIL3)]);
    s.reverse = true;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    assert_eq!(
        text(&lines)[2],
        format!("  Command error: \"Command failed: {FAIL3}\n\"")
    );
}

#[tokio::test(start_paused = true)]
async fn command_drops_ticks_while_an_attempt_runs() {
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![command(SLOW_OK)]);
    s.interval = 50 * MS;
    s.simultaneous = Some(5);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| text(&lines).len() == 2).await;
    for _ in 0..3 {
        advance(50 * MS).await;
    }
    assert_eq!(run.await.unwrap(), Ok(()));
    let attempts = text(&lines)
        .iter()
        .filter(|l| l.starts_with("executing command"))
        .count();
    assert_eq!(attempts, 1, "{:?}", text(&lines));
}
