//! JS binding for `wait_on_core::waiter::wait`: types, a field-by-field mapping and the
//! `AsyncBlock` wrapper only. Every loop decision lives in `wait-on-core` (tested there).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use wait_on_core::waiter;

use crate::http::{HttpCheckerOptions, ValidateStatus};

/// `log(line)`. Weak: the pending Promise already holds the loop.
type LogLine = ThreadsafeFunction<String, (), String, Status, false, true>;

/// Validated and clamped JS-side; `simultaneous`/`timeoutMs` absent mean unlimited/never.
#[napi(object)]
pub struct WaitSpec {
    pub delay_ms: u32,
    pub interval_ms: u32,
    pub window_ms: u32,
    pub tcp_timeout_ms: u32,
    pub command_timeout_ms: u32,
    pub simultaneous: Option<u32>,
    pub timeout_ms: Option<u32>,
    pub reverse: bool,
    pub verbose: bool,
    pub resources: Vec<ResourceSpec>,
}

/// `kind` picks which optional fields are read: file/socket `path`, tcp `path`+`host`+`port`,
/// command `command`, http `http`. A missing field crosses as its default.
#[napi(object)]
pub struct ResourceSpec {
    pub name: String,
    /// `"file" | "http" | "tcp" | "socket" | "command"`.
    pub kind: String,
    pub path: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub command: Option<String>,
    pub http: Option<HttpCheckerOptions>,
}

/// `error` crosses as `null` (not an absent key) on success.
#[napi(object, use_nullable = true)]
pub struct WaitResult {
    pub ok: bool,
    pub error: Option<String>,
}

fn ms(n: u32) -> Duration {
    Duration::from_millis(u64::from(n))
}

impl TryFrom<ResourceSpec> for waiter::Resource {
    type Error = Error;

    fn try_from(r: ResourceSpec) -> Result<Self> {
        Self::from_parts(
            r.name,
            &r.kind,
            r.path,
            r.host,
            r.port,
            r.command,
            r.http.map(Into::into),
        )
        .map_err(Error::from_reason)
    }
}

impl TryFrom<WaitSpec> for waiter::WaitSpec {
    type Error = Error;

    fn try_from(s: WaitSpec) -> Result<Self> {
        Ok(Self {
            delay: ms(s.delay_ms),
            interval: ms(s.interval_ms),
            window: ms(s.window_ms),
            tcp_timeout: ms(s.tcp_timeout_ms),
            command_timeout: ms(s.command_timeout_ms),
            simultaneous: s.simultaneous.map(|n| n as usize),
            timeout: s.timeout_ms.map(ms),
            reverse: s.reverse,
            resources: (s.resources.into_iter().map(TryInto::try_into)).collect::<Result<_>>()?,
        })
    }
}

/// `wait(spec, log?, validateStatus?)`: resolves `{ ok, error }`, never rejecting for a
/// timeout or a checker construction error. Every `log` line has run before it settles.
#[napi(ts_return_type = "Promise<WaitResult>")]
pub fn wait(
    env: Env,
    spec: WaitSpec,
    log: Option<LogLine>,
    validate_status: Option<ValidateStatus>,
) -> Result<AsyncBlock<WaitResult>> {
    let sink = waiter::Sink {
        verbose: spec.verbose,
        log: log.map(|f| {
            let f = Arc::new(f);
            Box::new(move |line: String| {
                let f = Arc::clone(&f);
                Box::pin(async move {
                    let _ = f.call_async(line).await;
                }) as Pin<Box<dyn Future<Output = ()> + Send>>
            }) as waiter::LogFn
        }),
    };
    let spec = waiter::WaitSpec::try_from(spec)?;
    // call_async_catch: a JS throw or non-bool is Err (=> not ready), never a fatal exception.
    let validate = validate_status.map(|f| {
        let f = Arc::new(f);
        Arc::new(move |s: u16| {
            let f = Arc::clone(&f);
            async move { f.call_async_catch(s).await }
        })
    });
    AsyncBlockBuilder::new(async move {
        let r = waiter::wait(spec, sink, validate).await;
        Ok(WaitResult {
            ok: r.is_ok(),
            error: r.err(),
        })
    })
    .build(&env)
}
