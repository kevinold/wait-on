//! `waiter::wait` end to end through the public API, against real resources under a
//! paused clock. Scenarios already pinned by `src/waiter/tests.rs` are not repeated.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use tokio::time::advance;
use wait_on_core::waiter::{Kind, Resource, wait};

#[tokio::test(start_paused = true)]
async fn http_head_and_get_print_result_lines_and_a_refused_url_times_out() {
    let (base, _seen) = server(Some(OK_CLOSE));
    let (head, get) = (format!("{base}/head"), format!("{base}/get"));
    let refused = format!("http://127.0.0.1:{}/", closed_port());
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![
        http_with(&head, |o| o.method = "HEAD".into()),
        http_with(&get, |_| {}),
        http_with(&refused, |_| {}),
    ]);
    s.timeout = Some(300 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    let error = format!("  HTTP(S) error for {refused} ");
    settle(|| {
        let t = text(&lines);
        t.iter()
            .filter(|l| l.starts_with("  HTTP(S) result"))
            .count()
            == 2
            && t.iter().any(|l| l.starts_with(&error))
    })
    .await;
    advance(300 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {refused}"))
    );
    let for_url = |u: &str| -> Vec<String> {
        text(&lines)
            .into_iter()
            .filter(|l| l.contains(&format!("url:{u} ")) || l.contains(&format!("for {u}:")))
            .collect()
    };
    assert_eq!(
        for_url(&head),
        [
            format!("making HTTP(S) HEAD request to  url:{head} ..."),
            result_line(&head, 200, "OK", true),
        ]
    );
    assert_eq!(
        for_url(&get),
        [
            format!("making HTTP(S) GET request to  url:{get} ..."),
            result_line(&get, 200, "OK", true),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn validate_false_keeps_waiting_until_it_answers_true() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let validate = Arc::new(move |status: u16| {
        let n = counted.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok::<_, ()>(status == 200 && n > 0))
    });
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http_with(&url, |o| o.method = "HEAD".into())]);
    s.interval = 100 * MS;
    let run = tokio::spawn(wait(s, sink, Some(validate)));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| has(&lines, &result_line(&url, 200, "OK", false))).await;
    assert!(!run.is_finished(), "latched on a false verdict");
    advance(100 * MS).await;
    // settle the second check before awaiting, or auto-advance can start a third
    settle(|| has(&lines, &result_line(&url, 200, "OK", true))).await;
    assert_eq!(run.await.unwrap(), Ok(()));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let results: Vec<String> = text(&lines)
        .into_iter()
        .filter(|l| l.starts_with("  HTTP(S) result"))
        .collect();
    assert_eq!(
        results,
        [
            result_line(&url, 200, "OK", false),
            result_line(&url, 200, "OK", true)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn validate_error_is_not_ready() {
    let (url, _seen) = server(Some(OK_CLOSE));
    let validate = Arc::new(|_: u16| std::future::ready(Err::<bool, _>("boom")));
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![http_with(&url, |_| {})]);
    s.timeout = Some(300 * MS);
    let run = tokio::spawn(wait(s, sink, Some(validate)));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| has(&lines, &result_line(&url, 200, "OK", false))).await;
    advance(300 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {url}"))
    );
}

#[tokio::test(start_paused = true)]
async fn settle_aborts_in_flight_checks_and_stops_ticking() {
    let (url, seen) = server(None);
    let (sink, lines) = recorder(false);
    let mut s = spec(vec![http_with(&url, |o| o.method = "HEAD".into())]);
    s.interval = 100 * MS;
    s.timeout = Some(150 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    assert_eq!(next(&seen).await, Seen::Head(0));
    advance(100 * MS).await;
    assert_eq!(next(&seen).await, Seen::Head(1));
    advance(49 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: {url}"))
    );
    // both hung requests are dropped client-side at the settle
    let mut closed = vec![next(&seen).await, next(&seen).await];
    closed.sort_by_key(|s| format!("{s:?}"));
    assert_eq!(closed, [Seen::Closed(0), Seen::Closed(1)]);
    // and no tick runs after it
    advance(300 * MS).await;
    breathe(100).await;
    assert!(seen.try_recv().is_err(), "a check ran after the settle");
}

const WIN: bool = cfg!(windows);
const SLEEP5: &str = if WIN {
    "ping -n 6 127.0.0.1 >nul"
} else {
    "sleep 5"
};

#[tokio::test(start_paused = true)]
async fn command_runs_once_across_five_ticks_and_is_killed_at_command_timeout() {
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![Resource {
        name: format!("command:{SLEEP5}"),
        kind: Kind::Command(SLEEP5.into()),
    }]);
    s.interval = 20 * MS;
    s.command_timeout = 200 * MS;
    s.timeout = Some(500 * MS);
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| text(&lines).len() == 2).await;
    for _ in 0..5 {
        advance(20 * MS).await;
    }
    let killed = format!("  Command error: \"Command failed: {SLEEP5}\nkilled after 200ms\"");
    settle(|| has(&lines, &killed)).await;
    assert_eq!(
        text(&lines)[1..],
        [format!("executing command \"{SLEEP5}\" ..."), killed]
    );
    advance(500 * MS).await;
    assert_eq!(
        run.await.unwrap(),
        Err(format!("Timed out waiting for: command:{SLEEP5}"))
    );
}

#[tokio::test(start_paused = true)]
async fn mixed_wait_logs_one_waiting_line_per_flip_except_the_last() {
    let file = temp("mixed");
    std::fs::write(&file, "x").unwrap();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let (url, _seen) = server(Some(OK_CLOSE));
    let (sock, _guard) = listening_socket("mixed.sock");
    let resources = vec![
        Resource {
            name: file.clone(),
            kind: Kind::File(file.clone()),
        },
        Resource {
            name: format!("tcp:127.0.0.1:{port}"),
            kind: Kind::Tcp {
                path: format!("127.0.0.1:{port}"),
                host: "127.0.0.1".into(),
                port,
            },
        },
        http_with(&url, |o| o.method = "HEAD".into()),
        Resource {
            name: format!("socket:{sock}"),
            kind: Kind::Socket(sock.clone()),
        },
        Resource {
            name: "command:echo hi".into(),
            kind: Kind::Command("echo hi".into()),
        },
    ];
    let names: Vec<String> = resources.iter().map(|r| r.name.clone()).collect();
    let (sink, lines) = recorder(false);
    let out = wait(spec(resources), sink, NONE).await;
    let _ = std::fs::remove_file(&file);
    assert_eq!(out, Ok(()));
    let lines = text(&lines);
    assert_eq!(lines.len(), 5, "{lines:?}");
    // each line names the still-unready resources in order, one fewer each flip
    let mut left = names;
    for (k, line) in lines.iter().enumerate() {
        assert_eq!(
            line,
            &format!("waiting for {} resources: {}", 5 - k, left.join(", "))
        );
        if k < 4 {
            let after: Vec<&str> = lines[k + 1]
                .split_once(": ")
                .unwrap()
                .1
                .split(", ")
                .collect();
            left.retain(|n| after.contains(&n.as_str()));
            assert_eq!(left.len(), 4 - k, "{lines:?}");
        }
    }
}
