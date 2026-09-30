//! HTTP(S) readiness check (HEAD/GET) on reqwest + rustls/ring. No proxy, no OpenSSL.

use std::future::Future;
use std::sync::{Mutex, Once};
use std::time::Duration;

use reqwest::{Client, Method, redirect::Policy};
use tokio::sync::watch;

/// Per-resource options, fixed when the checker is built.
#[derive(Debug, Clone, Default)]
pub struct HttpOptions {
    pub url: String,
    /// `"HEAD"` or `"GET"`.
    pub method: String,
    /// Sent verbatim on every request.
    pub headers: Vec<(String, String)>,
    /// `true` follows up to 20 hops; `false` hands the 3xx to the status rule.
    pub follow_redirect: bool,
    /// Whole-request timeout (connect through body); `None` or 0 means no timeout.
    pub timeout_ms: Option<u64>,
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
    cancel: watch::Sender<bool>,
}

static PROVIDER: Once = Once::new();

impl HttpChecker {
    /// Builds the per-resource client. Errs only on an invalid method or builder failure.
    pub fn new(opts: HttpOptions) -> Result<Self, String> {
        // reqwest with `rustls-no-provider` panics in `build()` without a default provider.
        PROVIDER.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let method = Method::from_bytes(opts.method.as_bytes()).map_err(|e| e.to_string())?;
        let mut builder = Client::builder()
            .no_proxy()
            // ponytail: strictSSL=false default only; strictSSL=true stays on the JS engine (L5).
            .tls_danger_accept_invalid_certs(true)
            .redirect(if opts.follow_redirect {
                Policy::limited(20) // undici fetch's hop limit
            } else {
                Policy::none()
            });
        if let Some(ms) = opts.timeout_ms.filter(|ms| *ms > 0) {
            builder = builder.timeout(Duration::from_millis(ms));
        }
        Ok(Self {
            client: Mutex::new(Some(builder.build().map_err(|e| error_chain(&e))?)),
            method,
            url: opts.url,
            headers: opts.headers,
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
        let mut req = client.request(self.method.clone(), &self.url);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        let mut resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return not_ready(&e),
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

    /// Scripted reply: raw bytes written after reading one request head, then
    /// (if `stall`) the connection is held open instead of closed.
    struct Reply {
        raw: String,
        stall: bool,
    }

    fn reply(raw: &str) -> Reply {
        Reply {
            raw: raw.to_string(),
            stall: false,
        }
    }

    fn stall(raw: &str) -> Reply {
        Reply {
            raw: raw.to_string(),
            stall: true,
        }
    }

    /// Serves one connection per reply on an ephemeral port; returns the base URL
    /// and a receiver of the raw request heads the server saw.
    fn serve(script: Vec<Reply>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
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
                let _ = s.write_all(r.raw.as_bytes());
                if r.stall {
                    thread::sleep(Duration::from_secs(2));
                }
            }
        });
        (base, rx)
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
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
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
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn connection_refused_is_not_ready() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
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
        })
        .unwrap();
        assert!(c.check(None::<NoValidate>).await.ok);
        let head = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(head.starts_with("HEAD / "), "{head}");
        assert!(head.contains("x-custom: keep\r\n"), "{head}");
        assert!(head.contains("authorization: Basic dTpw\r\n"), "{head}");
    }

    #[tokio::test]
    async fn redirect_chain_over_20_hops_is_not_ready() {
        let hop = format!("HTTP/1.1 302 Found\r\nLocation: /r\r\nContent-Length: 0\r\n{CLOSE}\r\n");
        let (base, _rx) = serve((0..25).map(|_| reply(&hop)).collect());
        let out = checker(base, "GET", true, Some(2000))
            .check(None::<NoValidate>)
            .await;
        assert!(!out.ok, "{out:?}");
        assert!(
            out.error
                .as_deref()
                .unwrap_or("")
                .contains("too many redirects"),
            "{out:?}"
        );
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
    }
}
