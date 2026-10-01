//! `command:` through `waiter::wait`, forward and reverse, on a paused clock. The command
//! itself runs in real time on the blocking pool (tokio does not auto-advance while it runs);
//! these commands exit at once, and `settle`'s 5 s real deadline is the only headroom needed.

mod common;

use common::*;
use tokio::time::Instant;
use wait_on_core::waiter::{Kind, Resource, wait};

const OK0: &str = "exit 0";
const FAIL3: &str = if cfg!(windows) { "exit /b 3" } else { "exit 3" };

fn command(c: &str) -> Resource {
    Resource {
        name: format!("command:{c}"),
        kind: Kind::Command(c.to_string()),
    }
}

#[tokio::test(start_paused = true)]
async fn command_forward_ready_on_exit_zero() {
    let (sink, lines) = recorder(true);
    assert_eq!(wait(spec(vec![command(OK0)]), sink, NONE).await, Ok(()));
    assert!(has(
        &lines,
        &format!("  Command \"{OK0}\" success. stdout: \"\"")
    ));
}

#[tokio::test(start_paused = true)]
async fn command_forward_times_out_on_nonzero_exit() {
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![command(FAIL3)]);
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: command:{FAIL3}")));
    assert_eq!(e, 500 * MS);
    let lines = text(&lines);
    assert!(
        lines.iter().any(|l| l.contains("Command error:")),
        "{lines:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn command_reverse_ready_when_command_fails() {
    // OQ2: reverse settles under the paused clock with the attempt on `spawn_blocking`.
    let (sink, _) = recorder(false);
    let mut s = spec(vec![command(FAIL3)]);
    s.reverse = true;
    let t0 = Instant::now();
    assert_eq!(wait(s, sink, NONE).await, Ok(()));
    let e = t0.elapsed();
    assert_eq!(e, MS); // the first attempt
}

#[tokio::test(start_paused = true)]
async fn command_reverse_times_out_while_command_succeeds() {
    let (sink, _) = recorder(false);
    let mut s = spec(vec![command(OK0)]);
    s.reverse = true;
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: command:{OK0}")));
    assert_eq!(e, 500 * MS);
}
