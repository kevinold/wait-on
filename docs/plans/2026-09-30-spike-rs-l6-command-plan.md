---
title: "[L6] command: resource in Rust - Plan"
type: feat
date: 2026-09-30
topic: rust-port
lane: L6
kind: preview
branch: rs-58-command
closes: kevinold/wait-on#58
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L6] command: resource in Rust - Plan

Implementation-ready lane plan for sub-issue #58 (lane L6 of spine #35). Product Contract preservation: requirements R-L6-1..R-L6-10 keep the IDs and meaning of the requirements-only plan; the named-risks table moved to the Planning Contract with test IDs, the open areas (napi shape, timeout mechanism, Windows shell, stdin, maxBuffer, error text, stale prebuild) are decided in KTD1..KTD4, and the engine x reverse x commandTimeout x OS matrix lives under the Verification Contract. The spine plan is controlling and is never edited by this lane.

---

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`: under `WAIT_ON_ENGINE=rust` with a loaded addon, every `command:` attempt is run by Rust with the shell, exit-code, `commandTimeout` and reverse behavior a user already sees under JS on Linux, macOS and Windows; the JS engine, `npm test`, coverage thresholds, public API, CLI, `WAIT_ON_SCHEMA` and `index.d.ts` are unchanged; `npm run ci:rs` runs every command test under `rust-strict` with `test/rust-pending.js` still empty. Proves spine lane row L6 (R1).

**Means.** One pure runner in `crates/wait-on-core` with Node's `exec` shell semantics and a std-only per-attempt kill (KTD1, KTD2), one napi export `runCommand(command, timeoutMs)` resolving on a dedicated thread to `{ ok, stdout, error }` and never rejecting (KTD3), and a one-line check selection in `createCommand$` keyed on the addon `waitOnImpl` already resolves (KTD4). The rxjs pipeline and `negateAsync` are untouched. A recording JS stub addon proves the Rust route ran (KTD5).

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions.** A `.github/workflows/` change is needed -> stop and report to the PM. A public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change would be needed -> stop. A new runtime npm dependency would be needed -> stop. A command test must be added to `test/rust-pending.js` -> stop and report (R-L6-8). A new crate is needed because the std-only timeout in KTD2 cannot be made to work -> stop and record the reason before adding one.

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`; command tests stay on the real clock; one PR, base `spike-next-rs` on `kevinold/wait-on`, body `Closes #58`; never merged by the lane; merge `origin/spike-next-rs` (no rebase, no force-push) before marking ready.

---

## Product Contract

### Summary

Move the per-attempt `command:` check (`commandPasses` in `lib/wait-on.js`) into Rust and route the Rust engine through it while the JS rxjs pipeline keeps orchestrating (KD-S3). The command runs with the same shell semantics as Node's `child_process.exec` on each OS, exit 0 means ready, `commandTimeout` kills an attempt at its per-attempt bound, and reverse mode stays in JS through `negateAsync`.

### Problem Frame

Today the addon answers only the `file:` probe (L3). `createCommand$` always calls `commandPasses`, which uses `util.promisify(child_process.exec)`: `/bin/sh -c <command>` on POSIX, `ComSpec` (default `cmd.exe`) with `/d /s /c "<command>"` and verbatim arguments on Windows; the child inherits `process.env` and `process.cwd()`; any non-zero exit, signal death, spawn error or `timeout` kill (`killSignal: 'SIGKILL'`) resolves `false`, exit 0 resolves `true`; verbose output logs `executing command "<cmd>" ...`, then `  Command "<cmd>" success. stdout: "<stdout>"` or `  Command error: "<message>"`. The rxjs shape (`exhaustMap`, `startWith(false)`, `distinctUntilChanged()`, `take(2)`) does not change.

### Requirements

**Rust runner**

- R-L6-1 Core runner. `wait-on-core` exposes a pure function taking a command string and an optional per-attempt timeout in ms (0 means no limit). It runs the command through the same shell Node's `exec` uses on each OS and returns whether the command exited 0, the captured stdout (for the success log line) and an error description (for the failure log line). It never panics. A spawn failure, a non-zero exit, a signal death or a timeout all report not-ready.
- R-L6-2 Timeout. When `commandTimeout > 0` and the attempt is still running at that bound, the child shell is killed (SIGKILL-equivalent; `TerminateProcess` on Windows) and the attempt reports not-ready. The same known ceiling as JS applies: only the shell is killed, and a detached grandchild can outlive it.
- R-L6-3 Addon surface. `wait-on-napi` exports the runner to JS as an async function that returns a Promise and does not block the event loop. A command can run for seconds, so a sync export is out. KTD3 fixes the name, the result shape and the thread it runs on.

**Routing and pipeline**

- R-L6-4 Routing. `createCommand$` uses the addon's runner when `addon` is non-null. Otherwise it calls `commandPasses` exactly as today. It uses the `addon` already threaded through deps by L3 and adds no second `resolveEngine`.
- R-L6-5 Reverse and pipeline unchanged. `negateAsync`, `exhaustMap`, `startWith`, `distinctUntilChanged`, `take(2)` and the `executing command` line stay unchanged. Only the check source differs by engine.
- R-L6-6 Log shape. Under the Rust engine the verbose success and error lines keep their JS shape: the success line carries stdout and the error line carries a description. Byte-identical error text is not required, because full log parity belongs to L7.

**Proof and parity**

- R-L6-7 Prove the path ran. A test shows that under `rust` plus the fixture addon, the command check was answered by the addon and not by a silent JS fallback. Per L3's technique, the fixture records calls and returns an answer JS cannot produce for the given command.
- R-L6-8 Real-addon parity. Every existing command test passes under `npm run ci:rs` (`rust-strict`, real addon) with `test/rust-pending.js` still empty: the six in `test/api.mocha.js` `describe('command (#87, #15, #71)')`, the five in `test/cli.mocha.js`, and the command cases in `test/cli-conformance-helper.mocha.js` and `test/parser-properties.mocha.js`. The pending list is already empty on the base, so "shrink the pending list" means keeping it empty. Needing to list a command test is a stop-and-report, not a quiet add.
- R-L6-9 JS engine unchanged. The `npm test` outcome, the `.nycrc.json` coverage thresholds, the public API, the CLI, `WAIT_ON_SCHEMA` and `index.d.ts` do not change.
- R-L6-10 Guides. The `docs/guides/` pages that list ported checks (`architecture.md`, `testing.md`, `development.md` and `contributing-dual-engine.md` as applicable) say that under the Rust engine the Rust addon answers `command:`, where in the code that happens, and every deliberate JS-vs-Rust difference KTD1 and KTD2 record.

### Scope Boundaries

- Allowed paths per lane contract (#58): `crates/`, `Cargo.toml`, `Cargo.lock`, `lib/`, `bin/`, `test/`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `package.json`, `scripts/`, `AGENTS.md`, `README.md`. No `.github/workflows/` edits (stop condition). No new npm runtime dependency. No new crate: `std::process`, `std::thread` and napi's own promise API suffice (KTD2, KTD3); if the std-only timeout proves impractical, stop and record the reason before adding one.
- Not in scope: moving the polling loop to Rust (L7), tcp/socket (L2), http (L4), byte-exact verbose error text (L7).
- Edits to shared files stay small and additive so the L2 and L4 merges are mechanical: `crates/*/src/lib.rs` (new functions and structs appended), `lib/wait-on.js` (`createCommand$` only), `test/fixtures/fake-addon.js` (one new method), `test/engine.mocha.js` (one new `describe`, one `ENGINE_VARS` entry), `docs/guides/` (new bullets and rows).
- Non-goals (considered, not built, one line each):
  - Node's `maxBuffer` kill: JS fails an attempt whose stdout or stderr exceeds 1 MiB; Rust keeps the first 1 MiB and drains the rest (KTD1), because a readiness check that fails on output volume is a JS quirk nobody asked to keep, while unbounded memory would be a real hazard.
  - The non-`cmd.exe` `ComSpec` branch (Node passes `-c <cmd>` when `ComSpec` is not `cmd.exe`): no CI row can exercise it and no user has asked; add when reported.
  - An open stdin pipe as Node gives the child: Rust uses a null stdin (KTD1), so a command that reads stdin gets EOF instead of blocking until a timeout; recorded in the guide as a deliberate difference.
  - A feature-detect guard for a stale prebuild or fixture lacking `runCommand` with fallback to JS (KTD4): the `TypeError` already arrives through the callback on the first poll and names the missing function; a silent fallback would hide a stale build under `rust`.
  - Process-group or job-object kill of grandchildren: same ceiling as JS (`ponytail:` comment in `commandPasses`), recorded in R-L6-2.
  - `windowsHide`, a `--verbose` line naming the engine, and passing the engine name into deps: nothing in this lane needs them.
  - A `wait-timeout`, `subprocess` or `tokio::process` dependency: KTD2 does it in std.

### Sources

- Issue kevinold/wait-on#58 (no comments at authoring time).
- Spine plan KD-S1, KD-S2, KD-S3, KD-S7, KD-S9; lane row L6. Source plan R1. Sibling lane plan `docs/plans/2026-09-30-spike-rs-l3-file-plan.md` (`AsyncTask` precedent, stub-addon technique, real-addon skip pattern, guide edits).
- `lib/wait-on.js` (`waitOnImpl` deps line, `createCommand$`, `commandPasses`, `negateAsync`, `WAIT_ON_SCHEMA` `commandTimeout`), `lib/engine.js`, `crates/wait-on-core/src/lib.rs`, `crates/wait-on-napi/src/lib.rs`, `crates/wait-on-napi/Cargo.toml` (`napi = "3"`, `default-features = false`, `features = ["napi4"]`, no tokio, `test = false`), `Cargo.toml` (edition 2024), `test/engine.mocha.js` (`withEnv`, `runCLI`, `ENGINE_VARS`, `FIXTURE_ADDON`, `addonPath({})` skip pattern, `file: probe routing` block), `test/fixtures/fake-addon.js`, `test/rust-pending.js`, `test/api.mocha.js` and `test/cli.mocha.js` command blocks, `scripts/ci-rs.js`, `deny.toml`, `docs/guides/architecture.md`, `docs/guides/testing.md`, `docs/guides/development.md`, `docs/guides/contributing-dual-engine.md`.
- Node `lib/child_process.js` `normalizeSpawnArguments` (single-pass trace): `shell: true` selects `/bin/sh -c` on POSIX; on Windows `process.env.comspec || 'cmd.exe'`, and when that matches `cmd(.exe)` the args are `/d /s /c "<cmd>"` with `windowsVerbatimArguments`; stdin is a pipe; `maxBuffer` 1 MiB per stream; the callback fires on `close`; `error.message` is `Command failed: <cmd>\n<stderr>`.

---

## Planning Contract

### Key Technical Decisions

- KTD1. Runner in core: `wait_on_core::run_command(command, timeout_ms) -> CommandResult { ok, stdout, error }`, never panicking or returning an error type. Shell: unix `/bin/sh -c <command>`; Windows the program named by `ComSpec` (default `cmd.exe`), args `/d`, `/s`, `/c` then the command wrapped in double quotes via `std::os::windows::process::CommandExt::raw_arg` so the command line is byte-identical to Node's verbatim form. stdin `Stdio::null()`, stdout and stderr piped and decoded lossily as UTF-8. `ok` is `status.success()`, so signal death (`code()` is `None`) and non-zero exit are not-ready. Captured stdout and stderr keep at most 1 MiB each and drain the rest to a sink, bounding memory without emulating Node's kill. `error` has one shape everywhere: `Command failed: <cmd>\n<detail>`, where detail is the captured stderr on exit or spawn-error text on spawn failure, and `killed after <timeout_ms>ms` on timeout (stderr is unavailable on that path by KTD2). Chosen over returning stdout/stderr separately and formatting in JS: the message is a log-only string, one format in one place. Chosen over mirroring Node's `-c` branch for a non-cmd `ComSpec`: non-goal above.
- KTD2. Std-only timeout: spawn, start one reader thread per pipe, then loop on `child.try_wait()` with a short sleep (10 ms; `ponytail:` comment naming the granularity) until the deadline; on expiry `child.kill()` then `wait()`, and do not join the readers (a grandchild may hold the pipe open; the threads end when the pipe closes). When `timeout_ms > 0` the same deadline also bounds the reader joins: if the shell exits first but a backgrounded grandchild still holds stdout, keep polling the reader handles (`is_finished`) in the same loop and, at the deadline, return `ok = status.success()` with whatever was captured, detaching the readers. This matches Node, whose `exec` kill destroys the stdout/stderr streams at the bound so `close` fires there (`exec('sleep 3 & echo hi', { timeout: 300 })` reports success with stdout `hi` at ~300 ms). With `timeout_ms == 0`, `wait()` then join both readers unbounded, which matches Node's `close` semantics: a grandchild holding stdout delays the result on both engines. Chosen over the `wait-timeout` crate (a dependency for fifteen lines), `tokio::process` (needs a runtime and the napi `tokio_rt` feature) and a waiter thread with a channel (the kill needs `&mut Child`, so it would need a mutex around the child).
- KTD3. Async napi export on a dedicated thread: a `#[napi]` export `run_command(env, command: String, timeout_ms: u32)` creates a deferred promise with `Env::create_deferred`, spawns a `std::thread` that calls `wait_on_core::run_command` and resolves the deferred with a `#[napi(object)] CommandResult { ok: bool, stdout: String, error: String }`; JS sees `runCommand(command, timeoutMs): Promise<{ ok, stdout, error }>`, and the promise never rejects because core never errors and compute has no fallible step. Chosen over L3's `AsyncTask` on the libuv threadpool: a command blocks its thread for its whole run, the pool has four threads shared with `fileSize` (L3) and Node's own `fs`/`dns` work, and JS `exec` never held a pool thread, so four slow commands would silently delay `file:` probes and DNS lookups under Rust only; the deferred form is the same size and T8 pins it. Chosen over `#[napi] async fn` (needs tokio) and over a sync export (blocks the event loop and `timer(timeout)`). Chosen over a `Promise<boolean>`: the log lines need stdout and the error text (R-L6-6). A rejection would kill the rxjs stream where JS returns `false`, so "never rejects" is the shape's invariant, pinned by T3.
- KTD4. Routing keyed on addon presence, `commandPasses` untouched. `createCommand$` destructures `addon` from deps (already resolved once in `waitOnImpl`, L3) and picks the base check: with an addon, a small sibling function that awaits `addon.runCommand(command, commandTimeout)`, prints the same success or error line from `{ ok, stdout, error }`, and returns `ok`; without one, `commandPasses` as today. `checkFn = reverse ? negateAsync(base) : base`, so reverse, `exhaustMap`, `startWith`, `distinctUntilChanged`, `take(2)` and the `executing command` line are unchanged. No feature-detect guard: a stale prebuild or fixture without `runCommand` throws inside the async wrapper, rxjs turns it into an error notification, and `cleanup` delivers it to the callback naming the missing function; the fix is `npm run build:napi`, which `docs/guides/development.md` already states. (session-settled: user-directed — only the per-attempt check moves to Rust; the rxjs pipeline and `negateAsync` stay JS, chosen over moving the loop now: KD-S3 assigns the loop to L7.) (session-settled: user-directed — JS stays default and behavior-unchanged, chosen over changing the default or JS behavior: side-by-side spike, KD-S1.)
- KTD5. Stub-addon technique. `test/fixtures/fake-addon.js` gains `async runCommand(command, timeoutMs)` that pushes `{ command, timeoutMs }` onto the shared `calls` array and resolves `{ ok: process.env.WAIT_ON_FAKE_COMMAND_OK !== '0', stdout: 'fake', error: 'fake' }` without spawning anything. Default `ok: true` means a command that always fails under JS (`node -e "process.exit(1)"`) succeeds under `rust` + fixture, an outcome JS cannot produce (T4); `WAIT_ON_FAKE_COMMAND_OK=0` makes a passing command report not-ready, so reverse resolves where JS would time out (T6). The recorded `timeoutMs` proves `commandTimeout` reaches the addon. In-process tests share the fixture module instance `resolveEngine` required, so `calls` is the path proof; CLI subprocesses cannot share it, so the CLI cell proves the route by exit-code contrast (0 under `rust` + fixture, 1 under `js`). L1's fixture tests wait on `__filename` (a file), so routing and the fixture method can land in the same commit without a transient red. Chosen over a spawning fake (proves nothing JS could not) and over path-keyed magic (an env knob is explicit and `withEnv` restores it).
- KTD6. Test placement. Engine routing tests live in `test/engine.mocha.js` under a new `describe('command: check routing')`, reusing `withEnv`, `runCLI` and the `calls` reset in `beforeEach`; `ENGINE_VARS` gains `WAIT_ON_FAKE_COMMAND_OK`. The one new engine-agnostic behavior test (T9, reverse plus `commandTimeout`) joins the existing command block in `test/api.mocha.js` so both `npm test` and `ci:rs` run it. Cargo tests use per-OS shell builtins so `cargo test` needs no `node`: `#[cfg(unix)]` cases with `exit 3`, `echo`, `sleep`, `$CARGO_PKG_NAME`; `#[cfg(windows)]` cases with `exit /b 3`, `echo`, `ping -n <k> 127.0.0.1 >nul` as the sleep (the `timeout` builtin refuses a redirected stdin), and `%CARGO_PKG_NAME%`. `CARGO_PKG_NAME` is set by cargo in the test process, so env inheritance is tested without `std::env::set_var`, which is `unsafe` under edition 2024. Chosen over a new spec file (duplicates the env helpers) and over node-based cargo tests (couples `cargo test` to a node install).
- KTD7. Lane contract. (session-settled: user-directed — no `.github/workflows/` edits, no new runtime dependency, stay inside allowed paths; chosen over workflow edits in the lane: KD-S7, operator request on #35.) (session-settled: user-directed — PR base `spike-next-rs` on kevinold/wait-on, `Closes #58`, never merged by the lane; chosen over merging or targeting jeffbski/wait-on: PM owns merge, fork-only.)

### Assumptions

- `Env::create_deferred` and a `#[napi(object)]` struct as the resolved value compile under `napi = { version = "3", default-features = false, features = ["napi4"] }` (the deferred resolves through a threadsafe function, an N-API 4 feature). Verified by `cargo clippy` in U1; if a feature flag is missing, enable it on the existing `napi` dependency (still no `tokio`).
- `CommandExt::raw_arg` (stable since Rust 1.62) is available under the pinned toolchain.
- `std::env::var("ComSpec")` resolves case-insensitively on Windows, as Node's `process.env.comspec` does.
- `commandTimeout` crosses the boundary as `u32` ms; the schema already bounds it to a non-negative integer, and values above `u32::MAX` are not a readiness-check concern.
- Command strings cross as napi `String` (UTF-8); stdout is decoded lossily, so invalid UTF-8 in output changes only the verbose log line.
- The killed-by-timeout path cannot be given a grandchild deterministically on every OS in a cargo test; its detach-not-join behavior is by construction (KTD2), and the api kill test under `ci:rs` covers the common no-grandchild case.
- Siblings L2 and L4 add their own fake-addon methods and `describe` blocks; if either changes the shape of `calls` when `origin/spike-next-rs` is merged, adopt their shape (mechanical, assertions adjusted).
- Pipeline run: the scoping synthesis confirmation was skipped (no synchronous user); the decisions above not labeled session-settled are planning inferences the PR reviewer may redirect.

### Risks

| Risk | Answered by |
|---|---|
| Wrong shell or quoting on Windows (`cmd.exe /d /s /c "<cmd>"`), so `node -e "process.exit(0)"` or the `&&`-chained marker command fails | T1 `#[cfg(windows)]` quoted and `&&`-chained case; every api/cli command test under `ci:rs` on the windows row (T10) |
| Exit code misread (signal death or non-zero treated as ready) | T1: exit 0 ready, exit 3 not ready with stderr in `error`; T2: killed attempt not ready |
| Timeout does not kill, so `exhaustMap` wedges polling until the global timeout | T2: a sleeping command with a 200 ms bound returns within one second; api `kills a command that exceeds commandTimeout and keeps polling` under `ci:rs` (T10) |
| A backgrounded grandchild holding stdout stretches an attempt past `commandTimeout` after the shell exits | T2 unix `sleep 3 & echo hi` with a 300 ms bound returns within one second, ready |
| Timeout 0 kills or truncates a long-running attempt | T2: a one-second sleep with timeout 0 is ready with complete stdout |
| A long command starves the libuv threadpool, delaying `file:` probes and DNS under Rust only | KTD3 dedicated thread; T8 (five slow commands never delay a `file:` resource) |
| A sync or blocking export stalls `timer(timeout)` | T3 asserts the export returns a Promise; api `keeps polling to the global timeout...` under `ci:rs` (T10) |
| The addon promise rejects, killing the stream where JS returns `false` | T3: a non-zero command resolves (not rejects) with `ok: false`; T1 spawn-failure case resolves not-ready |
| Silent JS fallback masquerading as Rust success | T4 (failing command succeeds only via the stub, `calls` recorded), T6 (reverse on a passing command), T7 (stub never called under `js`), CLI exit contrast in T4 |
| Env or cwd not inherited by the Rust child | T1 `CARGO_PKG_NAME` case; cli `exits 0 when a command starts passing later` (absolute marker path) under `ci:rs` (T10) |
| `commandTimeout` not forwarded to the addon | T4 asserts the recorded `timeoutMs` |
| Verbose lines lose their shape under Rust | T5 captures `console.log` and asserts the success and error line prefixes |
| Reverse plus `commandTimeout` never exercised on either engine | T9 (hung command, reverse, 200 ms bound resolves) under `npm test` and `ci:rs` |
| `create_deferred` unavailable under `napi4` with default features off | U1 `cargo clippy`; the Assumptions fallback (enable the flag) keeps KTD3; `AsyncTask` is the last resort and would fail T8 |
| Stale local prebuild without `runCommand` | T3 goes red with `addon.runCommand is not a function`; `docs/guides/development.md` names the export (U3) |
| L2/L4 parallel edits to `crates/*/src/lib.rs`, `fake-addon.js`, `engine.mocha.js`, `createCommand$`'s neighbors, guides | All L6 edits additive; merge `origin/spike-next-rs` before ready and rerun the Verification Contract |
| T7, T8, T9 are green on first run | Expected by sequencing. T7 pins the untouched `js` branch: after U2 GREEN, temporarily invert the selector and confirm T7 goes red, then restore. T8 is written after U1's export: temporarily build `runCommand` as an `AsyncTask` and confirm T8's timeout message names `__filename`, then restore the deferred export. T9 passes under `js`: under `npm run ci:rs`, temporarily pass `0` instead of `commandTimeout` from the addon-backed sibling and confirm T9 fails with `Timed out waiting for`, then restore |

---

## Implementation Units

Order: U1 -> U2 -> U3. Test and code land in the same Conventional Commit. U1 first so `ci:rs` is green at every commit (routing before the export would fail `rust-strict` with a missing `runCommand`).

### U1. Rust runner in core and `runCommand` napi export

**Goal.** `wait_on_core::run_command` exists with Node's `exec` shell semantics and the per-attempt kill; the built addon answers `runCommand(command, timeoutMs)` with a never-rejecting Promise of `{ ok, stdout, error }` from a dedicated thread.

**Requirements.** R-L6-1, R-L6-2, R-L6-3.

**Dependencies.** None.

**Files.** `crates/wait-on-core/src/lib.rs`, `crates/wait-on-napi/src/lib.rs`, `test/engine.mocha.js` (T3). `crates/wait-on-napi/Cargo.toml` only if the feature assumption fails.

**Approach.** KTD1, KTD2, KTD3, KTD6. Core: a `CommandResult` struct, `run_command`, a private shell-selection helper with `#[cfg(unix)]`/`#[cfg(windows)]` bodies, and the reader-thread plus `try_wait` loop, appended after `file_size` with T1/T2 in the existing `#[cfg(test)]` module. Napi: the `#[napi(object)]` result struct, the export taking `Env`, `create_deferred`, `std::thread::spawn`, resolve. T3 requires `addonPath({})` directly and skips when the file does not exist, like the `fileSize` case.

**Patterns to follow.** `file_size` and its tests in `crates/wait-on-core/src/lib.rs` (temp names with the pid, `#[cfg(unix)]` cases); "should answer fileSize from the built addon when a host prebuild exists" in `test/engine.mocha.js`.

**Test scenarios.**

- T1 (`cargo test`, `crates/wait-on-core`): `echo hello` is ready with stdout `hello` after trimming (both OSes).
- T1: `exit 3` (unix) / `exit /b 3` (windows) is not ready and `error` starts with `Command failed: ` and contains the command.
- T1: a command that writes to stderr then exits non-zero has that stderr text in `error`.
- T1: a quoted, `&&`-chained command (`echo "quoted" && exit 0` / `echo "quoted" && exit /b 0`) is ready and its stdout contains `quoted`.
- T1: `$CARGO_PKG_NAME` / `%CARGO_PKG_NAME%` compared to `wait-on-core` is ready (env inherited).
- T1: an unrunnable command the shell cannot find is not ready with a `Command failed:` message.
- T1: unix signal death (`kill -9 $$`) is not ready.
- T2 (`cargo test`): `sleep 5` / `ping -n 6 127.0.0.1 >nul` with a 200 ms timeout returns within one second, not ready, `error` mentions `killed after 200ms`.
- T2: `sleep 1` / `ping -n 2 127.0.0.1 >nul` with timeout 0 is ready.
- T2 (`#[cfg(unix)]`): `sleep 3 & echo hi` with a 300 ms timeout returns within one second, ready, with stdout `hi` (the shell exited 0; the backgrounded grandchild holding stdout does not stretch the attempt past the bound).
- T2: a command that prints more than 1 MiB and exits 0 is ready with captured stdout capped at 1 MiB.
- T3 (`test/engine.mocha.js`, "should answer runCommand from the built addon when a host prebuild exists"): skips unless `fs.existsSync(addonPath({}))`; `addon.runCommand('node -e "process.exit(0)"', 0)` is a Promise resolving to `ok: true`; a command that writes `boom` to stderr and exits 3 resolves (never rejects) to `ok: false` with `error` starting `Command failed: ` and containing `boom`.

**Verification.** Cargo fmt, clippy `-D warnings`, test and deny green on the host; after `npm run build:napi`, T3 runs (not skipped) and passes; `npm test` unchanged.

### U2. Route the command check by engine, recording stub addon, matrix cells

**Goal.** Under a loaded addon, `createCommand$` checks through `addon.runCommand` and prints the JS-shaped verbose lines; under `js` and `rust` fallback it uses `commandPasses` unchanged; the fixture proves the route; the reverse plus `commandTimeout` cell has a test on both engines.

**Requirements.** R-L6-4, R-L6-5, R-L6-6, R-L6-7, R-L6-9.

**Dependencies.** U1.

**Files.** `lib/wait-on.js` (`createCommand$` destructures `addon`, selects the base check; the addon-backed sibling function), `test/fixtures/fake-addon.js` (`runCommand`), `test/engine.mocha.js` (`ENGINE_VARS`, new `describe`), `test/api.mocha.js` (T9 in the command block).

**Approach.** KTD4, KTD5, KTD6. The failing command for T4/T5/T7 is `command:node -e "process.exit(1)"`; the passing one for T6 is `command:node -e "process.exit(0)"`. Fast options `timeout: 1000, interval: 100`. T5 replaces `console.log` for the duration of one call and restores it in `finally`. Plain `it` on the real clock throughout (AGENTS.md clock rule).

**Patterns to follow.** The `file: probe routing` block in `test/engine.mocha.js` (`calls` reset, `withEnv`, `runCLI` with a resource argument, exit-code contrast); `commandPasses`'s two output lines.

**Test scenarios** (`test/engine.mocha.js`, describe "command: check routing", unless noted).

- T4 "should succeed on a failing command under rust when the stub addon answers the check": `WAIT_ON_ENGINE=rust`, fixture path, resources `[failing]`, `commandTimeout: 200`; resolves; `calls` deep-includes `{ command: 'node -e "process.exit(1)"', timeoutMs: 200 }`.
- T4 CLI twin "should exit 0 from the CLI on a failing command under rust with the stub addon, and 1 under js": `runCLI(rust, failing)` exits 0; the same spawn under `js` exits 1 with `Timed out` on stderr.
- T5 "should keep the verbose success and error line shapes under rust": with `verbose: true`, the forward run logs a line starting `  Command "node -e "process.exit(1)"" success. stdout: "fake"`; with `WAIT_ON_FAKE_COMMAND_OK=0` and `reverse: true` the run logs a line starting `  Command error: "fake"`.
- T6 "should succeed in reverse mode on a passing command when the stub reports not-ready under rust": `WAIT_ON_FAKE_COMMAND_OK=0`, resources `[passing]`, `reverse: true`; resolves; `calls` deep-includes the passing command with `timeoutMs: 0`.
- T7 "should never call the stub and time out on a failing command under js": `WAIT_ON_ENGINE=js` + fixture path, `timeout: 300`; rejects with `Timed out waiting for`; `calls` stays empty. Green on first run by design (see Risks).
- T9 (`test/api.mocha.js`, command block) "in reverse mode, treats a command killed by commandTimeout as failing": a runner that hangs forever (the `hang.js` shape already used by the kill test, without a marker), `reverse: true`, `commandTimeout: 200`, `timeout: 4000`; resolves. Runs under `js` in `npm test` and under `rust-strict` in `ci:rs`.

**Verification.** `npm test`; `npm run test:coverage` meets `.nycrc.json` thresholds with the addon branch of `createCommand$` and the sibling function covered by T4-T6; `npm run ci:rs` green locally.

### U3. Real-addon gate, threadpool proof and guides

**Goal.** Every existing command test passes under `rust-strict` with the real addon on ubuntu, macos and windows; slow commands never delay other resources; the guides state that `command:` is answered by Rust, where, and the deliberate differences.

**Requirements.** R-L6-3 (thread placement), R-L6-8, R-L6-10.

**Dependencies.** U1, U2.

**Files.** `test/engine.mocha.js` (T8); `docs/guides/architecture.md` (a `command:` (L6) bullet: `wait_on_core::run_command`, the shell per OS, exit 0 is ready, `commandTimeout` kill, export `runCommand(command, timeoutMs): Promise<{ ok, stdout, error }>` on a dedicated thread, `createCommand$` selection, reverse and pipeline in JS; the deliberate differences from KTD1/KTD2: null stdin, 1 MiB keep-and-drain instead of kill, timeout error text, `cmd.exe`-only `ComSpec` form); `docs/guides/testing.md` (engine row adds command routing and the real-addon `runCommand` and threadpool cases; fixture paragraph adds `runCommand` and `WAIT_ON_FAKE_COMMAND_OK`); `docs/guides/development.md` (the stale-prebuild sentence names `runCommand` beside `fileSize`). `test/rust-pending.js` untouched.

**Approach.** T8 first (it is the named test for KTD3), then the docs-only guide edits, then T10 as the gate: run `npm run ci:rs` locally and read the mocha output for the command tests; the PR's `rust` job repeats it on all three OSes.

**Patterns to follow.** The skip-if-no-prebuild pattern; `docs/guides/contributing-dual-engine.md` docs-as-done checklist; the L3 `file:` bullet in `architecture.md`.

**Test scenarios.**

- T8 (`test/engine.mocha.js`, "should keep answering a file: probe while five slow commands run under the built addon"): skips unless a host prebuild exists; `WAIT_ON_ENGINE=rust-strict`; resources are five `command:node -e "setTimeout(function () {}, 3000)"` entries (distinct by a trailing argument) plus `__filename`; `timeout: 1500, interval: 100, window: 100`; rejects with a `Timed out waiting for` message that lists the five commands and not `__filename`. Under a threadpool-bound export the file probe queues behind the commands and the message names `__filename`.
- T10: `npm run ci:rs` green; its mocha pass shows T3 and T8 executed, 0 pending, and every api, cli, `cli-conformance-helper` and `parser-properties` command test (including T9) passing under `rust-strict`. A red command test here is a stop-and-report (R-L6-8), never a pending-list add.
- Docs check: `docs/guides/architecture.md` has the `command:` (L6) bullet; no guide says the command check is JS-only under Rust.

**Verification.** `npm run ci:rs` locally; PR `rust` job green on ubuntu, macos, windows.

---

## Verification Contract

Run from the repo root; all must pass before the PR opens, and again after merging `origin/spike-next-rs`.

- `npm test`: lint, types, mocha under JS; outcome unchanged apart from the new tests (T4-T7, T9; T3 and T8 skip without a host prebuild).
- `npm run test:coverage`: `.nycrc.json` thresholds met with the new `createCommand$` branch covered by the fixture tests.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (T1, T2 run on the host OS), `cargo deny check`.
- `npm run build:napi`, then T3 and T8 run (not skipped).
- `npm run ci:rs`: green; mocha pass under `rust-strict` shows 0 pending and every command test passing (T10).
- CI on the PR: every check green, including the `rust` rows on ubuntu, macos and windows, commitlint and PR title.

Matrix coverage (engine x mode x `commandTimeout`); the OS axis is covered by `ci:rs` running every cell on ubuntu, macos and windows, with the cargo cells split by `#[cfg(unix)]`/`#[cfg(windows)]` (T1, T2):

| Engine | forward, no timeout | forward, timeout | reverse, no timeout | reverse, timeout |
|---|---|---|---|---|
| `js` | existing api/cli command suites + T7 | api `kills a command that exceeds commandTimeout...` | api reverse test + cli reverse pair | T9 |
| `rust`, stub addon | T4 API + CLI, T5 | T4 (records `timeoutMs: 200`); the kill itself is core behavior proven by T2 | T6, T5 error line | carve-out: `negateAsync` wraps the same stub call proven by T6, and the timeout value reaches the addon by T4 |
| `rust-strict`, real addon | T3, T8, T10 | T10 (api kill test) | T10 (api reverse, cli reverse pair) | T9 under `ci:rs` |
| `rust` fallback (`addon: null`) | L1 fallback tests route through the same null check as `js` | carve-out: same branch | carve-out: same branch | carve-out: same branch |
| core (`cargo test`) | T1 | T2 | carve-out: reverse is JS-only (`negateAsync`) | carve-out: same |

---

## Definition of Done

- R-L6-1..R-L6-10 met; T1..T10 present, named for behavior, each seen red for the right reason before green (T7, T8, T9 by the temporary inversions noted in Risks).
- Verification Contract fully green locally and on the PR.
- `test/rust-pending.js` still `[]`; no `.only`/`.skip` in the diff (conditional `this.skip()` in T3 and T8 only).
- `lib/wait-on.js` diff limited to `createCommand$` destructuring `addon`, the check selection and the addon-backed sibling function; `commandPasses`, `negateAsync`, the pipeline operators and the `executing command` line unchanged.
- Guides updated per U3 with the deliberate differences recorded; `docs/plans/**` never deleted; this plan's Resume notes appended; spine and sibling plans untouched.
- Cleanup: no abandoned-attempt code in the diff (no `AsyncTask` variant beside the deferred export, no second fixture file, no env knobs beyond `WAIT_ON_FAKE_COMMAND_OK`, no leftover crate in `Cargo.toml`/`Cargo.lock`); no generated files outside gitignored paths; child processes from T2/T3/T8 exit or are killed by the tests' own bounds.
- `origin/spike-next-rs` merged (no rebase, no force-push) before ready; PR opened against `spike-next-rs` with `Closes #58`; not merged by the lane.

---

## Resume notes

<!-- Lane L6 notes only. Append below; never edit the spine plan or sibling lane plans. -->

- 2026-09-30: U1-U3 implemented on `rs-58-command`. After merging `origin/spike-next-rs` (L2 tcp/socket), `npm test` 359 passing and `npm run ci:rs` green (cargo 29 passed, mocha 359 passing, 0 pending under `rust-strict`, darwin-arm64). Red seen per test: 11 cargo cases vs a stub, grandchild case with the reader deadline removed, T3 `runCommand is not a function`, T4-T6 JS-path timeouts, T7 by selector inversion, T8 against an `AsyncTask` build, T9 with `commandTimeout` dropped to 0. Doc review fixed KTD2 before implementation (Node's exec also returns at the bound when a grandchild holds stdout). Deviation: `run(shell, command, timeout)` test seam added so the spawn-failure branch has a cargo test. Merge note: L2 routes by per-export feature detection (`typeof addon?.tcpCheck`), while `fileSize` (L3) and `runCommand` (L6) route on addon presence per KTD4; aligning the three is left to the spine. Cross-model (codex) passes not run: egress not confirmed this session. Code review verdict Ready to merge; residual risks (thread-spawn panic leaves the deferred unsettled, `commandTimeout` above u32 wraps) recorded in the PR.
