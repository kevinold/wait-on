//! `@engine` scenarios from `features/` against `waiter::wait` (R17, KTD6). The steps build a
//! `WaitSpec` straight from the scenario's words (KTD7): no resource strings are parsed and no
//! JS default or clamp applies. Servers are copied from `wait-on-core/tests/common/mod.rs`
//! rather than shared, so `wait-on-core` grows no test-only surface.

use std::any::Any;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use cucumber::gherkin::Step;
use cucumber::{StatsWriter, World, given, then, when};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use wait_on_core::http::{HttpOptions, NoValidate};
use wait_on_core::waiter::{Kind, Resource, Sink, WaitSpec, wait};

#[derive(Default, World)]
struct EngineWorld {
    resources: Vec<Resource>,
    /// Listeners and files that must outlive the wait; dropped with the scenario.
    guards: Vec<Box<dyn Any>>,
    outcome: Option<Result<(), String>>,
}

impl std::fmt::Debug for EngineWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineWorld")
            .field("resources", &self.resources)
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl EngineWorld {
    fn add(&mut self, name: String, kind: Kind) {
        self.resources.push(Resource { name, kind });
    }

    /// Each resource's name becomes `<resource n>` (KTD5), as the Node runner does.
    fn normalize(&self, message: &str) -> String {
        let mut out = message.to_string();
        for (i, r) in self.resources.iter().enumerate() {
            out = out.replace(&r.name, &format!("<resource {}>", i + 1));
        }
        out
    }

    fn fails_with(&self, message: &str) {
        match &self.outcome {
            Some(Err(e)) => assert_eq!(self.normalize(e), message),
            other => panic!("expected the wait to fail, got {other:?}"),
        }
    }
}

/// A unique temp path; a unix socket path must stay under ~104 bytes.
fn temp(name: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let file = format!("wait-on-features-{}-{n}-{name}", std::process::id());
    std::env::temp_dir()
        .join(file)
        .to_string_lossy()
        .into_owned()
}

/// Removes a temp path when the scenario ends.
struct Rm(String);
impl Drop for Rm {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn reply(status: u16) -> String {
    format!("HTTP/1.1 {status} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

/// Reads one request head, answers `reply`, then reads until the client closes.
fn respond(mut s: impl Read + Write, reply: &str) {
    let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let _ = s.write_all(reply.as_bytes());
    let _ = s.flush();
    while let Ok(1..) = s.read(&mut buf) {}
}

/// Serves every connection on its own thread; the listener lives as long as the thread.
fn accept_loop<S: Send + 'static>(
    mut accept: impl FnMut() -> std::io::Result<S> + Send + 'static,
    handle: impl Fn(S) + Send + Sync + Clone + 'static,
) {
    std::thread::spawn(move || {
        while let Ok(s) = accept() {
            let handle = handle.clone();
            std::thread::spawn(move || handle(s));
        }
    });
}

/// An http(s) server on 127.0.0.1 answering `status`; returns its port.
fn http_server(status: u16, tls: Option<Arc<ServerConfig>>) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let reply = reply(status);
    accept_loop(
        move || listener.accept().map(|(s, _)| s),
        move |s| match &tls {
            Some(cfg) => {
                let conn = ServerConnection::new(Arc::clone(cfg)).unwrap();
                respond(StreamOwned::new(conn, s), &reply);
            }
            None => respond(s, &reply),
        },
    );
    port
}

fn pem(name: &str) -> String {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wait-on-core/tests/fixtures"
    );
    std::fs::read_to_string(std::path::Path::new(dir).join(name)).unwrap()
}

/// `server.pem` (SAN 127.0.0.1), signed by `ca.pem`.
fn tls() -> Arc<ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let certs = CertificateDer::pem_slice_iter(pem("server.pem").as_bytes())
        .map(Result::unwrap)
        .collect();
    let key = PrivateKeyDer::from_pem_slice(pem("server-key.pem").as_bytes()).unwrap();
    let cfg = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    Arc::new(cfg)
}

/// A socket path nothing listens on: a temp path, or an unused pipe name on Windows.
fn socket_path(name: &str) -> String {
    if cfg!(windows) {
        let n = temp(name).replace(['\\', '/', ':'], "-");
        format!(r"\\.\pipe\{n}")
    } else {
        temp(name)
    }
}

/// A listening unix socket (or named pipe) answering `reply` when set, else just accepting.
#[cfg(unix)]
fn socket_server(world: &mut EngineWorld, reply: Option<String>) -> String {
    let path = socket_path("sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    world.guards.push(Box::new(Rm(path.clone())));
    accept_loop(
        move || listener.accept().map(|(s, _)| s),
        move |s| {
            if let Some(r) = &reply {
                respond(s, r);
            }
        },
    );
    path
}

/// Each client open takes one pipe instance, so a task keeps one waiting for the next.
#[cfg(windows)]
fn socket_server(world: &mut EngineWorld, reply: Option<String>) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ServerOptions;
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let path = socket_path("pipe");
    let p = path.clone();
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&path)
        .unwrap();
    let task = tokio::spawn(async move {
        while server.connect().await.is_ok() {
            let next = ServerOptions::new().create(&p).unwrap();
            let mut s = std::mem::replace(&mut server, next);
            let reply = reply.clone();
            tokio::spawn(async move {
                let (mut head, mut buf) = (Vec::new(), [0u8; 1024]);
                if let Some(r) = reply {
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match s.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    let _ = s.write_all(r.as_bytes()).await;
                }
                while let Ok(1..) = s.read(&mut buf).await {}
            });
        }
    });
    world.guards.push(Box::new(Abort(task)));
    path
}

fn http(url: &str, method: &str, f: impl FnOnce(&mut HttpOptions)) -> Kind {
    let mut o = HttpOptions {
        url: url.into(),
        method: method.into(),
        follow_redirect: true,
        ..Default::default()
    };
    f(&mut o);
    Kind::Http(o)
}

#[given("an existing file")]
fn existing_file(w: &mut EngineWorld) {
    let path = temp("file");
    std::fs::write(&path, "data").unwrap();
    w.guards.push(Box::new(Rm(path.clone())));
    w.add(path.clone(), Kind::File(path));
}

#[given("a missing file")]
fn missing_file(w: &mut EngineWorld) {
    let path = temp("missing");
    w.add(path.clone(), Kind::File(path));
}

fn tcp(w: &mut EngineWorld, host: &str, port: u16) {
    let path = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let kind = Kind::Tcp {
        path: path.clone(),
        host: host.into(),
        port,
    };
    w.add(format!("tcp:{path}"), kind);
}

#[given(regex = r"^a TCP server on (a free port|the IPv6 loopback)$")]
fn tcp_server(w: &mut EngineWorld, on: String) {
    let host = if on == "a free port" {
        "127.0.0.1"
    } else {
        "::1"
    };
    let listener = std::net::TcpListener::bind((host, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    w.guards.push(Box::new(listener));
    tcp(w, host, port);
}

#[given("nothing listening on a free port")]
fn closed_port(w: &mut EngineWorld) {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    tcp(w, "127.0.0.1", port);
}

#[given("a unix socket server")]
fn unix_socket_server(w: &mut EngineWorld) {
    let path = socket_server(w, None);
    w.add(format!("socket:{path}"), Kind::Socket(path));
}

#[given("nothing listening on a unix socket")]
fn missing_socket(w: &mut EngineWorld) {
    let path = socket_path("missing-sock");
    w.add(format!("socket:{path}"), Kind::Socket(path));
}

#[given(regex = r"^an HTTP server answering (\d+)( to GET)?$")]
fn http_server_answering(w: &mut EngineWorld, status: u16, get: String) {
    let port = http_server(status, None);
    let url = format!("http://127.0.0.1:{port}/");
    let (name, method) = if get.is_empty() {
        (url.clone(), "HEAD")
    } else {
        (format!("http-get://127.0.0.1:{port}/"), "GET")
    };
    w.add(name, http(&url, method, |_| {}));
}

/// Trust is always explicit roots: `roots: None` means "accept any certificate" in Rust.
#[given(regex = r"^an HTTPS server answering (\d+), (trusted through its CA|not trusted)$")]
fn https_server(w: &mut EngineWorld, status: u16, trust: String) {
    let port = http_server(status, Some(tls()));
    let url = format!("https://127.0.0.1:{port}/");
    let ca = if trust == "not trusted" {
        "other-ca.pem"
    } else {
        "ca.pem"
    };
    w.add(
        url.clone(),
        http(&url, "HEAD", |o| o.roots = Some(vec![pem(ca)])),
    );
}

#[given(expr = "an HTTP server on a unix socket answering {int}")]
fn http_unix_server(w: &mut EngineWorld, status: u16) {
    let path = socket_server(w, Some(reply(status)));
    let kind = http("http://localhost/", "HEAD", |o| {
        o.socket_path = Some(path.clone())
    });
    w.add(format!("http://unix:{path}:/"), kind);
}

#[given(expr = "a command that exits {int}")]
fn command(w: &mut EngineWorld, code: i32) {
    let c = format!("node -e \"process.exit({code})\"");
    w.add(format!("command:{c}"), Kind::Command(c));
}

#[when(
    regex = r"^I wait( in reverse)? with timeout (\d+)ms, interval (\d+)ms, window (\d+)ms, delay (\d+)ms, tcp timeout (\d+)ms and command timeout (\d+)ms$"
)]
#[allow(clippy::too_many_arguments)]
async fn wait_for(
    w: &mut EngineWorld,
    reverse: String,
    timeout: u64,
    interval: u64,
    window: u64,
    delay: u64,
    tcp_timeout: u64,
    command_timeout: u64,
) {
    let ms = Duration::from_millis;
    let spec = WaitSpec {
        delay: ms(delay),
        interval: ms(interval),
        window: ms(window),
        tcp_timeout: ms(tcp_timeout),
        command_timeout: ms(command_timeout),
        simultaneous: None,
        timeout: Some(ms(timeout)),
        reverse: !reverse.is_empty(),
        resources: w.resources.clone(),
    };
    let sink = Sink {
        log: None,
        verbose: false,
    };
    w.outcome = Some(wait(spec, sink, None::<Arc<NoValidate>>).await);
}

#[then("the wait succeeds")]
fn succeeds(w: &mut EngineWorld) {
    assert_eq!(w.outcome, Some(Ok(())));
}

#[then("the wait fails with:")]
fn fails_with(w: &mut EngineWorld, step: &Step) {
    w.fails_with(step.docstring.as_deref().unwrap_or_default().trim());
}

#[then(expr = "the wait times out naming resource {int}")]
fn times_out_naming(w: &mut EngineWorld, n: usize) {
    w.fails_with(&format!("Timed out waiting for: <resource {n}>"));
}

#[tokio::main]
async fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/../../features");
    let summary = EngineWorld::cucumber()
        .fail_on_skipped()
        .filter_run(features, |feature, rule, scenario| {
            let rule = rule.map(|r| r.tags.as_slice()).unwrap_or_default();
            [feature.tags.as_slice(), rule, scenario.tags.as_slice()]
                .iter()
                .any(|tags| tags.iter().any(|t| t == "engine"))
        })
        .await;
    // Parsing and hook errors count as failures too, so an unparsable feature cannot drop out.
    if summary.scenarios_stats().passed == 0 || summary.execution_has_failed() {
        std::process::exit(1);
    }
}
