//! The polling loop: one wait over every resource with the JS engine's schedule,
//! stabilization, concurrency, timeout, reverse and log text (`lib/engine-js.js`).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::{JoinSet, spawn_blocking};
use tokio::time::{Instant, sleep_until, timeout_at};

use crate::NotReady;
use crate::http::{HttpChecker, HttpOptions};

/// One wait, validated and parsed JS-side. Timer fields are already floored/clamped there.
#[derive(Debug, Clone)]
pub struct WaitSpec {
    pub delay: Duration,
    pub interval: Duration,
    /// File size stabilization window (JS already raised it to `interval`).
    pub window: Duration,
    pub tcp_timeout: Duration,
    /// Per-attempt kill bound for `command:`; zero means none.
    pub command_timeout: Duration,
    /// Checks in flight per resource; `None` is unlimited. Ignored by `command:`.
    pub simultaneous: Option<usize>,
    /// `None` never times out.
    pub timeout: Option<Duration>,
    pub reverse: bool,
    pub resources: Vec<Resource>,
}

/// `name` is the resource string as given, used in the waiting and timeout text.
#[derive(Debug, Clone)]
pub struct Resource {
    pub name: String,
    pub kind: Kind,
}

impl Resource {
    /// The JS spec's `kind` string picks which fields are read: file/socket `path`, tcp
    /// `path`+`host`+`port`, command `command`, http `http`. A missing field is its default.
    pub fn from_parts(
        name: String,
        kind: &str,
        path: Option<String>,
        host: Option<String>,
        port: Option<u16>,
        command: Option<String>,
        http: Option<HttpOptions>,
    ) -> Result<Self, String> {
        let path = path.unwrap_or_default();
        let kind = match kind {
            "file" => Kind::File(path),
            "http" => Kind::Http(http.unwrap_or_default()),
            "tcp" => Kind::Tcp {
                path,
                host: host.unwrap_or_default(),
                port: port.unwrap_or_default(),
            },
            "socket" => Kind::Socket(path),
            "command" => Kind::Command(command.unwrap_or_default()),
            other => return Err(format!("unknown resource kind: {other}")),
        };
        Ok(Self { name, kind })
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    File(String),
    Http(HttpOptions),
    /// `path` is the text after `tcp:`, shown in the verbose line.
    Tcp {
        path: String,
        host: String,
        port: u16,
    },
    Socket(String),
    Command(String),
}

/// Forwards one line; the returned future settles once the line has been written.
pub type LogFn = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Where lines go: `log` absent writes nothing; `verbose` adds the per-resource lines.
pub struct Sink {
    pub log: Option<LogFn>,
    pub verbose: bool,
}

impl Sink {
    async fn log(&self, line: impl FnOnce() -> String) {
        if let Some(log) = &self.log {
            log(line()).await;
        }
    }

    async fn output(&self, line: impl FnOnce() -> String) {
        if self.verbose {
            self.log(line).await;
        }
    }
}

enum Probe {
    File(String),
    Http {
        checker: HttpChecker,
        method: String,
        url: String,
        /// `socketPath:<path>` or empty, as in the JS line.
        desc: String,
    },
    Tcp {
        path: String,
        host: String,
        port: u16,
    },
    Socket(String),
    Command(String),
}

/// A check's raw answer: a file size for the stabilization scan, else the verdict.
enum Out {
    Size(i64),
    Ready(bool),
}

/// Runs the whole wait. `Err` is the exact timeout or checker construction message.
/// `validate` decides http readiness from the status (an `Err` means not ready).
pub async fn wait<V, Fut, E>(
    spec: WaitSpec,
    sink: Sink,
    validate: Option<Arc<V>>,
) -> Result<(), String>
where
    V: Fn(u16) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<bool, E>> + Send + 'static,
    E: 'static,
{
    let mut names = Vec::new();
    let mut probes = Vec::new();
    for r in spec.resources {
        names.push(r.name);
        probes.push(Arc::new(match r.kind {
            Kind::File(p) => Probe::File(p),
            Kind::Http(opts) => Probe::Http {
                method: opts.method.clone(),
                url: opts.url.clone(),
                desc: (opts.socket_path.as_ref())
                    .map_or(String::new(), |p| format!("socketPath:{p}")),
                checker: HttpChecker::new(opts)?,
            },
            Kind::Tcp { path, host, port } => Probe::Tcp { path, host, port },
            Kind::Socket(p) => Probe::Socket(p),
            Kind::Command(c) => Probe::Command(c),
        }));
    }
    let sink = Arc::new(sink);
    let mut ready = vec![false; names.len()];
    sink.log(|| waiting(&names, &ready)).await;

    let start = Instant::now();
    // Node floors timers at 1 ms, and the JS timeout timer is subscribed first: it wins ties.
    let first = start + spec.delay.max(MIN);
    let timed_out = |ready: &[bool]| format!("Timed out waiting for: {}", remaining(&names, ready));
    if let Some(deadline) = spec
        .timeout
        .map(|t| start + t.max(MIN))
        .filter(|d| *d <= first)
    {
        sleep_until(deadline).await;
        return Err(timed_out(&ready));
    }
    let schedule = Schedule {
        first,
        period: spec.interval.max(MIN),
        simultaneous: spec.simultaneous.unwrap_or(usize::MAX),
        window: spec.window,
        tcp_timeout: spec.tcp_timeout,
        command_timeout: spec.command_timeout,
        reverse: spec.reverse,
    };
    let mut tasks = JoinSet::new();
    for (i, probe) in probes.iter().enumerate() {
        let (probe, sink, validate) = (Arc::clone(probe), Arc::clone(&sink), validate.clone());
        tasks.spawn(async move {
            poll(probe, schedule, sink, validate).await;
            i
        });
    }
    let drain = async {
        while let Some(done) = tasks.join_next().await {
            ready[done.expect("resource tasks never panic")] = true;
            if ready.contains(&false) {
                sink.log(|| waiting(&names, &ready)).await;
            }
        }
    };
    let finished = match spec.timeout {
        Some(t) => timeout_at(start + t, drain).await.is_ok(),
        None => {
            drain.await;
            true
        }
    };
    // Settle: abort every resource task and in-flight check, close every http pool.
    drop(tasks);
    for probe in &probes {
        if let Probe::Http { checker, .. } = &**probe {
            checker.cancel();
        }
    }
    if finished {
        Ok(())
    } else {
        Err(timed_out(&ready))
    }
}

const MIN: Duration = Duration::from_millis(1);

#[derive(Clone, Copy)]
struct Schedule {
    first: Instant,
    period: Duration,
    simultaneous: usize,
    window: Duration,
    tcp_timeout: Duration,
    command_timeout: Duration,
    reverse: bool,
}

fn remaining(names: &[String], ready: &[bool]) -> String {
    let left: Vec<&str> = names
        .iter()
        .zip(ready)
        .filter(|(_, r)| !**r)
        .map(|(n, _)| n.as_str())
        .collect();
    left.join(", ")
}

fn waiting(names: &[String], ready: &[bool]) -> String {
    let left = ready.iter().filter(|r| !**r).count();
    format!("waiting for {left} resources: {}", remaining(names, ready))
}

/// Ticks one resource until a check reads ready. Ticks past `simultaneous` queue and start
/// one per completion (rxjs mergeMap); `command:` drops them while an attempt runs (exhaustMap).
async fn poll<V, Fut, E>(probe: Arc<Probe>, s: Schedule, sink: Arc<Sink>, validate: Option<Arc<V>>)
where
    V: Fn(u16) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<bool, E>> + Send + 'static,
    E: 'static,
{
    let (limit, queue) = match *probe {
        Probe::Command(_) => (1, false),
        _ => (s.simultaneous, true),
    };
    let mut inflight = JoinSet::new();
    let mut queued = 0usize;
    let mut next = s.first;
    let start = |set: &mut JoinSet<_>| {
        set.spawn(check(
            Arc::clone(&probe),
            s,
            Arc::clone(&sink),
            validate.clone(),
        ));
    };
    let path = if let Probe::File(p) = &*probe {
        p.as_str()
    } else {
        ""
    };
    // The JS `scan` seed: no size yet.
    let (mut size, mut since) = (-1, s.first);
    loop {
        if !inflight.is_empty() {
            if let Ok(done) = timeout_at(next, inflight.join_next()).await {
                let out = done
                    .expect("in-flight set is not empty")
                    .expect("checks never panic");
                let is_ready = match out {
                    Out::Ready(ok) => ok,
                    Out::Size(x) if s.reverse => x == -1,
                    Out::Size(-1) => false,
                    Out::Size(x) => {
                        let now = Instant::now();
                        if x != size {
                            sink.output(|| {
                                format!(
                                    "  file exists, checking for size changes, size:{x} file:{path}"
                                )
                            })
                            .await;
                            (size, since) = (x, now);
                            false
                        } else if now >= since + s.window {
                            sink.output(|| format!("  file stabilized at size:{x} file:{path}"))
                                .await;
                            true
                        } else {
                            sink.output(|| {
                                format!("  file exists, checking for size change during stability window, size:{x} file:{path}")
                            })
                            .await;
                            false
                        }
                    }
                };
                if is_ready {
                    if let Probe::Http { checker, .. } = &*probe {
                        checker.cancel();
                    }
                    return;
                }
                if queued > 0 {
                    queued -= 1;
                    start(&mut inflight);
                }
                continue;
            }
        } else {
            sleep_until(next).await;
        }
        next = Instant::now() + s.period;
        if inflight.len() < limit {
            start(&mut inflight);
        } else if queue {
            queued += 1;
        }
    }
}

/// One check with its verbose lines (`lib/engine-js.js` text); verdicts are reverse-negated.
async fn check<V, Fut, E>(
    probe: Arc<Probe>,
    s: Schedule,
    sink: Arc<Sink>,
    validate: Option<Arc<V>>,
) -> Out
where
    V: Fn(u16) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<bool, E>> + Send + 'static,
    E: 'static,
{
    let ok = match &*probe {
        Probe::File(path) => {
            sink.output(|| format!("checking file stat for file:{path} ..."))
                .await;
            let p = path.clone();
            let size = spawn_blocking(move || crate::file_size(&p))
                .await
                .expect("file_size never panics");
            return Out::Size(size);
        }
        Probe::Http {
            checker,
            method,
            url,
            desc,
        } => {
            sink.output(|| format!("making HTTP(S) {method} request to {desc} url:{url} ..."))
                .await;
            let out = checker
                .check(validate.map(|v| move |status| v(status)))
                .await;
            match &out.error {
                Some(e) => sink.output(|| format!("  HTTP(S) error for {url} {e}")).await,
                None => {
                    sink.output(|| {
                        let text = out.status_text.as_ref().map_or("undefined".into(), |t| format!("'{t}'"));
                        let status = out.status.expect("a response has a status");
                        format!("  HTTP(S) result for {url}: {{ status: {status}, statusText: {text}, ok: {} }}", out.ok)
                    })
                    .await
                }
            }
            out.ok
        }
        Probe::Tcp { path, host, port } => {
            sink.output(|| format!("making TCP connection to {path} ..."))
                .await;
            let ms = s.tcp_timeout.as_millis();
            let result = crate::tcp::ready(host, *port, ms as u32).await;
            sink.output(|| match &result {
                Ok(()) => format!("  TCP connection successful to host:{host} port:{port}"),
                Err(NotReady::TimedOut) => {
                    format!(
                        "  timed out connecting to TCP host:{host} port:{port} tcpTimeout:{ms}ms"
                    )
                }
                Err(e) => format!("  error connecting to TCP host:{host} port:{port} {e}"),
            })
            .await;
            result.is_ok()
        }
        Probe::Socket(path) => {
            sink.output(|| format!("making socket connection to {path} ..."))
                .await;
            let result = crate::socket::ready(path).await;
            sink.output(|| match &result {
                Ok(()) => format!("  connected to socket:{path}"),
                Err(e) => format!("  error connecting to socket socket:{path} {e}"),
            })
            .await;
            result.is_ok()
        }
        Probe::Command(command) => {
            sink.output(|| format!("executing command \"{command}\" ..."))
                .await;
            let (c, ms) = (command.clone(), s.command_timeout.as_millis() as u32);
            let r = spawn_blocking(move || crate::run_command(&c, ms))
                .await
                .expect("run_command never panics");
            sink.output(|| {
                if r.ok {
                    format!("  Command \"{command}\" success. stdout: \"{}\"", r.stdout)
                } else {
                    format!("  Command error: \"{}\"", r.error)
                }
            })
            .await;
            r.ok
        }
    };
    Out::Ready(ok != s.reverse)
}

#[cfg(test)]
mod tests;
