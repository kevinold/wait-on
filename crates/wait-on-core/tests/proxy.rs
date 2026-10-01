//! An explicit `proxy` URI through `waiter::wait`, against the in-test proxy. The proxy's
//! recorded heads are the path proof. Env proxies and `NO_PROXY` are resolved in JS and
//! stay JS front-door tests (KD2).

mod common;

use common::*;

#[tokio::test(start_paused = true)]
async fn proxy_forward_https_target_tunnels_connect() {
    let srv = serve(Some(OK_CLOSE), Some(tls(false)));
    let (proxy, heads) = proxy();
    let r = http_with(&srv.url, |o| {
        o.roots = Some(vec![pem("ca.pem")]);
        o.proxy = Some(proxy);
    });
    ready(r, false, &result_line(&srv.url, 200, "OK", true)).await;
    let authority = srv.url.trim_start_matches("https://");
    let heads = text(&heads);
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert!(
        heads[0].starts_with(&format!("CONNECT {authority} ")),
        "{heads:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn proxy_forward_userinfo_becomes_basic_auth() {
    let (target, _seen) = server(Some(OK_CLOSE));
    let (proxy, heads) = proxy();
    let r = http_with(&target, |o| {
        o.proxy = Some(proxy.replace("http://", "http://u:p@"))
    });
    ready(r, false, &result_line(&target, 200, "OK", true)).await;
    let heads = text(&heads);
    assert!(
        heads[0].starts_with(&format!("GET {target}/ ")),
        "{heads:?}"
    );
    assert!(
        heads[0].contains("\r\nproxy-authorization: Basic dTpw\r\n"),
        "{heads:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn proxy_reverse_ready_when_proxy_down() {
    let (target, _seen) = server(Some(OK_CLOSE));
    let r = http_with(&target, |o| {
        o.proxy = Some(format!("http://127.0.0.1:{}", closed_port()))
    });
    ready(r, true, &format!("  HTTP(S) error for {target} ")).await;
}
