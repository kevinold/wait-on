---
title: Porting a JS timer loop to tokio means matching Node's per-API overflow and the deadline, not only the outcome
date: 2026-10-01
category: best-practices
module: crates/wait-on-core
problem_type: best_practice
component: rust-engine
severity: medium
applies_when:
  - Moving rxjs or setTimeout-driven scheduling into a Rust/tokio loop that must stay observably identical to the JS engine
  - Mapping user duration options from JS into a Rust spec (napi u32 fields)
  - Writing tokio paused-clock tests for a loop that also does real localhost I/O
related_components: [crates/wait-on-core, lib/engine-rust.js, lib/engine-js.js]
tags: [tokio, paused-clock, timers, node-timers, overflow, parity, napi, spike-next-rs]
---

# Porting a JS timer loop to tokio means matching Node's per-API overflow and the deadline, not only the outcome

## Context

Lane L7 (#59) moved wait-on's polling loop from the rxjs pipeline (`lib/engine-js.js`) into `wait_on_core::waiter` (`crates/wait-on-core/src/waiter.rs`), reached through one napi `wait()` call. The JS engine is the oracle: callback, CLI output and timing must match it. Two parity bugs survived the plan, the cargo suite at 100% line and region coverage, and the first parity tests. Code review found both. Each came from treating a JS timer rule as uniform or as outcome-only.

## Guidance

1. **Node's overflow rule differs per API. Check each option against the Node call that backs it on the JS engine.** Joi admits integers up to `Number.MAX_SAFE_INTEGER`, so values above 2^31-1 ms reach the engine. Observed on Node 26:
   - `setTimeout`/`setInterval` (rxjs `timer`, the `delay`, `interval`, `timeout` options) reset the delay to **1 ms** with a `TimeoutOverflowWarning`.
   - `child_process.exec` `timeout` (`commandTimeout`, `lib/engine-js.js:345`) also becomes **1 ms**: the child is killed at once.
   - `net.Socket#setTimeout` (`tcpTimeout`, `lib/engine-js.js:288`) **truncates to 2^31-1** ("Timer duration was truncated to 2147483647").

   The shim first sent every field through one `timerMs` helper, so `tcpTimeout: 3e9` became a 1 ms connect bound and every Rust tcp attempt timed out. The fix keeps `timerMs` for the setTimeout-backed fields and caps `tcpTimeout` with `Math.min(tcpTimeout, 2 ** 31 - 1)` (`lib/engine-rust.js:29-39`). Probe the real API before choosing a mapping: `node -e "require('child_process').exec('sleep 1',{timeout:3e9},e=>console.log(e&&e.killed))"`.

2. **An early decision still fires at the deadline.** When `timeout <= max(delay, 1 ms)`, the outcome is already known. The JS timeout timer is subscribed before the resource timers, Node floors both to 1 ms, and a check result always lands after its tick, so the timeout wins. But JS rejects only when its timer fires. The first Rust version returned `Err` immediately: with `delay: 3000, timeout: 600` it rejected after 3 ms where JS took 600 ms. The fix waits for the deadline before returning (`crates/wait-on-core/src/waiter.rs:139-146`, `sleep_until(start + timeout.max(1 ms))`).

3. **Paused-clock tests must assert when, not only what.** Under `#[tokio::test(start_paused = true)]` a wrong-timing return still produces the right `Err`. Measure virtual time: `let started = Instant::now(); ... assert_eq!(started.elapsed(), (timeout * MS).max(MS));` (`crates/wait-on-core/src/waiter/tests.rs:202`). Back it with one real-clock front-door test that runs on both engines (`test/api.mocha.js:1243`, "should time out at the deadline, not sooner, ...": elapsed within 550..2500 ms for `timeout: 600`).

4. **Settle real I/O before advancing a paused clock.** With real localhost sockets, `spawn_blocking` stats or shell commands in flight, tokio's auto-advance can jump virtual time while the I/O is still pending, so extra ticks start. The suites use a bounded real-time yield loop (`settle`/`breathe` in `crates/wait-on-core/src/waiter/tests.rs:131-150` and `crates/wait-on-core/tests/loop.rs:64-80`) to let I/O finish before an explicit `advance`. Lines that auto-advance can repeat are asserted with "contains", not an exact vector.

5. **Keep 100% region coverage measurable.** Put unit tests in a sibling file (`waiter/tests.rs` via `#[cfg(test)] mod tests;`) and exclude test files from the report (`cargo llvm-cov -p wait-on-core --summary-only --ignore-filename-regex '(waiter/tests\.rs|/tests/)'`), because every `assert!` adds an untaken panic region. Avoid `tokio::select!` without an `else` arm, which expands to an unreachable branch. Use `timeout_at`/`sleep_until` instead, and `expect` for invariants rather than defensive arms.

## Why This Matters

Both bugs passed every check that only compared outcomes. The tcpTimeout one breaks a whole resource type for users who pass a huge value meaning "no limit". The early-timeout one changes CLI timing in scripts that rely on `-d`/`-t` together. Neither shows up as a failing test unless the test measures time or the option is above 2^31-1.

## When to Apply

Any lane or port that replaces a Node timer, socket timeout or child-process timeout with a Rust equivalent, and any paused-clock test of such a loop. Re-check when a new duration option crosses napi.

## Examples

Wrong (one rule for every field, outcome-only short-circuit):

```js
tcpTimeoutMs: timerMs(tcpTimeout), // socket timeouts truncate; this sends 1 ms
```

```rust
if spec.timeout.is_some_and(|t| start + t <= first) {
    return Err(timed_out(&ready)); // decided early, but JS rejects at the deadline
}
```

Right:

```js
tcpTimeoutMs: Math.min(tcpTimeout, 2 ** 31 - 1), // Node truncates a socket timeout instead
```

```rust
if let Some(deadline) = spec.timeout.map(|t| start + t.max(MIN)).filter(|d| *d <= first) {
    sleep_until(deadline).await;
    return Err(timed_out(&ready));
}
```

Related: [napi readiness checks must not block the libuv threadpool](napi-readiness-checks-must-not-block-the-libuv-threadpool.md) (where blocking checks run), [porting JS regex parsers with a differential oracle](porting-js-regex-parsers-to-rust-with-a-differential-oracle.md) (the same JS-as-oracle discipline for parsers).
