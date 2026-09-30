---
title: "[L6] command: resource in Rust - Requirements"
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
---

# [L6] command: resource in Rust - Requirements

Requirements-only plan for sub-issue #58 (lane L6 of spine #35), written from the issue
body (it had no comments when this was written). The spine plan controls and this lane never
edits it. Authority: `AGENTS.md` > spine KD-S1..KD-S9 > this plan.

## Summary

Move the `command:` check (`commandPasses` in `lib/wait-on.js`) into Rust. Under
`WAIT_ON_ENGINE=rust*` with a loaded addon, the addon runs the command. JS still orchestrates
the rxjs pipeline (KD-S3). The command runs with the same shell semantics as Node's
`child_process.exec` on Linux, macOS and Windows. Exit 0 means ready. `commandTimeout` kills
an attempt at its per-attempt bound. Reverse mode stays in JS through `negateAsync`.

## Problem Frame

Today the addon answers only the `file:` probe (L3). `createCommand$` always calls
`commandPasses`, which uses `util.promisify(child_process.exec)`:

- The shell is `/bin/sh -c <command>` on POSIX. On Windows it is `process.env.ComSpec ||
  cmd.exe` with `/d /s /c "<command>"` and verbatim arguments.
- The child inherits `process.env` and `process.cwd()`.
- Any non-zero exit, a signal, a spawn error, or the `timeout` kill (with
  `killSignal: 'SIGKILL'`) resolves to `false`. Exit 0 resolves to `true`.
- Verbose output logs `executing command "<cmd>" ...`, then
  `  Command "<cmd>" success. stdout: "<stdout>"` or `  Command error: "<message>"`.

The rxjs shape does not change: `exhaustMap` (no self-overlap), `startWith(false)`,
`distinctUntilChanged()`, and `take(2)`.

## Requirements

- **R-L6-1 Core runner.** `wait-on-core` exposes a pure function. It takes a command string
  and an optional per-attempt timeout in ms (0 means no limit). It runs the command through
  the same shell Node's `exec` uses on each OS. It returns whether the command exited 0,
  plus the captured stdout (for the success log line) and an error description (for the
  failure log line). It never panics. A spawn failure, a non-zero exit, a signal death, or
  a timeout all report not-ready.
- **R-L6-2 Timeout.** When `commandTimeout > 0` and the attempt is still running at that
  bound, the child shell is killed (SIGKILL-equivalent; `TerminateProcess` on Windows) and
  the attempt reports not-ready. The same known ceiling as JS applies: only the shell is
  killed, and a detached grandchild can outlive it.
- **R-L6-3 Addon surface.** `wait-on-napi` exports the runner to JS as an async function
  that returns a Promise and does not block the event loop. It follows L3's `AsyncTask`
  precedent, which needs no `tokio`. A command can run for seconds, so a sync export is
  out.
- **R-L6-4 Routing.** `createCommand$` uses the addon's runner when `addon` is non-null.
  Otherwise it calls `commandPasses` exactly as today. It uses the `addon` already threaded
  through deps by L3. It adds no second `resolveEngine`.
- **R-L6-5 Reverse and pipeline unchanged.** `negateAsync`, `exhaustMap`, `startWith`,
  `distinctUntilChanged`, `take(2)` and the `executing command` line stay unchanged. Only
  the check source differs by engine.
- **R-L6-6 Log shape.** Under the Rust engine the verbose success and error lines keep
  their JS shape. The success line carries stdout and the error line carries a
  description. Byte-identical error text is not required, because full log parity belongs
  to L7.
- **R-L6-7 Prove the path ran.** A test shows that under `rust` plus the fixture addon, the
  command check was answered by the addon and not by a silent JS fallback. Per L3's
  technique, the fixture records calls and returns an answer JS cannot produce for the
  given command.
- **R-L6-8 Real-addon parity.** Every existing command test passes under
  `npm run ci:rs` (`rust-strict`, real addon) with `test/rust-pending.js` still empty. That
  covers the six in `api.mocha.js` `describe('command (#87, #15, #71)')`, the five in
  `cli.mocha.js`, and the command cases in `cli-conformance-helper.mocha.js` and
  `parser-properties.mocha.js`. The pending list is already empty on the base, so
  "shrink the pending list" means keeping it empty. Needing to list a command test is a
  stop-and-report, not a quiet add.
- **R-L6-9 JS engine unchanged.** The `npm test` outcome, the `.nycrc.json` coverage
  thresholds, the public API, the CLI, `WAIT_ON_SCHEMA` and `index.d.ts` do not change.
- **R-L6-10 Guides.** The `docs/guides/` pages that list ported checks
  (`architecture.md`, `testing.md`, `contributing-dual-engine.md` as applicable) say that
  under the Rust engine the Rust addon answers `command:`, and where in the code that
  happens.

## Named risks, each with the test that answers it

| Risk | Test |
|---|---|
| Wrong shell on Windows (`cmd.exe /d /s /c` quoting), so `node -e "process.exit(0)"` fails | Existing api/cli command tests under `ci:rs` on the windows row. A cargo test runs a quoted, `&&`-chained command through the core runner on every OS. |
| Exit code misread (for example, a signal treated as success) | Cargo tests: `exit 0` is ready; `exit 3` is not ready; a command killed by timeout is not ready. |
| Timeout does not kill, so `exhaustMap` wedges polling | Cargo test: a sleeping command with a short timeout returns within the bound. The api test `kills a command that exceeds commandTimeout and keeps polling` passes under `ci:rs`. |
| Blocking the event loop stalls `timer(timeout)` | A test asserts that the addon export returns a Promise. The api test `keeps polling to the global timeout...` passes under `ci:rs`. |
| Silent JS fallback looks like Rust success | Fixture-addon tests in `test/engine.mocha.js` check the recorded calls and an outcome JS cannot produce, in both forward and reverse. |
| Env or cwd not inherited | The CLI test `exits 0 when a command starts passing later` (it reads a marker path) passes under `ci:rs`. A cargo test reads an env var set by the parent. |
| Sibling L2/L4 edits to `crates/*/src/lib.rs`, `fake-addon.js`, `engine.mocha.js`, guides | All L6 edits are additive. Merge `origin/spike-next-rs` before ready and rerun the full verification. |

## Scope Boundaries

- Allowed paths come from the lane contract (#58). No `.github/workflows/` edits; if one
  is needed, stop and report. No new npm runtime dependency. Prefer `std::process` (no
  `tokio`, no new crate). If a timeout without a crate is impractical, record the reason
  before adding one.
- Not in scope: moving the polling loop to Rust (L7), tcp/socket (L2), http (L4), and
  byte-exact verbose error text (L7).
- Edits to shared files stay small and additive. Those files are `crates/*/src/lib.rs`,
  `lib/wait-on.js` `createCommand$`, `test/fixtures/fake-addon.js`,
  `test/engine.mocha.js`, and `docs/guides/`.

## Definition of Done

- `npm test` is green, `npm run ci:rs` is green with an empty pending list,
  `cargo test`/`clippy` are green, and coverage thresholds hold.
- Guides are updated, this plan's Resume notes are appended, and the spine and sibling
  plans are untouched.
- `origin/spike-next-rs` is merged in (no rebase, no force-push) before ready. A PR against
  `spike-next-rs` on kevinold/wait-on carries `Closes #58`, every check is green, and the
  lane does not merge it.

---

## Resume notes

<!-- Lane L6 notes only. Append below; never edit the spine plan or sibling lane plans. -->
