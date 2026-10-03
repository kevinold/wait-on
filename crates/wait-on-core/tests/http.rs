//! `http:` / `http-get:` through `waiter::wait`, forward and reverse, on a paused clock:
//! status handling, headers, redirects. Checker internals are pinned in `src/http.rs`.

mod common;

use common::*;

#[tokio::test(start_paused = true)]
async fn http_forward_head_ready_on_200() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let r = http_with(&url, |o| o.method = "HEAD".into());
    ready(r, false, &result_line(&url, 200, "OK", true)).await;
}

#[tokio::test(start_paused = true)]
async fn http_forward_get_ready_on_204() {
    let (url, _seen) = server(Some(reply("204 No Content")));
    let ok = result_line(&url, 204, "No Content", true);
    ready(http_with(&url, |_| {}), false, &ok).await;
}

#[tokio::test(start_paused = true)]
async fn http_forward_get_times_out_on_404() {
    let (url, _seen) = server(Some(reply("404 Not Found")));
    let line = result_line(&url, 404, "Not Found", false);
    times_out(http_with(&url, |_| {}), false, &line).await;
}

#[tokio::test(start_paused = true)]
async fn http_forward_headers_reach_server_verbatim() {
    let srv = serve(Some(OK_CLOSE), None);
    let r = http_with(&srv.url, |o| {
        o.headers = vec![
            ("x-custom".into(), "keep".into()),
            ("authorization".into(), "Basic dTpw".into()),
        ]
    });
    ready(r, false, &result_line(&srv.url, 200, "OK", true)).await;
    let head = text(&srv.heads).remove(0);
    assert!(head.contains("\r\nx-custom: keep\r\n"), "{head}");
    assert!(head.contains("\r\nauthorization: Basic dTpw\r\n"), "{head}");
}

#[tokio::test(start_paused = true)]
async fn http_forward_follows_redirect_when_enabled() {
    let target = serve(Some(OK_CLOSE), None);
    let (url, _seen) = server(Some(redirect_to(&format!("{}/ok", target.url))));
    let r = http_with(&url, |o| o.follow_redirect = true);
    ready(r, false, &result_line(&url, 200, "OK", true)).await;
    let heads = text(&target.heads);
    assert!(heads[0].starts_with("GET /ok "), "{heads:?}");
}

#[tokio::test(start_paused = true)]
async fn http_forward_redirect_not_followed_times_out() {
    let target = serve(Some(OK_CLOSE), None);
    let (url, _seen) = server(Some(redirect_to(&target.url)));
    let r = http_with(&url, |o| o.follow_redirect = false);
    times_out(r, false, &result_line(&url, 302, "Found", false)).await;
    assert!(text(&target.heads).is_empty(), "the redirect was followed");
}

#[tokio::test(start_paused = true)]
async fn http_reverse_ready_on_500() {
    let (url, _seen) = server(Some(reply("500 Internal Server Error")));
    let line = result_line(&url, 500, "Internal Server Error", false);
    ready(http_with(&url, |_| {}), true, &line).await;
}

#[tokio::test(start_paused = true)]
async fn http_reverse_times_out_on_200() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let line = result_line(&url, 200, "OK", true);
    times_out(http_with(&url, |_| {}), true, &line).await;
}
