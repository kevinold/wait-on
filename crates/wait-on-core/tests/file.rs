//! `file:` through `waiter::wait`, forward and reverse, on a paused clock.

mod common;

use common::*;
use tokio::time::{Instant, advance};
use wait_on_core::waiter::{Kind, Resource, wait};

/// A temp file holding `size` bytes, removed on drop.
struct TempFile(String);

impl TempFile {
    fn new(name: &str, size: usize) -> Self {
        let f = TempFile(temp(name));
        f.write(size);
        f
    }
    fn write(&self, size: usize) {
        std::fs::write(&self.0, "x".repeat(size)).unwrap();
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn file(path: &str) -> Resource {
    Resource {
        name: path.to_string(),
        kind: Kind::File(path.to_string()),
    }
}

fn stats(lines: &Lines) -> usize {
    text(lines)
        .iter()
        .filter(|l| l.starts_with("checking file stat"))
        .count()
}

#[tokio::test(start_paused = true)]
async fn file_forward_ready_when_present() {
    // The first read only seeds the size, so even with window 0 the second tick is ready.
    let f = TempFile::new("present", 1);
    let (sink, lines) = recorder(false);
    let t0 = Instant::now();
    assert_eq!(wait(spec(vec![file(&f.0)]), sink, NONE).await, Ok(()));
    let e = t0.elapsed();
    assert_eq!(e, 251 * MS);
    assert_eq!(text(&lines), [format!("waiting for 1 resources: {}", f.0)]);
}

#[tokio::test(start_paused = true)]
async fn file_forward_window_waits_for_stable_size() {
    let f = TempFile::new("growing", 1);
    let p = f.0.clone();
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&p)]);
    s.interval = 100 * MS;
    s.window = 300 * MS;
    let t0 = Instant::now();
    let run = tokio::spawn(wait(s, sink, NONE));
    let changed = |n: usize| format!("  file exists, checking for size changes, size:{n} file:{p}");
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| has(&lines, &changed(1))).await;
    f.write(2);
    advance(100 * MS).await;
    settle(|| has(&lines, &changed(2))).await;
    f.write(3);
    advance(100 * MS).await;
    settle(|| has(&lines, &changed(3))).await;
    assert_eq!(run.await.unwrap(), Ok(()));
    let e = t0.elapsed();
    // last change seen at 201 ms, stable one 300 ms window later
    assert_eq!(e, 501 * MS);
    assert!(has(
        &lines,
        &format!("  file stabilized at size:3 file:{p}")
    ));
}

#[tokio::test(start_paused = true)]
async fn file_forward_times_out_when_missing() {
    let p = temp("missing");
    let (sink, _) = recorder(false);
    let mut s = spec(vec![file(&p)]);
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: {p}")));
    assert_eq!(e, 500 * MS);
}

#[tokio::test(start_paused = true)]
async fn file_reverse_ready_once_removed() {
    let f = TempFile::new("reverse", 1);
    let (sink, lines) = recorder(true);
    let mut s = spec(vec![file(&f.0)]);
    s.reverse = true;
    s.interval = 100 * MS;
    let t0 = Instant::now();
    let run = tokio::spawn(wait(s, sink, NONE));
    settle(|| text(&lines).len() == 1).await;
    advance(MS).await;
    settle(|| stats(&lines) == 1).await;
    advance(100 * MS).await;
    settle(|| stats(&lines) == 2).await;
    breathe(20).await; // let the 101 ms stat land before the removal
    advance(49 * MS).await;
    std::fs::remove_file(&f.0).unwrap();
    assert_eq!(run.await.unwrap(), Ok(()));
    let e = t0.elapsed();
    // removed at 150 ms; the next tick (201 ms) reads it gone
    assert_eq!(e, 201 * MS);
    assert_eq!(stats(&lines), 3);
}

#[tokio::test(start_paused = true)]
async fn file_reverse_times_out_while_present() {
    let f = TempFile::new("stays", 1);
    let (sink, _) = recorder(false);
    let mut s = spec(vec![file(&f.0)]);
    s.reverse = true;
    s.timeout = Some(500 * MS);
    let t0 = Instant::now();
    let out = wait(s, sink, NONE).await;
    let e = t0.elapsed();
    assert_eq!(out, Err(format!("Timed out waiting for: {}", f.0)));
    assert_eq!(e, 500 * MS);
}
