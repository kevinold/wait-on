---
title: A napi readiness check must run on napi's tokio runtime, not the libuv threadpool
date: 2026-09-30
category: best-practices
module: crates/wait-on-napi
problem_type: best_practice
component: rust-engine
severity: high
applies_when:
  - Porting a wait-on resource check (tcp, socket, file, http, command) to Rust behind the napi addon
  - A Rust check can wait on the network or the OS for longer than one poll interval
  - Writing a timeout test or a cross-platform connect for a Rust check
related_components: [crates/wait-on-core, lib/wait-on.js]
tags: [napi-rs, tokio, asynctask, libuv-threadpool, windows, ipv6, timeout, spike-next-rs]
---

# A napi readiness check must run on napi's tokio runtime, not the libuv threadpool

## Context

Lane L2 (#54) moved the `tcp:` and `socket:` checks to Rust (`crates/wait-on-core`, bound in `crates/wait-on-napi`). JS still polls through rxjs: every `interval` tick calls the check through `mergeMap` with concurrency `simultaneous`, which defaults to Infinity. So several checks for the same resource can be in flight at once. The obvious napi shape, `AsyncTask` with a blocking `std::net` connect, fails under that load, and a few other traps showed up only on review or on Windows.

## Guidance

1. **Use `#[napi] async fn` (napi feature `async`), not `AsyncTask` + blocking I/O.** `AsyncTask::compute` runs on the libuv threadpool, which has 4 threads by default. With `tcpTimeout: 0` against a black-holed host, each tick parks one more thread until the OS connect timeout, which is tens of seconds or more. Node's `fs` and DNS work then stalls, and sibling `file:` and `http:` resources stop progressing. JS `net.connect` never touches that pool. `std::net::TcpStream::connect_timeout` also rejects a zero `Duration`. See `crates/wait-on-napi/Cargo.toml:17` (`features = ["napi4", "async"]`) and `crates/wait-on-napi/src/lib.rs:37`.
2. **Declare result objects `#[napi(object, use_nullable = true)]`** when JS expects `null`. Without it, a `None` field is left out of the object instead of being set to `null` (`crates/wait-on-napi/src/lib.rs:12`).
3. **Race resolved addresses; don't try them in order.** `tokio::net::TcpStream::connect((host, port))` tries each address sequentially. On Windows, a refused loopback connect is reported only after about 2 s (the SYN is retried after the RST). So `localhost` resolving to `::1` first uses up the default 300 ms `tcpTimeout` before `127.0.0.1` is tried. Resolve with `lookup_host`, spawn one connect per address in a `JoinSet`, and take the first `Ok`. Dropping the set aborts the losers (`crates/wait-on-core/src/tcp.rs:21-28`). Node gets the same result from `autoSelectFamily`.
4. **Declare tokio `rt` in the core crate** (`crates/wait-on-core/Cargo.toml:10`). `lookup_host` goes through `spawn_blocking`, which panics without `rt`. The core crate works today only because napi turns that feature on for the whole build.
5. **Test a zero timeout against `localhost`, not an IP literal.** `tokio::time::timeout` polls the inner future once before it checks the deadline, and a loopback connect to `127.0.0.1` can finish on that first poll. A test that uses the IP literal passes even when `0` is wrongly treated as "expire immediately" (`crates/wait-on-core/src/tcp.rs:68`).
6. **Allow for Windows' slow loopback refusal in refusal tests.** Give any test that expects an `io::Error` for a closed port a bound of seconds (the lane used 5000 ms). With a 300 ms bound, it gets `TimedOut` on Windows instead.

## Why This Matters

Each trap fails without an error at the point of the change. Threadpool starvation looks like unrelated resources being slow. The sequential address walk passes on macOS and Linux, where refusal is instant, and fails only on the windows `ci:rs` row. The zero-timeout test stays green while the behavior is broken.

## When to Apply

Every lane that adds a Rust check behind the addon (L3 `file:`, L4/L5 http, L6 `command:`). Put blocking filesystem or process work behind `tokio::task::spawn_blocking` or tokio's async APIs, never directly in the napi call path or an `AsyncTask`.

## Examples

Path proof that the Rust engine actually answered, not the JS fallback. (Historical: the original `test/engine-checks.mocha.js` wrapped the per-check `addon.tcpCheck` export; lane L13 (#76) removed both the file and that export.)

- **Rust:** the readiness behavior is specified in `crates/wait-on-core/tests/tcp.rs` (`tcp_forward_times_out_while_connect_pending` and the refused/reverse cells).
- **API:** `test/fixtures/counting-addon.js` records every `wait` call that reaches the real prebuild, so a test asserts the call count to prove the wait ran in Rust.
- **CLI:** run with `--verbose` and assert stdout contains `(os error`. Only Rust's `io::Error` text includes it; Node prints `ECONNREFUSED`.
