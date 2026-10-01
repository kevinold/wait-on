//! `tcp:` through `waiter::wait`, forward and reverse, on a paused clock.

mod common;

use std::time::Duration;

use common::*;
use tokio::net::TcpListener;
use tokio::time::Instant;
use wait_on_core::waiter::{Kind, Resource, wait};

fn tcp(host: &str, port: u16) -> Resource {
    let path = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    Resource {
        name: format!("tcp:{path}"),
        kind: Kind::Tcp {
            path,
            host: host.into(),
            port,
        },
    }
}

#[tokio::test(start_paused = true)]
async fn tcp_forward_ready_when_listening() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sink, lines) = recorder(true);
    assert_eq!(
        wait(spec(vec![tcp("127.0.0.1", port)]), sink, NONE).await,
        Ok(())
    );
    let ok = format!("  TCP connection successful to host:127.0.0.1 port:{port}");
    assert!(has(&lines, &ok), "{:?}", text(&lines));
}

#[tokio::test(start_paused = true)]
async fn tcp_forward_ipv6_literal_ready() {
    // Skip, don't fail, on a host without IPv6 loopback. This file is outside the coverage
    // report, so the skip arm costs the gate nothing (the inline src twin was removed for that).
    let Ok(listener) = TcpListener::bind("[::1]:0").await else {
        return;
    };
    let port = listener.local_addr().unwrap().port();
    let (sink, lines) = recorder(true);
    let r = tcp("::1", port);
    assert_eq!(r.name, format!("tcp:[::1]:{port}"));
    assert_eq!(wait(spec(vec![r]), sink, NONE).await, Ok(()));
    let ok = format!("  TCP connection successful to host:::1 port:{port}");
    assert!(has(&lines, &ok), "{:?}", text(&lines));
}

#[tokio::test(start_paused = true)]
async fn tcp_forward_times_out_while_connect_pending() {
    // A black-holed address: the connect hangs (or fails at once on a host with no route);
    // the overall timeout, not tcpTimeout, ends the wait.
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![tcp("10.255.255.1", 9)]);
    s.tcp_timeout = Duration::from_secs(30);
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err("Timed out waiting for: tcp:10.255.255.1:9".into()));
    assert_eq!(e, 500 * MS);
    let lines = text(&lines);
    assert!(
        !lines.iter().any(|l| l.contains("tcpTimeout:")),
        "{lines:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn tcp_reverse_ready_when_nothing_listens() {
    // Real-time refusal; Windows takes ~2 s, inside `settle`'s 5 s and with no timeout set.
    let port = closed_port();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![tcp("127.0.0.1", port)]);
    s.reverse = true;
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let refused = format!("  error connecting to TCP host:127.0.0.1 port:{port} ");
    let lines = text(&lines);
    assert!(lines.iter().any(|l| l.starts_with(&refused)), "{lines:?}");
}

#[tokio::test(start_paused = true)]
async fn tcp_reverse_times_out_while_listening() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sink, _) = recorder(false);
    let mut s = spec(vec![tcp("127.0.0.1", port)]);
    s.reverse = true;
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(
        out,
        Err(format!("Timed out waiting for: tcp:127.0.0.1:{port}"))
    );
    assert_eq!(e, 500 * MS);
}
