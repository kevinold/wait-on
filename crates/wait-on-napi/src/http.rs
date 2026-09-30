//! JS binding for `wait_on_core::http`: `new HttpChecker(opts)`, `check(validateStatus?)`, `cancel()`.

use std::collections::HashMap;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use wait_on_core::http;

/// `CalleeHandled = false`: called as `fn(status)`. Weak: the pending Promise already holds the loop.
type ValidateStatus = ThreadsafeFunction<u16, bool, u16, Status, false, true>;

#[napi(object)]
pub struct HttpCheckerOptions {
    pub url: String,
    /// `"HEAD"` or `"GET"`.
    pub method: String,
    pub headers: HashMap<String, String>,
    pub follow_redirect: bool,
    pub timeout_ms: Option<u32>,
}

#[napi(object)]
pub struct HttpCheckResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub error: Option<String>,
}

#[napi]
pub struct HttpChecker {
    // Arc: each check's future owns a handle, so it never borrows the JS object.
    inner: Arc<http::HttpChecker>,
}

#[napi]
impl HttpChecker {
    #[napi(constructor)]
    pub fn new(opts: HttpCheckerOptions) -> Result<Self> {
        let inner = http::HttpChecker::new(http::HttpOptions {
            url: opts.url,
            method: opts.method,
            headers: opts.headers.into_iter().collect(),
            follow_redirect: opts.follow_redirect,
            timeout_ms: opts.timeout_ms.map(u64::from),
        })
        .map_err(Error::from_reason)?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Resolves `{ ok, status?, statusText?, error? }`; never rejects for not-ready.
    #[napi(ts_return_type = "Promise<HttpCheckResult>")]
    pub fn check(
        &self,
        env: Env,
        validate_status: Option<ValidateStatus>,
    ) -> Result<AsyncBlock<HttpCheckResult>> {
        let inner = Arc::clone(&self.inner);
        AsyncBlockBuilder::new(async move {
            // call_async_catch: a JS throw or non-bool is Err (=> not ready), never a fatal exception.
            let validate =
                validate_status.map(|f| move |s: u16| async move { f.call_async_catch(s).await });
            let out = inner.check(validate).await;
            Ok(HttpCheckResult {
                ok: out.ok,
                status: out.status,
                status_text: out.status_text,
                error: out.error,
            })
        })
        .build(&env)
    }

    /// Settles every in-flight and later `check` as `{ ok: false, error: 'cancelled' }`.
    #[napi]
    pub fn cancel(&self) {
        self.inner.cancel();
    }
}
