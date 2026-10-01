use crate::NotReady;
use std::io;
use std::time::Duration;
use tokio::net::{TcpStream, lookup_host};
use tokio::task::JoinSet;

/// Connects to `host:port`, then closes. `timeout_ms` bounds the whole attempt; 0 means none.
pub async fn ready(host: &str, port: u16, timeout_ms: u32) -> Result<(), NotReady> {
    let attempt = connect(host, port);
    if timeout_ms == 0 {
        return attempt.await;
    }
    tokio::time::timeout(Duration::from_millis(timeout_ms.into()), attempt)
        .await
        .unwrap_or(Err(NotReady::TimedOut))
}

/// Races one connect per resolved address; first success wins (dropping the stream closes it).
/// Sequential tries lose on Windows, where a refused loopback connect takes ~2 s.
async fn connect(host: &str, port: u16) -> Result<(), NotReady> {
    let mut attempts = JoinSet::new();
    for addr in lookup_host((host, port)).await.map_err(NotReady::Io)? {
        attempts.spawn(TcpStream::connect(addr));
    }
    let mut last_err = io::Error::new(io::ErrorKind::NotFound, "no addresses resolved");
    while let Some(joined) = attempts.join_next().await {
        match joined.map_err(io::Error::other).and_then(|r| r) {
            Ok(_stream) => return Ok(()), // JoinSet drop aborts the losers
            Err(e) => last_err = e,
        }
    }
    Err(NotReady::Io(last_err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn ready_when_listening() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(ready("127.0.0.1", port, 300).await.is_ok());
    }

    #[tokio::test]
    async fn times_out_within_bound() {
        // A black-holed address: TimedOut, or an immediate network error on hosts without a route.
        let start = std::time::Instant::now();
        let result = ready("10.255.255.1", 9, 200).await;
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
        let reason = result.unwrap_err().to_string();
        // TimedOut only after the bound; an Io error only before it.
        assert_eq!(
            reason == "timed out",
            elapsed >= Duration::from_millis(200),
            "{reason}"
        );
    }

    #[tokio::test]
    async fn timeout_zero_means_no_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // "localhost", not an IP literal: resolving it cannot finish on the first poll, so a
        // zero-length timeout would fire (an IP-literal loopback connect can finish instantly).
        assert!(ready("localhost", port, 0).await.is_ok());
    }

    #[tokio::test]
    async fn refused_when_nothing_listens() {
        let port = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // 5000: Windows reports a loopback refusal only after ~2 s.
        let reason = ready("127.0.0.1", port, 5000)
            .await
            .unwrap_err()
            .to_string();
        assert_ne!(reason, "timed out"); // an Io error, not the bound
        assert!(!reason.is_empty());
    }

    #[tokio::test]
    async fn localhost_finds_ipv4_only_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(ready("localhost", port, 300).await.is_ok());
    }

    #[tokio::test]
    async fn ipv6_literal() {
        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(ready("::1", port, 300).await.is_ok());
    }

    #[tokio::test]
    async fn resolve_error_is_not_ready() {
        // Large bound so a slow CI resolver cannot turn this into TimedOut.
        let reason = ready("no-such-host.invalid", 1, 5000)
            .await
            .unwrap_err()
            .to_string();
        assert_ne!(reason, "timed out"); // an Io error, not the bound
    }

    #[tokio::test]
    async fn closes_after_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (result, accepted) = tokio::join!(ready("127.0.0.1", port, 300), listener.accept());
        assert!(result.is_ok());
        let (mut server_side, _) = accepted.unwrap();
        let mut buf = [0u8; 1];
        let n = tokio::time::timeout(Duration::from_millis(300), server_side.read(&mut buf))
            .await
            .expect("no EOF within 300 ms")
            .unwrap();
        assert_eq!(n, 0);
    }
}
