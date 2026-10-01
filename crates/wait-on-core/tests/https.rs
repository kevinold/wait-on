//! `https:` through `waiter::wait` against an in-test rustls server on the KTD5 fixtures:
//! TLS as Rust receives it (`roots`, `cert` + `key`, roots absent). `passphrase` and
//! `strictSSL` are resolved in JS and stay JS front-door tests (KD2).

mod common;

use common::*;
use tokio::time::advance;

fn roots(name: &str) -> Option<Vec<String>> {
    Some(vec![pem(name)])
}

#[tokio::test(start_paused = true)]
async fn https_forward_verified_roots_ready() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let r = http_with(&srv.url, |o| o.roots = roots("ca.pem"));
    ready(r, false, &result_line(&srv.url, 200, "OK", true)).await;
}

#[tokio::test(start_paused = true)]
async fn https_forward_wrong_roots_times_out() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let r = http_with(&srv.url, |o| o.roots = roots("other-ca.pem"));
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![r]);
    s.timeout = Some(300 * MS);
    let run = first_check(s, sink, &lines, || error_line(&lines, &srv.url).is_some()).await;
    advance(300 * MS).await;
    let out = run.await.unwrap();
    assert_eq!(out, Err(format!("Timed out waiting for: {}", srv.url)));
    let e = error_line(&lines, &srv.url).unwrap();
    assert!(e.contains("invalid peer certificate: UnknownIssuer"), "{e}");
}

#[tokio::test(start_paused = true)]
async fn https_forward_no_roots_accepts_any_cert() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let r = http_with(&srv.url, |o| o.roots = None);
    ready(r, false, &result_line(&srv.url, 200, "OK", true)).await;
}

#[tokio::test(start_paused = true)]
async fn https_forward_mtls_identity_ready() {
    let srv = serve(Some(OK_CLOSE), Some(tls(true)));
    let r = http_with(&srv.url, |o| {
        o.roots = roots("ca.pem");
        o.cert = Some(pem("client.pem"));
        o.key = Some(pem("client-key.pem"));
    });
    ready(r, false, &result_line(&srv.url, 200, "OK", true)).await;
    // the server saw the request only after verifying the client certificate
    assert_eq!(text(&srv.heads).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn https_forward_missing_identity_times_out() {
    let srv = serve(Some(OK_CLOSE), Some(tls(true)));
    let r = http_with(&srv.url, |o| o.roots = roots("ca.pem"));
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![r]);
    s.timeout = Some(300 * MS);
    let run = first_check(s, sink, &lines, || error_line(&lines, &srv.url).is_some()).await;
    advance(300 * MS).await;
    let out = run.await.unwrap();
    assert_eq!(out, Err(format!("Timed out waiting for: {}", srv.url)));
    assert!(text(&srv.heads).is_empty(), "the server read a request");
}

#[tokio::test(start_paused = true)]
async fn https_forward_garbage_roots_fail_tls_hop_only() {
    // Unparsable roots build an empty trust set (`tls_certs_only([])`): https never
    // verifies, plain http with the same options is unaffected.
    let tls_srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let (plain, _seen) = server(Some(OK_CLOSE));
    let garbage = |o: &mut wait_on_core::http::HttpOptions| o.roots = Some(vec!["not pem".into()]);
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![
        http_with(&tls_srv.url, garbage),
        http_with(&plain, garbage),
    ]);
    s.timeout = Some(300 * MS);
    let ok = result_line(&plain, 200, "OK", true);
    let run = first_check(s, sink, &lines, || {
        has(&lines, &ok) && error_line(&lines, &tls_srv.url).is_some()
    })
    .await;
    advance(300 * MS).await;
    let out = run.await.unwrap();
    assert_eq!(out, Err(format!("Timed out waiting for: {}", tls_srv.url)));
}

#[tokio::test(start_paused = true)]
async fn https_forward_http_redirect_to_https_follows() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let (start, _seen) = server(Some(redirect_to(&format!("{}/ok", srv.url))));
    let r = http_with(&start, |o| o.roots = roots("ca.pem"));
    ready(r, false, &result_line(&start, 200, "OK", true)).await;
    let heads = text(&srv.heads);
    assert!(heads[0].starts_with("GET /ok "), "{heads:?}");
}

#[tokio::test(start_paused = true)]
async fn https_reverse_ready_when_roots_wrong() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let r = http_with(&srv.url, |o| o.roots = roots("other-ca.pem"));
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![r]);
    s.reverse = true;
    let run = first_check(s, sink, &lines, || error_line(&lines, &srv.url).is_some()).await;
    assert_eq!(run.await.unwrap(), Ok(()));
}
