use crate::NotReady;

/// Connects to a unix socket (a named pipe on Windows), then closes. No timeout, like JS.
pub async fn ready(path: &str) -> Result<(), NotReady> {
    #[cfg(unix)]
    let opened = tokio::net::UnixStream::connect(path).await;
    #[cfg(windows)]
    let opened = tokio::net::windows::named_pipe::ClientOptions::new().open(path);
    opened.map(drop).map_err(NotReady::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wait-on-core-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ready_when_listening() {
        let path = temp_path("ready.sock");
        let _ = std::fs::remove_file(&path);
        let _listener = tokio::net::UnixListener::bind(&path).unwrap();
        assert!(ready(path.to_str().unwrap()).await.is_ok());
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn not_ready_when_missing() {
        let path = temp_path("no-such-sock");
        // `ready` has no timeout, so every error is `NotReady::Io`.
        assert!(ready(path.to_str().unwrap()).await.is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn ready_when_pipe_exists() {
        let path = format!(r"\\.\pipe\wait-on-core-{}", std::process::id());
        let _server = tokio::net::windows::named_pipe::ServerOptions::new()
            .create(&path)
            .unwrap();
        assert!(ready(&path).await.is_ok());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn not_ready_when_pipe_missing() {
        let path = format!(r"\\.\pipe\wait-on-core-missing-{}", std::process::id());
        assert!(ready(&path).await.is_err());
    }
}
