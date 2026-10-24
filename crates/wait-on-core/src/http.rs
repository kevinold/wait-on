//! HTTP(S) readiness check (HEAD/GET) on reqwest + rustls/ring. No OpenSSL, no env proxy.

use std::future::Future;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT_ENCODING, ACCEPT_LANGUAGE, HeaderMap, HeaderValue, LOCATION};
use reqwest::{Certificate, Client, ClientBuilder, Identity, Method, Proxy, Url, redirect::Policy};
use tokio::sync::watch;

/// Per-resource options, fixed when the checker is built.
#[derive(Debug, Clone, Default)]
pub struct HttpOptions {
    pub url: String,
    /// `"HEAD"` or `"GET"`.
    pub method: String,
    /// Sent verbatim on every request.
    pub headers: Vec<(String, String)>,
    /// `true` follows up to 21 hops; `false` hands the 3xx to the status rule.
    pub follow_redirect: bool,
    /// Whole-request timeout (connect through body); `None` or 0 means no timeout.
    pub timeout_ms: Option<u64>,
    /// PEM bundles to trust exclusively (verified TLS); `None` accepts any cert.
    pub roots: Option<Vec<String>>,
    /// Client certificate PEM; used with `key`.
    pub cert: Option<String>,
    /// Client key PEM (PKCS#1, SEC1 or PKCS#8); used with `cert`.
    pub key: Option<String>,
    /// Proxy URI for every request; `None` connects directly (env is ignored).
    pub proxy: Option<String>,
    /// Unix socket (named pipe on Windows) replacing TCP; the proxy is then ignored.
    pub socket_path: Option<String>,
}

/// One check's result. `ok` is the readiness verdict before any `reverse` negation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpOutcome {
    pub ok: bool,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub error: Option<String>,
}

/// Type to name `None` when there is no validate callback: `check(None::<NoValidate>)`.
pub type NoValidate = fn(u16) -> std::future::Ready<Result<bool, std::convert::Infallible>>;

/// One reqwest `Client` per resource, reused across polls, plus a cancel token.
/// `cancel()` drops the client so its pooled keep-alive sockets close, like the JS
/// path's `dispatcher.close()` in `finalize`.
pub struct HttpChecker {
    client: Mutex<Option<Client>>,
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    follow_redirect: bool,
    /// Covers the whole redirect chain and the final body.
    timeout: Option<Duration>,
    cancel: watch::Sender<bool>,
}

/// 9.x and the JS engine (undici >=8.12): 21 redirects are followed, a 22nd fails.
const MAX_REDIRECTS: usize = 21;
/// undici's redirect handler follows these when they carry a `Location`.
const REDIRECTS: [u16; 6] = [300, 301, 302, 303, 307, 308];

static PROVIDER: Once = Once::new();

impl HttpChecker {
    /// Builds the per-resource client. Errs on an invalid method or an unparsable proxy URI;
    /// never on TLS material (KTD3).
    pub fn new(opts: HttpOptions) -> Result<Self, String> {
        // reqwest with `rustls-no-provider` panics in `build()` without a default provider.
        PROVIDER.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let method = Method::from_bytes(opts.method.as_bytes()).map_err(|e| e.to_string())?;
        let proxy =
            (opts.proxy.as_deref().map(Proxy::all).transpose()).map_err(|e| error_chain(&e))?;
        let base = || {
            let mut b = Client::builder()
                .no_proxy()
                // undici fetch's default; a caller Accept-Language replaces it.
                .default_headers(HeaderMap::from_iter([(
                    ACCEPT_LANGUAGE,
                    HeaderValue::from_static("*"),
                )]))
                // `send` follows redirects itself, hop by hop like undici's redirect handler
                .redirect(Policy::none());
            if let Some(p) = &proxy {
                b = b.proxy(p.clone());
            }
            // Replaces TCP and any proxy (reqwest docs); JS never pairs the two anyway.
            if let Some(path) = opts.socket_path.clone() {
                #[cfg(unix)]
                {
                    b = b.unix_socket(path);
                }
                #[cfg(windows)]
                {
                    b = b.windows_named_pipe(path);
                }
            }
            b
        };
        // Bad TLS material fails every TLS hop, not construction (Node's per-connection error).
        // Only TLS material can fail a build here: the proxy is parsed above, the provider is
        // installed, and an empty trust set builds (`non_pem_roots_are_an_empty_trust_set`).
        let client = with_tls(base(), &opts)
            .and_then(ClientBuilder::build)
            .or_else(|_| base().tls_certs_only([]).build())
            .expect("a client with an empty trust set always builds");
        Ok(Self {
            client: Mutex::new(Some(client)),
            method,
            url: opts.url,
            headers: opts.headers,
            follow_redirect: opts.follow_redirect,
            timeout: opts
                .timeout_ms
                .filter(|ms| *ms > 0)
                .map(Duration::from_millis),
            cancel: watch::channel(false).0,
        })
    }

    /// Sends one request. `validate` decides readiness from the status (a callback
    /// error means not ready); without it the 2xx rule applies. Never panics on
    /// transport errors: they resolve `ok: false` with `error` set.
    pub async fn check<F, Fut, E>(&self, validate: Option<F>) -> HttpOutcome
    where
        F: FnOnce(u16) -> Fut,
        Fut: Future<Output = Result<bool, E>>,
    {
        let mut flag = self.cancel.subscribe();
        tokio::select! {
            out = self.send(validate) => out,
            // Resolves at once when already cancelled; the sender lives in `self`.
            _ = flag.wait_for(|c| *c) => cancelled(),
        }
    }

    /// Settles every in-flight and later `check` as `{ ok: false, error: "cancelled" }`.
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
        self.client.lock().unwrap().take();
    }

    async fn send<F, Fut, E>(&self, validate: Option<F>) -> HttpOutcome
    where
        F: FnOnce(u16) -> Fut,
        Fut: Future<Output = Result<bool, E>>,
    {
        let Some(client) = self.client.lock().unwrap().clone() else {
            return cancelled();
        };
        let deadline = self.timeout.map(|t| Instant::now() + t);
        let mut url = match Url::parse(&self.url) {
            Ok(u) => u,
            Err(e) => return failed(e.to_string()),
        };
        let mut headers = self.headers.clone();
        let sent = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
        let (own_encoding, range) = (sent("accept-encoding"), sent("range"));
        // undici's loop history: the url as first sent, then each hop as pathname + search
        let mut history: Vec<String> = vec![];
        let mut resp = loop {
            let mut req = client.request(self.method.clone(), url.clone());
            for (k, v) in &headers {
                req = req.header(k, v);
            }
            // fetch's accept-encoding, picked per hop from that hop's scheme; never decoded
            if !own_encoding {
                req = req.header(
                    ACCEPT_ENCODING,
                    match (range, url.scheme()) {
                        (true, _) => "identity",
                        (false, "https") => "br, gzip, deflate, zstd",
                        _ => "gzip, deflate",
                    },
                );
            }
            if let Some(d) = deadline {
                req = req.timeout(d.saturating_duration_since(Instant::now()));
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => return not_ready(&e),
            };
            let location = (self.follow_redirect && REDIRECTS.contains(&resp.status().as_u16()))
                .then(|| resp.headers().get(LOCATION))
                .flatten()
                .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned());
            let Some(location) = location else {
                break resp;
            };
            if history.len() >= MAX_REDIRECTS {
                return failed("max redirects".into());
            }
            history.push(if history.is_empty() {
                without_fragment(&url).into()
            } else {
                loop_key(&url)
            });
            let next = match url.join(&location) {
                Ok(u) => u,
                Err(e) => return failed(e.to_string()),
            };
            if history.contains(&loop_key(&next)) {
                return failed(format!(
                    "Redirect loop detected. Cannot redirect to {}.",
                    next.origin().ascii_serialization()
                ));
            }
            // undici's cleanRequestHeaders: host always, credentials when the origin changes
            let cross_origin = next.origin() != url.origin();
            headers.retain(|(k, _)| {
                !k.eq_ignore_ascii_case("host")
                    && !(cross_origin
                        && ["authorization", "cookie", "proxy-authorization"]
                            .iter()
                            .any(|h| k.eq_ignore_ascii_case(h)))
            });
            url = next;
        };
        let status = resp.status();
        let mut out = HttpOutcome {
            ok: match validate {
                Some(f) => f(status.as_u16()).await.unwrap_or(false),
                None => status.is_success(),
            },
            status: Some(status.as_u16()),
            status_text: status.canonical_reason().map(str::to_owned),
            error: None,
        };
        // GET reads the body only when the status passed, under the same timeout.
        if out.ok && self.method == Method::GET {
            // drain chunk by chunk: the body only has to arrive in time, never be kept
            loop {
                match resp.chunk().await {
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(e) => {
                        out.ok = false;
                        out.error = Some(error_chain(&e));
                        break;
                    }
                }
            }
        }
        out
    }
}

/// `roots` → verified TLS over exactly those certs; absent → any cert. `cert`+`key` → identity.
fn with_tls(mut b: ClientBuilder, opts: &HttpOptions) -> reqwest::Result<ClientBuilder> {
    b = match &opts.roots {
        Some(roots) => {
            let mut certs = vec![];
            for pem in roots {
                certs.extend(Certificate::from_pem_bundle(pem.as_bytes())?);
            }
            b.tls_certs_only(certs)
        }
        None => b.tls_danger_accept_invalid_certs(true),
    };
    // ponytail: a lone cert or key is ignored; JS only passes the pair.
    if let (Some(cert), Some(key)) = (&opts.cert, &opts.key) {
        b = b.identity(Identity::from_pem(format!("{cert}\n{key}").as_bytes())?);
    }
    Ok(b)
}

fn without_fragment(u: &Url) -> Url {
    let mut u = u.clone();
    u.set_fragment(None);
    u
}

/// How undici's redirect handler names a hop: pathname + search, so an empty `?` drops.
fn loop_key(u: &Url) -> String {
    let mut u = without_fragment(u);
    if u.query() == Some("") {
        u.set_query(None);
    }
    u.into()
}

fn failed(error: String) -> HttpOutcome {
    HttpOutcome {
        error: Some(error),
        ..Default::default()
    }
}

fn cancelled() -> HttpOutcome {
    HttpOutcome {
        error: Some("cancelled".into()),
        ..Default::default()
    }
}

fn not_ready(e: &reqwest::Error) -> HttpOutcome {
    HttpOutcome {
        error: Some(error_chain(e)),
        ..Default::default()
    }
}

/// reqwest's top-level message is generic ("error sending request"); append the causes.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    /// Scripted reply: raw bytes written `wait` after reading one request head, then
    /// (if `stall`) the connection is held open instead of closed.
    struct Reply {
        raw: String,
        stall: bool,
        wait: Duration,
    }

    fn reply(raw: &str) -> Reply {
        Reply {
            raw: raw.to_string(),
            stall: false,
            wait: Duration::ZERO,
        }
    }

    fn stall(raw: &str) -> Reply {
        Reply {
            stall: true,
            ..reply(raw)
        }
    }

    fn slow(raw: &str, ms: u64) -> Reply {
        Reply {
            wait: Duration::from_millis(ms),
            ..reply(raw)
        }
    }

    /// Serves one connection per reply on an ephemeral port; returns the base URL
    /// and a receiver of the raw request heads the server saw.
    fn serve(script: Vec<Reply>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        (base, serve_on(listener, script))
    }

    fn serve_on(listener: TcpListener, script: Vec<Reply>) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for r in script {
                let Ok((mut s, _)) = listener.accept() else {
                    return;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&head).into_owned());
                thread::sleep(r.wait);
                let _ = s.write_all(r.raw.as_bytes());
                if r.stall {
                    thread::sleep(Duration::from_secs(2));
                }
            }
        });
        rx
    }

    fn checker(
        url: String,
        method: &str,
        follow_redirect: bool,
        timeout_ms: Option<u64>,
    ) -> HttpChecker {
        HttpChecker::new(HttpOptions {
            url,
            method: method.into(),
            headers: vec![],
            follow_redirect,
            timeout_ms,
            ..Default::default()
        })
        .unwrap()
    }

    const CLOSE: &str = "Connection: close\r\n";

    #[tokio::test]
    async fn cancel_closes_the_pooled_keep_alive_connection() {
        // JS parity: finalize closes the undici dispatcher, so no idle socket outlives the resource.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (closed_tx, closed_rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            // keep-alive: a second read returns 0 only when the client closes the socket
            let _ = s.read(&mut buf);
            let _ = closed_tx.send(());
        });
        let c = checker(url, "GET", true, None);
        assert!(c.check(None::<NoValidate>).await.ok);
        c.cancel();
        // poll asynchronously so the runtime can run the connection task that closes the socket
        let deadline = Instant::now() + Duration::from_secs(2);
        while closed_rx.try_recv().is_err() {
            assert!(
                Instant::now() < deadline,
                "the idle pooled connection stayed open after cancel()"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(c);
    }

    #[tokio::test]
    async fn http_reuses_one_connection_across_checks() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = listener.try_clone().unwrap();
        thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut buf = [0u8; 1024];
            for _ in 0..2 {
                let _ = s.read(&mut buf);
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            }
        });
        // a second connection would get no reply: its check times out
        let c = checker(url, "GET", true, Some(1000));
        assert!(c.check(None::<NoValidate>).await.ok);
        assert!(c.check(None::<NoValidate>).await.ok);
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
    }

    #[tokio::test]
    async fn head_200_with_content_length_and_no_body_is_ready() {
        let (base, _rx) = serve(vec![stall("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n")]);
        let c = checker(base, "HEAD", true, Some(1000));
        let out = c.check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(200));
        assert_eq!(out.status_text.as_deref(), Some("OK"));
    }

    #[tokio::test]
    async fn get_204_is_ready_and_404_is_not() {
        let (base, _rx) = serve(vec![
            reply(&format!("HTTP/1.1 204 No Content\r\n{CLOSE}\r\n")),
            reply(&format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n{CLOSE}\r\n"
            )),
        ]);
        let c = checker(base, "GET", true, Some(1000));
        let out = c.check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(204));
        let out = c.check(None::<NoValidate>).await;
        assert!(!out.ok, "{out:?}");
        assert_eq!(out.status, Some(404));
    }

    fn redirect_script() -> Vec<Reply> {
        vec![
            reply(&format!(
                "HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\n{CLOSE}\r\n"
            )),
            reply(&format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n{CLOSE}\r\nhi"
            )),
        ]
    }

    #[tokio::test]
    async fn follow_redirect_true_follows_302_to_200() {
        let (base, rx) = serve(redirect_script());
        let out = checker(base, "GET", true, Some(1000))
            .check(None::<NoValidate>)
            .await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(200));
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            rx.recv_timeout(Duration::from_secs(2))
                .unwrap()
                .starts_with("GET /ok ")
        );
    }

    #[tokio::test]
    async fn follow_redirect_false_reports_the_302() {
        let (base, _rx) = serve(redirect_script());
        let out = checker(base, "GET", false, Some(1000))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert_eq!(out.status, Some(302));
    }

    #[tokio::test]
    async fn validate_callback_decides_readiness_from_the_exact_status() {
        let (base, _rx) = serve(vec![
            reply(&format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n{CLOSE}\r\n"
            )),
            reply(&format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n{CLOSE}\r\n"
            )),
            reply(&format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n{CLOSE}\r\n"
            )),
        ]);
        let c = checker(base, "GET", true, Some(1000));
        let seen = std::sync::Mutex::new(vec![]);
        let out = c
            .check(Some(async |s| {
                seen.lock().unwrap().push(s);
                Ok::<_, ()>(true)
            }))
            .await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(401));
        let out = c
            .check(Some(async |s| {
                seen.lock().unwrap().push(s);
                Ok::<_, ()>(false)
            }))
            .await;
        assert!(!out.ok, "{out:?}");
        assert_eq!(*seen.lock().unwrap(), vec![401, 200]);
        let out = c.check(Some(async |_| Err::<bool, _>("threw"))).await;
        assert!(!out.ok, "callback error must be not ready: {out:?}");
        assert_eq!(out.status, Some(200));
    }

    #[tokio::test]
    async fn timeout_when_server_never_writes() {
        let (base, _rx) = serve(vec![stall("")]);
        let start = Instant::now();
        let out = checker(base, "HEAD", true, Some(50))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(out.error.is_some(), "{out:?}");
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    }

    #[tokio::test]
    async fn get_timeout_covers_a_stalled_body() {
        let (base, _rx) = serve(vec![stall(
            "HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nab",
        )]);
        let start = Instant::now();
        let out = checker(base, "GET", true, Some(50))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(out.error.is_some(), "{out:?}");
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    }

    #[tokio::test]
    async fn connection_refused_is_not_ready() {
        let port = closed_port();
        let out = checker(format!("http://127.0.0.1:{port}"), "HEAD", true, None)
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(out.error.is_some(), "{out:?}");
        assert_eq!(out.status, None);
    }

    #[tokio::test]
    async fn headers_reach_the_server_verbatim() {
        let (base, rx) = serve(vec![reply(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n{CLOSE}\r\n"
        ))]);
        let c = HttpChecker::new(HttpOptions {
            url: base,
            method: "HEAD".into(),
            headers: vec![
                ("x-custom".into(), "keep".into()),
                ("authorization".into(), "Basic dTpw".into()),
            ],
            follow_redirect: true,
            timeout_ms: Some(1000),
            ..Default::default()
        })
        .unwrap();
        assert!(c.check(None::<NoValidate>).await.ok);
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(head.starts_with("HEAD / "), "{head}");
        assert!(head.contains("x-custom: keep\r\n"), "{head}");
        assert!(head.contains("authorization: Basic dTpw\r\n"), "{head}");
    }

    #[tokio::test]
    async fn default_accept_language_yields_to_the_caller() {
        let ok = format!("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n{CLOSE}\r\n");
        let (base, rx) = serve(vec![reply(&ok), reply(&ok)]);
        let head = |headers: Vec<(String, String)>| {
            let c = HttpChecker::new(HttpOptions {
                url: format!("{base}/health?"),
                method: "HEAD".into(),
                headers,
                follow_redirect: true,
                timeout_ms: Some(1000),
                ..Default::default()
            })
            .unwrap();
            let rx = &rx;
            async move {
                assert!(c.check(None::<NoValidate>).await.ok);
                rx.recv_timeout(Duration::from_secs(2)).unwrap()
            }
        };
        let plain = head(vec![]).await;
        assert!(plain.starts_with("HEAD /health? "), "{plain}");
        assert!(plain.contains("accept-language: *\r\n"), "{plain}");
        let own = head(vec![("Accept-Language".into(), "en".into())]).await;
        assert!(own.contains("accept-language: en\r\n"), "{own}");
        assert!(!own.contains("accept-language: *"), "{own}");
    }

    /// `hops` redirects (`/1`, `/2`, ...), then 200.
    fn redirect_chain(hops: usize) -> Vec<Reply> {
        let mut script: Vec<Reply> = (1..=hops)
            .map(|n| {
                reply(&format!(
                    "HTTP/1.1 302 Found\r\nLocation: /{n}\r\nContent-Length: 0\r\n{CLOSE}\r\n"
                ))
            })
            .collect();
        script.push(reply(OK));
        script
    }

    // 9.x parity (follow-redirects maxRedirects 21), as the JS engine on undici >=8.12.
    #[tokio::test]
    async fn redirect_chain_of_21_hops_is_ready() {
        let (base, _rx) = serve(redirect_chain(21));
        let out = checker(base, "GET", true, Some(2000))
            .check(None::<NoValidate>)
            .await;
        assert!(out.ok, "{out:?}");
    }

    #[tokio::test]
    async fn redirect_chain_of_22_hops_is_not_ready() {
        let (base, _rx) = serve(redirect_chain(22));
        let out = checker(base, "GET", true, Some(2000))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(
            out.error.as_deref().unwrap_or("").contains("max redirects"),
            "{out:?}"
        );
    }

    fn redirect(status: &str, location: &str) -> Reply {
        reply(&format!(
            "HTTP/1.1 {status}\r\nLocation: {location}\r\nContent-Length: 0\r\n{CLOSE}\r\n"
        ))
    }

    /// Request heads seen so far, waiting briefly for each.
    fn heads(rx: &mpsc::Receiver<String>, n: usize) -> Vec<String> {
        (0..n)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).unwrap())
            .collect()
    }

    // undici's redirect handler: credentials stay on a same-origin hop and are dropped when
    // the origin changes; no hop gets a Referer.
    #[tokio::test]
    async fn cross_origin_hop_drops_credentials_and_same_origin_keeps_them() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let rx = serve_on(
            listener,
            vec![
                redirect("302 Found", "/same"),
                redirect("302 Found", &format!("http://localhost:{port}/other")),
                reply(OK),
            ],
        );
        let c = HttpChecker::new(HttpOptions {
            url: format!("http://127.0.0.1:{port}/"),
            method: "GET".into(),
            headers: [
                ("authorization", "Basic dTpw"),
                ("cookie", "a=1"),
                ("proxy-authorization", "Basic cDpw"),
                ("x-keep", "1"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .to_vec(),
            follow_redirect: true,
            timeout_ms: Some(2000),
            ..Default::default()
        })
        .unwrap();
        assert!(c.check(None::<NoValidate>).await.ok);
        let seen = heads(&rx, 3);
        for head in &seen[..2] {
            for h in [
                "authorization: Basic dTpw",
                "cookie: a=1",
                "proxy-authorization: Basic cDpw",
            ] {
                assert!(head.contains(h), "{head}");
            }
        }
        assert!(seen[2].starts_with("GET /other "), "{}", seen[2]);
        assert!(seen[2].contains("x-keep: 1\r\n"), "{}", seen[2]);
        for h in ["authorization:", "cookie:", "referer:"] {
            assert!(!seen[2].contains(h), "{}", seen[2]);
        }
        assert!(!seen[1].contains("referer:"), "{}", seen[1]);
    }

    // The timeout covers the whole chain, as reqwest's client timeout and the JS
    // AbortSignal do, not each hop.
    #[tokio::test]
    async fn timeout_covers_the_whole_redirect_chain() {
        let hop = |n| {
            slow(
                &format!(
                    "HTTP/1.1 302 Found\r\nLocation: /{n}\r\nContent-Length: 0\r\n{CLOSE}\r\n"
                ),
                150,
            )
        };
        let (base, _rx) = serve(vec![hop(1), hop(2), hop(3), reply(OK)]);
        let out = checker(base, "GET", true, Some(300))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(out.error.is_some(), "{out:?}");
    }

    #[tokio::test]
    async fn redirect_without_location_is_the_response() {
        let (base, rx) = serve(vec![reply(&format!(
            "HTTP/1.1 302 Found\r\nContent-Length: 0\r\n{CLOSE}\r\n"
        ))]);
        let accept_302: NoValidate = |s| std::future::ready(Ok(s == 302));
        let out = checker(format!("{base}/a?"), "GET", true, Some(1000))
            .check(Some(accept_302))
            .await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(302));
        assert!(heads(&rx, 1)[0].starts_with("GET /a? "));
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    // undici follows 300 like 301/302/303/307/308.
    #[tokio::test]
    async fn multiple_choices_with_location_is_followed() {
        let (base, rx) = serve(vec![redirect("300 Multiple Choices", "/ok"), reply(OK)]);
        let out = checker(base, "GET", true, Some(1000))
            .check(None::<NoValidate>)
            .await;
        assert!(out.ok, "{out:?}");
        assert!(heads(&rx, 2)[1].starts_with("GET /ok "));
    }

    // undici's loop check: a redirect to a URL already in the chain fails. It compares
    // pathname + search, so `/selfq` -> `/selfq?` is a loop too (the JS engine's delta).
    #[tokio::test]
    async fn redirect_to_a_visited_url_is_a_loop() {
        for (start, script) in [
            (
                "/a",
                vec![
                    redirect("302 Found", "/b"),
                    redirect("302 Found", "/a"),
                    reply(OK),
                ],
            ),
            ("/selfq", vec![redirect("302 Found", "/selfq?"), reply(OK)]),
        ] {
            let (base, _rx) = serve(script);
            let out = checker(format!("{base}{start}"), "GET", true, Some(1000))
                .check(None::<NoValidate>)
                .await;
            assert!(!out.ok, "{start}: {out:?}");
            assert!(
                out.error
                    .as_deref()
                    .unwrap_or("")
                    .contains("Redirect loop detected"),
                "{start}: {out:?}"
            );
        }
    }

    #[tokio::test]
    async fn unjoinable_location_and_unparsable_url_are_not_ready() {
        let (base, _rx) = serve(vec![redirect("302 Found", "http://[::1")]);
        let out = checker(base, "GET", true, Some(1000))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok && out.error.is_some(), "{out:?}");
        let out = checker("not a url".into(), "GET", true, None)
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok && out.error.is_some(), "{out:?}");
    }

    fn closed_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn with(url: String, f: impl FnOnce(&mut HttpOptions)) -> Result<HttpChecker, String> {
        let mut o = HttpOptions {
            url,
            method: "GET".into(),
            follow_redirect: true,
            timeout_ms: Some(1000),
            ..Default::default()
        };
        f(&mut o);
        HttpChecker::new(o)
    }

    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    #[tokio::test]
    async fn proxy_receives_absolute_form_requests_for_the_target() {
        let (proxy, rx) = serve(vec![reply(OK)]);
        let target = format!("http://127.0.0.1:{}/ready", closed_port());
        let c = with(target.clone(), |o| o.proxy = Some(proxy)).unwrap();
        let out = c.check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(head.starts_with(&format!("GET {target} ")), "{head}");
        let host = target
            .trim_start_matches("http://")
            .trim_end_matches("/ready");
        assert!(head.contains(&format!("host: {host}\r\n")), "{head}");
    }

    #[tokio::test]
    async fn proxy_userinfo_is_percent_decoded_into_basic_auth() {
        let (proxy, rx) = serve(vec![reply(OK)]);
        let proxy = proxy.replace("http://", "http://u:p%40%3A%2F@");
        let target = format!("http://127.0.0.1:{}/", closed_port());
        let c = with(target, |o| o.proxy = Some(proxy)).unwrap();
        assert!(c.check(None::<NoValidate>).await.ok);
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // base64("u:p@:/")
        assert!(
            head.contains("proxy-authorization: Basic dTpwQDov\r\n"),
            "{head}"
        );
    }

    #[tokio::test]
    async fn without_proxy_the_target_sees_origin_form() {
        let (base, rx) = serve(vec![reply(OK)]);
        assert!(
            with(base, |_| {})
                .unwrap()
                .check(None::<NoValidate>)
                .await
                .ok
        );
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(head.starts_with("GET / "), "{head}");
    }

    #[test]
    fn unparsable_proxy_uri_fails_construction() {
        let err = with("http://127.0.0.1:1/".into(), |o| {
            o.proxy = Some("not a uri".into())
        });
        assert!(err.is_err());
    }

    const BAD_PEM: &str =
        "-----BEGIN CERTIFICATE-----\n!!not base64!!\n-----END CERTIFICATE-----\n";

    /// KTD3: bad TLS material never fails `new`; plain http still works, https does not.
    async fn assert_tls_hops_fail_http_hops_pass(f: impl Fn(&mut HttpOptions) + Copy) {
        let (base, _rx) = serve(vec![reply(OK)]);
        let out = with(base, f).unwrap().check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
        let (base, _rx) = serve(vec![reply(OK)]);
        let https = base.replace("http://", "https://");
        let out = with(https, f).unwrap().check(None::<NoValidate>).await;
        assert!(!out.ok, "{out:?}");
        assert!(!out.error.unwrap_or_default().is_empty());
    }

    #[tokio::test]
    async fn unparsable_roots_fail_only_tls_hops() {
        assert_tls_hops_fail_http_hops_pass(|o| o.roots = Some(vec![BAD_PEM.into()])).await;
    }

    #[tokio::test]
    async fn garbage_identity_fails_only_tls_hops() {
        assert_tls_hops_fail_http_hops_pass(|o| {
            o.cert = Some("garbage".into());
            o.key = Some(BAD_PEM.into());
        })
        .await;
    }

    #[tokio::test]
    async fn non_pem_roots_are_an_empty_trust_set() {
        assert_tls_hops_fail_http_hops_pass(|o| o.roots = Some(vec!["not a pem".into()])).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn socket_path_carries_the_request_and_ignores_the_proxy() {
        use std::os::unix::net::UnixListener;
        let path = std::env::temp_dir().join(format!("wait-on-http-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap();
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            let _ = s.write_all(OK.as_bytes());
        });
        let dead_proxy = format!("http://127.0.0.1:{}", closed_port());
        let c = with("http://localhost/ready".into(), |o| {
            o.socket_path = Some(path.to_string_lossy().into_owned());
            o.proxy = Some(dead_proxy);
        })
        .unwrap();
        let out = c.check(None::<NoValidate>).await;
        let _ = std::fs::remove_file(&path);
        assert!(out.ok, "{out:?}");
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(head.starts_with("GET /ready "), "{head}");
        assert!(head.contains("host: localhost\r\n"), "{head}");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn socket_path_carries_the_request_over_a_named_pipe() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let path = format!(r"\\.\pipe\wait-on-http-{}", std::process::id());
        let mut server = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .create(&path)
            .unwrap();
        let served = tokio::spawn(async move {
            server.connect().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = server.read(&mut buf).await.unwrap();
            server.write_all(OK.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        let c = with("http://localhost/ready".into(), |o| {
            o.socket_path = Some(path.clone())
        })
        .unwrap();
        let out = c.check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
        assert!(served.await.unwrap().starts_with("GET /ready "));
    }

    #[tokio::test]
    async fn missing_socket_path_is_not_ready() {
        #[cfg(unix)]
        let path = std::env::temp_dir()
            .join(format!("wait-on-http-missing-{}.sock", std::process::id()))
            .to_string_lossy()
            .into_owned();
        #[cfg(windows)]
        let path = format!(r"\\.\pipe\wait-on-http-missing-{}", std::process::id());
        let c = with("http://localhost/".into(), |o| o.socket_path = Some(path)).unwrap();
        let out = c.check(None::<NoValidate>).await;
        assert!(!out.ok, "{out:?}");
        assert!(out.error.is_some(), "{out:?}");
    }

    #[tokio::test]
    async fn cancel_settles_in_flight_and_later_checks() {
        let (base, _rx) = serve(vec![stall(""), stall("")]);
        let c = std::sync::Arc::new(checker(base, "HEAD", true, None));
        let inflight = tokio::spawn({
            let c = c.clone();
            async move { c.check(None::<NoValidate>).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        c.cancel();
        let out = tokio::time::timeout(Duration::from_millis(500), inflight)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            out,
            HttpOutcome {
                ok: false,
                error: Some("cancelled".into()),
                ..Default::default()
            }
        );
        let later = c.check(None::<NoValidate>).await;
        assert_eq!(later.error.as_deref(), Some("cancelled"));
        assert!(!later.ok);
        // `check` races `send` against the cancel flag; pin the client-gone arm directly.
        assert_eq!(c.send(None::<NoValidate>).await, cancelled());
    }

    #[tokio::test]
    async fn validate_fn_pointer_overrides_the_2xx_rule() {
        // Same `NoValidate` instantiation as the `None` calls (llvm-cov counts regions per
        // instantiation), so that one instantiation runs both arms of the status rule.
        let (base, _rx) = serve(vec![reply(&format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n{CLOSE}\r\n"
        ))]);
        let accept_401: NoValidate = |s| std::future::ready(Ok(s == 401));
        let out = checker(base, "GET", true, Some(1000))
            .check(Some(accept_401))
            .await;
        assert!(out.ok, "{out:?}");
        assert_eq!(out.status, Some(401));
    }

    #[test]
    fn invalid_method_fails_construction() {
        let err = with("http://127.0.0.1:1/".into(), |o| {
            o.method = "BAD METHOD".into()
        });
        assert_eq!(err.err().as_deref(), Some("invalid HTTP method"));
    }

    #[tokio::test]
    async fn valid_client_identity_builds_and_plain_http_is_ready() {
        let (base, _rx) = serve(vec![reply(OK)]);
        let c = with(base, |o| {
            o.cert = Some(include_str!("../tests/fixtures/client.pem").into());
            o.key = Some(include_str!("../tests/fixtures/client-key.pem").into());
        })
        .unwrap();
        let out = c.check(None::<NoValidate>).await;
        assert!(out.ok, "{out:?}");
    }
}
