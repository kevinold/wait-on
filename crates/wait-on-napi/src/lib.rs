use napi_derive::napi;

mod http;

#[napi]
pub fn version() -> String {
    wait_on_core::version().to_string()
}

#[napi]
pub fn noop() {}
