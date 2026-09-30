use napi_derive::napi;

#[napi]
pub fn version() -> String {
    wait_on_core::version().to_string()
}

#[napi]
pub fn noop() {}

/// A check's answer; `reason` crosses as `null` (not an absent key) when ready.
#[napi(object, use_nullable = true)]
pub struct CheckResult {
    pub ready: bool,
    pub timed_out: bool,
    pub reason: Option<String>,
}

impl From<Result<(), wait_on_core::NotReady>> for CheckResult {
    fn from(r: Result<(), wait_on_core::NotReady>) -> Self {
        match r {
            Ok(()) => CheckResult {
                ready: true,
                timed_out: false,
                reason: None,
            },
            Err(e) => CheckResult {
                ready: false,
                timed_out: matches!(e, wait_on_core::NotReady::TimedOut),
                reason: Some(e.to_string()),
            },
        }
    }
}

#[napi]
pub async fn tcp_check(host: String, port: u16, timeout_ms: u32) -> CheckResult {
    wait_on_core::tcp::ready(&host, port, timeout_ms)
        .await
        .into()
}

#[napi]
pub async fn socket_check(path: String) -> CheckResult {
    wait_on_core::socket::ready(&path).await.into()
}
