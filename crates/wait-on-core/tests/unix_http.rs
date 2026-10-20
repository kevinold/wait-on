//! `http://unix:<sock>:<path>` as Rust receives it (`socket_path`) through `waiter::wait`:
//! a unix socket, or a named pipe on Windows (`common::socket_server` picks per `cfg`).

mod common;

use common::*;

const URL: &str = "http://localhost/ready";

/// `socket_path` replaces TCP and any proxy: the live proxy sees nothing.
async fn forward_ready(name: &str) {
    let (path, heads, _guard) = socket_server(name, OK_CLOSE);
    let (proxy, proxied) = proxy();
    let r = http_with(URL, |o| {
        o.socket_path = Some(path.clone());
        o.proxy = Some(proxy);
    });
    let (sink, lines) = recorder(true);
    let ok = result_line(URL, 200, "OK", true);
    let run = first_check(spec(vec![r]), sink, &lines, || has(&lines, &ok)).await;
    assert_eq!(run.await.unwrap(), Ok(()));
    let request = format!("making HTTP(S) GET request to socketPath:{path} url:{URL} ...");
    assert!(has(&lines, &request), "{:?}", text(&lines));
    let heads = text(&heads);
    assert!(heads[0].starts_with("GET /ready "), "{heads:?}");
    assert!(text(&proxied).is_empty(), "the proxy saw a request");
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn unix_http_forward_ready_over_socket() {
    forward_ready("http-fwd.sock").await;
}

#[cfg(windows)]
#[tokio::test(start_paused = true)]
async fn unix_http_forward_ready_over_named_pipe() {
    forward_ready("http-fwd").await;
}

fn missing(name: &str) -> wait_on_core::waiter::Resource {
    let path = missing_socket(name);
    http_with(URL, |o| o.socket_path = Some(path))
}

#[tokio::test(start_paused = true)]
async fn unix_http_forward_times_out_when_socket_missing() {
    let error = format!("  HTTP(S) error for {URL} ");
    times_out(missing("http-fwd-missing.sock"), false, &error).await;
}

#[tokio::test(start_paused = true)]
async fn unix_http_reverse_ready_when_socket_missing() {
    let error = format!("  HTTP(S) error for {URL} ");
    ready(missing("http-rev-missing.sock"), true, &error).await;
}
