//! JS shape of `wait_on_core::http::HttpOptions` (one `wait` resource's `http`) and the
//! `validateStatus` callback type.

use std::collections::HashMap;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use wait_on_core::http;

/// `CalleeHandled = false`: called as `fn(status)`. Weak: the pending Promise already holds the loop.
pub(crate) type ValidateStatus = ThreadsafeFunction<u16, bool, u16, Status, false, true>;

#[napi(object)]
pub struct HttpCheckerOptions {
    pub url: String,
    /// `"HEAD"` or `"GET"`.
    pub method: String,
    pub headers: HashMap<String, String>,
    pub follow_redirect: bool,
    pub timeout_ms: Option<u32>,
    /// PEM bundles to trust exclusively; absent accepts any cert.
    pub roots: Option<Vec<String>>,
    pub cert: Option<String>,
    pub key: Option<String>,
    /// Proxy URI; absent connects directly.
    pub proxy: Option<String>,
    /// Unix socket or Windows named pipe replacing TCP.
    pub socket_path: Option<String>,
}

impl From<HttpCheckerOptions> for http::HttpOptions {
    fn from(opts: HttpCheckerOptions) -> Self {
        http::HttpOptions {
            url: opts.url,
            method: opts.method,
            headers: opts.headers.into_iter().collect(),
            follow_redirect: opts.follow_redirect,
            timeout_ms: opts.timeout_ms.map(u64::from),
            roots: opts.roots,
            cert: opts.cert,
            key: opts.key,
            proxy: opts.proxy,
            socket_path: opts.socket_path,
        }
    }
}
