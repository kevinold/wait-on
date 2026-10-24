//! `https:` through `waiter::wait` against an in-test rustls server on the KTD5 fixtures:
//! TLS as Rust receives it (`roots`, `cert` + `key`, roots absent). `passphrase` and
//! `strictSSL` are resolved in JS and stay JS front-door tests (KD2).

mod common;

use common::*;
use tokio::time::advance;
use wait_on_core::http::{HttpChecker, HttpOptions, NoValidate};

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

/// One check of `url` with `headers`, which must be ready.
async fn check_ok(url: &str, headers: &[(&str, &str)], follow_redirect: bool) {
    let c = HttpChecker::new(HttpOptions {
        url: url.into(),
        method: "GET".into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        follow_redirect,
        timeout_ms: Some(2000),
        ..Default::default()
    })
    .unwrap();
    let out = c.check(None::<NoValidate>).await;
    assert!(out.ok, "{out:?}");
}

/// The accept-encoding of the first request `srv` saw.
fn encoding(srv: &Served) -> Option<String> {
    let heads = text(&srv.heads);
    heads[0]
        .lines()
        .find_map(|l| l.strip_prefix("accept-encoding: "))
        .map(str::to_owned)
}

const HTTP_ENC: &str = "gzip, deflate";
const HTTPS_ENC: &str = "br, gzip, deflate, zstd";

// The JS engine's (fetch's) default accept-encoding, chosen by each hop's scheme.
#[tokio::test]
async fn accept_encoding_by_scheme_without_redirects() {
    let plain = serve(Some(OK_CLOSE), None);
    let secure = serve(Some(OK_CLOSE), Some(tls(false)));
    check_ok(&plain.url, &[], false).await;
    check_ok(&secure.url, &[], false).await;
    assert_eq!(encoding(&plain).as_deref(), Some(HTTP_ENC));
    assert_eq!(encoding(&secure).as_deref(), Some(HTTPS_ENC));
}

#[tokio::test]
async fn accept_encoding_switches_with_the_scheme_on_each_hop() {
    let secure = serve(Some(OK_CLOSE), Some(tls(false)));
    let plain = serve(Some(redirect_to(&format!("{}/ok", secure.url))), None);
    check_ok(&plain.url, &[], true).await;
    assert_eq!(encoding(&plain).as_deref(), Some(HTTP_ENC));
    assert_eq!(encoding(&secure).as_deref(), Some(HTTPS_ENC));

    let plain = serve(Some(OK_CLOSE), None);
    let secure = serve(
        Some(redirect_to(&format!("{}/ok", plain.url))),
        Some(tls(false)),
    );
    check_ok(&secure.url, &[], true).await;
    assert_eq!(encoding(&secure).as_deref(), Some(HTTPS_ENC));
    assert_eq!(encoding(&plain).as_deref(), Some(HTTP_ENC));
}

// A caller accept-encoding replaces the default on every hop; a Range header makes it identity.
#[tokio::test]
async fn caller_accept_encoding_and_range_hold_on_every_hop() {
    for (header, expected) in [("Accept-Encoding", "br"), ("Range", "identity")] {
        let secure = serve(Some(OK_CLOSE), Some(tls(false)));
        let plain = serve(Some(redirect_to(&format!("{}/ok", secure.url))), None);
        check_ok(
            &plain.url,
            &[(header, if header == "Range" { "bytes=0-" } else { "br" })],
            true,
        )
        .await;
        assert_eq!(encoding(&plain).as_deref(), Some(expected), "{header}");
        assert_eq!(encoding(&secure).as_deref(), Some(expected), "{header}");
    }
}
