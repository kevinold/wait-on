use napi::{
    Env, Task,
    bindgen_prelude::{AsyncTask, Object},
};
use napi_derive::napi;

#[napi]
pub fn version() -> String {
    wait_on_core::version().to_string()
}

#[napi]
pub fn noop() {}

pub struct FileSize(String);

#[napi]
impl Task for FileSize {
    type Output = i64;
    type JsValue = i64;

    fn compute(&mut self) -> napi::Result<i64> {
        Ok(wait_on_core::file_size(&self.0))
    }

    fn resolve(&mut self, _env: Env, output: i64) -> napi::Result<i64> {
        Ok(output)
    }
}

/// `fileSize(path): Promise<number>`, off the event loop on the libuv threadpool.
#[napi(ts_return_type = "Promise<number>")]
pub fn file_size(path: String) -> AsyncTask<FileSize> {
    AsyncTask::new(FileSize(path))
}

#[napi(object)]
pub struct CommandResult {
    pub ok: bool,
    pub stdout: String,
    pub error: String,
}

/// `runCommand(command, timeoutMs): Promise<CommandResult>`, never rejecting. Each attempt
/// gets its own thread, not the libuv threadpool, so a slow command cannot starve `fileSize`.
#[napi(ts_return_type = "Promise<CommandResult>")]
pub fn run_command(env: &Env, command: String, timeout_ms: u32) -> napi::Result<Object<'_>> {
    let (deferred, promise) = env.create_deferred()?;
    std::thread::spawn(move || {
        let r = wait_on_core::run_command(&command, timeout_ms);
        deferred.resolve(move |_| {
            Ok(CommandResult {
                ok: r.ok,
                stdout: r.stdout,
                error: r.error,
            })
        });
    });
    Ok(promise)
}

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
