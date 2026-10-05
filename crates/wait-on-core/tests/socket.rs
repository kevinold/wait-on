//! `socket:` through `waiter::wait`, forward and reverse, on a paused clock. A unix socket,
//! or a named pipe on Windows (`common::listening_socket` / `missing_socket` pick per `cfg`).

mod common;

use common::*;
use tokio::time::Instant;
use wait_on_core::waiter::{Kind, Resource, wait};

fn socket(path: &str) -> Resource {
    Resource {
        name: format!("socket:{path}"),
        kind: Kind::Socket(path.to_string()),
    }
}

#[tokio::test(start_paused = true)]
async fn socket_forward_ready_when_listening() {
    let (path, _guard) = listening_socket("fwd.sock");
    let (sink, lines) = recorder(true);
    assert_eq!(wait(spec(vec![socket(&path)]), sink, NONE).await, Ok(()));
    assert!(has(&lines, &format!("  connected to socket:{path}")));
}

#[tokio::test(start_paused = true)]
async fn socket_forward_times_out_when_missing() {
    let path = missing_socket("fwd-missing.sock");
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![socket(&path)]);
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: socket:{path}")));
    assert_eq!(e, 500 * MS);
    let error = format!("  error connecting to socket socket:{path} ");
    let lines = text(&lines);
    assert!(lines.iter().any(|l| l.starts_with(&error)), "{lines:?}");
}

#[tokio::test(start_paused = true)]
async fn socket_reverse_ready_when_missing() {
    let path = missing_socket("rev-missing.sock");
    let (sink, _) = recorder(false);
    let mut s = spec(vec![socket(&path)]);
    s.reverse = true;
    let t0 = Instant::now();
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let e = t0.elapsed();
    assert_eq!(e, MS); // the first tick
}

#[tokio::test(start_paused = true)]
async fn socket_reverse_times_out_while_listening() {
    let (path, _guard) = listening_socket("rev.sock");
    let (sink, _) = recorder(false);
    let mut s = spec(vec![socket(&path)]);
    s.reverse = true;
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: socket:{path}")));
    assert_eq!(e, 500 * MS);
}
