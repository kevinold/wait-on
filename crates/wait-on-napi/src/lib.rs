use napi::{Env, Task, bindgen_prelude::AsyncTask};
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
