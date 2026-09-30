---
title: "[L3] file: resource check in Rust - Plan"
type: feat
date: 2026-09-30
topic: rust-port
lane: L3
kind: preview
branch: rs-55-file
closes: kevinold/wait-on#55
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# [L3] file: resource check in Rust - Plan

Implementation-ready lane plan for sub-issue #55 (lane L3 of spine #35). Product Contract preservation: requirements R-L3-1..R-L3-8 and tests T1..T7 keep the meaning of the requirements-only plan; sections were restructured to the unified-plan shape, the two Outstanding Questions are resolved in KTD2 and KTD4, and the test matrix moved under the Verification Contract. The spine plan is controlling and is never edited by this lane.

---

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`: under `WAIT_ON_ENGINE=rust` with a loaded addon, every `file:` (and bare-path) size probe is answered by Rust; the JS engine, `npm test`, coverage thresholds, public API, CLI, `WAIT_ON_SCHEMA` and `index.d.ts` are unchanged; `npm run ci:rs` runs every file test (stability `window`, reverse, Windows delete-pending) under `rust-strict` with `test/rust-pending.js` still empty. Proves PO10.

**Means.** One pure function in `crates/wait-on-core` (`std::fs::metadata`, any error is `-1`), one async napi export `fileSize(path)` returning a Promise of a number, and a one-line probe selection in `createFileResource$` keyed on the addon `waitOnImpl` already resolves (KTD1, KTD2, KTD3). The rxjs pipeline and the stabilization `scan` are untouched. A recording JS stub addon proves the Rust route ran (KTD4).

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions.** A `.github/workflows/` change is needed -> stop and report to the PM. A public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change would be needed -> stop. A new runtime dependency would be needed -> stop. A file test must be added to `test/rust-pending.js` -> stop and report (R-L3-6).

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`; one PR, base `spike-next-rs` on `kevinold/wait-on`, body `Closes #55`; never merged by the lane; merge `origin/spike-next-rs` (no rebase, no force-push) before marking ready.

---

## Product Contract

### Summary

Move the `file:` size probe (`getFileSize` in `lib/wait-on.js`) into Rust and route the Rust engine through it while the JS rxjs pipeline keeps orchestrating (KD-S3). The stabilization `window` logic (the `scan` in `createFileResource$`) stays in JS and is not touched, so its timing behavior is byte-identical by construction.

### Problem Frame

Under `WAIT_ON_ENGINE=rust*` today the addon loads but `waitOnImpl` discards the `resolveEngine` result and every check is still JS. L3 makes Rust answer "what is this file's size?" for `file:` resources and bare paths (the `default` branch of the `createResource$` switch). The contract is `getFileSize`: resolve to the file's `size` from a symlink-following `stat`, or `-1` on any error. Reverse mode relies on that contract: on Windows a delete-pending file keeps `stat`-ing successfully with its real size, and an `EPERM` during delete is reported as `-1`, never thrown (reverse-file comment in `test/api.mocha.js`).

### Requirements

- R-L3-1 Rust probe (core). `wait-on-core` exposes a pure function returning a file's size as `i64`, `-1` on any error (missing, `EPERM`/`EACCES`, `ENOTDIR`, delete-pending errors), never panicking or erroring. It follows symlinks like `fs.promises.stat` (a dangling symlink is `-1`). Directories report their `stat` size like Node does.
- R-L3-2 Addon surface. `wait-on-napi` exports it to JS as `fileSize(path)`. It must not block the Node event loop per call longer than a `stat`; KTD2 makes it an async task returning a Promise of a number.
- R-L3-3 Routing. When `resolveEngine` returns a loaded addon, the file resource's size probe calls `addon.fileSize`; otherwise `getFileSize` (JS) runs exactly as today. The engine resolved once in `waitOnImpl` is passed into the pipeline; no second resolve per poll. `rust` with a failed load (JS fallback, `addon: null`) uses JS.
- R-L3-4 Window unchanged. `createFileResource$`'s `scan`, reverse `map`, `window` floor to `interval`, and verbose `output(...)` lines are unchanged. Only the probe source differs by engine.
- R-L3-5 Prove the path ran. Tests show the Rust probe answered under the Rust engine (not a silent JS fallback), per AGENTS.md "Prove the path ran".
- R-L3-6 Pending list. `test/rust-pending.js` stays empty; every file test in `api.mocha.js`, `cli.mocha.js`, `cli-conformance*.mocha.js` passes under `rust-strict`. If a file test must be listed, that is a stop-and-report, not a quiet add.
- R-L3-7 JS engine unchanged. `npm test` outcome, coverage thresholds, public API, CLI, `WAIT_ON_SCHEMA`, `index.d.ts` unchanged.
- R-L3-8 Guides. `docs/guides/architecture.md` and `docs/guides/testing.md` (and any page listing ported checks) say `file:` is answered by Rust under the Rust engine, and where.

### Scope Boundaries

- Allowed paths per lane contract: `crates/`, `Cargo.toml`, `Cargo.lock`, `lib/`, `bin/`, `test/`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `package.json`, `scripts/`, `AGENTS.md`, `README.md`. No `.github/workflows/` edits (stop condition). No new runtime dependency; no new crate dependency (`std::fs::metadata` and napi's own `AsyncTask` suffice; no `tokio`).
- Not in scope: moving the polling/`window` loop to Rust (L7); tcp/socket (L2, sibling running in parallel). Edits to shared spots (`crates/*/src/lib.rs`, the `waitOnImpl` deps line in `lib/wait-on.js`, `test/fixtures/fake-addon.js`, `test/engine.mocha.js`, `docs/guides/`) stay small and additive so L2's merge is mechanical.
- Non-goals (considered, not built): a guard for a stale local prebuild that lacks `fileSize` (rebuild with `npm run build:napi`; the TypeError still arrives through the callback because it fires inside `mergeMap`); passing the engine name string into deps (addon presence is the only fact the pipeline needs); a `--verbose` line naming the engine (log parity is L7's); a stat-faithful fake addon (KTD4 explains why a constant stub is stronger evidence).

### Sources

- Issue kevinold/wait-on#55 (no comments at authoring time).
- Spine plan KD-S1, KD-S2, KD-S3, KD-S7, KD-S9; lane row L3. Source plan PO10, R1, R5. Sibling lane plan `docs/plans/2026-09-30-spike-rs-l1-scaffold-plan.md` (`resolveEngine` shape, `WAIT_ON_NATIVE_LIBRARY_PATH`, fixture addon).
- `lib/wait-on.js` (`waitOnImpl`, `createResource$`, `createFileResource$`, `getFileSize`), `lib/engine.js`, `crates/wait-on-core/src/lib.rs`, `crates/wait-on-napi/src/lib.rs`, `crates/wait-on-napi/Cargo.toml` (`napi = "3"`, `default-features = false`, `features = ["napi4"]`, `test = false`), `test/engine.mocha.js` (`withEnv`, `runCLI`, `OPTS` waits on `__filename`), `test/fixtures/fake-addon.js`, `test/rust-pending.js`, `test/frozen-clock.js`, `test/api.mocha.js` file tests, `scripts/ci-rs.js`, `.nycrc.json`, `docs/guides/architecture.md`, `docs/guides/testing.md`, `docs/guides/contributing-dual-engine.md`.

---

## Planning Contract

### Key Technical Decisions

- KTD1. Probe in core: `wait_on_core::file_size(path) -> i64` = `std::fs::metadata(path)` mapped to `len()` as `i64`, any `Err` to `-1`. `metadata` follows symlinks like `fs.stat`; directories report the OS `st_size` on both engines because both read the same syscall. One "any `Err` -> -1" branch, so missing, `ENOTDIR`, `EPERM` and Windows delete-pending errors share one code path (the T2 carve-out). (session-settled: user-directed — chosen over surfacing or throwing errors: reverse mode and Windows delete-pending rely on `-1`.)
- KTD2. Async napi export via napi's `AsyncTask` (libuv threadpool, no `tokio`): JS sees `fileSize(path): Promise<number>`; the task's compute calls `wait_on_core::file_size` and resolve passes the `i64` through. Chosen over a sync `#[napi] fn`: a sync stat blocks the event loop, so a hung stat (stale network mount) would also stall `timer(timeout)`, a JS-vs-Rust behavior delta; async keeps `getFileSize`'s Promise shape, so the rxjs call site stays `from(probe(filePath))` and `simultaneous` semantics are identical. Chosen over `#[napi] async fn`: that needs the `tokio_rt` feature and a runtime for a single stat. T3 pins the Promise shape.
- KTD3. Routing keyed on addon presence, resolved once. `waitOnImpl` keeps the `resolveEngine(process.env)` result (today it is discarded) and adds `addon` to the deps object passed to `createResource$`; `createFileResource$` uses `addon.fileSize` when `addon` is non-null, else `getFileSize`. `addon` is `null` under `js` and under `rust` fallback, so R-L3-3's three cases collapse into one null check; the engine name is not threaded. The verbose `checking file stat for file:...` line and everything after `mergeMap` are unchanged. (session-settled: user-directed — window/`scan` logic stays in JS byte-identical, chosen over moving the loop to Rust now: KD-S3 assigns the loop to L7.) (session-settled: user-directed — JS stays default and behavior-unchanged, chosen over changing the default: side-by-side spike, KD-S1.)
- KTD4. Stub-addon technique, and keeping L1's fixture tests green. `test/fixtures/fake-addon.js` (the file `WAIT_ON_NATIVE_LIBRARY_PATH` already points at in `test/engine.mocha.js`) gains an async `fileSize(path)` that pushes `path` onto an exported `calls` array and resolves `Number(process.env.WAIT_ON_FAKE_FILE_SIZE ?? 1)`. The constant default matters twice: the L1 tests that run `waitOn` with this fixture wait on `__filename` in forward mode with `window: 100`, and a constant non-negative size stabilizes within one window, so they keep passing once routing lands; and a missing path resolving to `1` is an outcome JS cannot produce, so `waitOn` succeeding on a missing file under `rust` + fixture proves the Rust route ran (T4). `WAIT_ON_FAKE_FILE_SIZE=-1` gives the reverse cell (T6) an "existing file reported gone", again impossible under JS. In-process tests read the same module instance `resolveEngine` required (require cache keys on the absolute path), so `calls` is the path proof. CLI subprocesses cannot share `calls`, so the CLI cell proves the route by exit-code contrast (0 under `rust` + fixture, 1 under `js`). Chosen over a stat-faithful fake (would need a second fixture for T4/T6 and proves nothing JS could not) and over path-keyed magic answers (an env knob is explicit and `withEnv` restores it).
- KTD5. New tests live in `test/engine.mocha.js` under a new `describe('file: probe routing')`, reusing `withEnv` and `runCLI`; `ENGINE_VARS` gains `WAIT_ON_FAKE_FILE_SIZE` (existing callers never set it) and `runCLI` takes an optional resource argument defaulting to `__filename`. Chosen over a new spec file: it would duplicate the env-scoping helpers, and engine routing is an engine concern.
- KTD6. Cargo tests need no crate: temp paths are `std::env::temp_dir()` joined with the process id and a test name; unix-only cases (`symlink`, `chmod 000`) are `#[cfg(unix)]`; the permission case skips itself when `std::fs::metadata` on the locked path succeeds (root), and restores permissions before asserting so cleanup always runs. Chosen over `tempfile`: no new crate dependency.
- KTD7. Lane contract. (session-settled: user-directed — no `.github/workflows/` edits, no new runtime dependency, stay inside allowed paths; chosen over workflow edits in the lane: KD-S7.) PR base `spike-next-rs` on kevinold/wait-on, `Closes #55`, never merged by the lane.

### Assumptions

- napi-rs 3's `AsyncTask`/`Task` compile under `napi = { version = "3", default-features = false, features = ["napi4"] }` (async work is N-API v1). Verified by `cargo clippy` in U1; if a feature flag is missing, enable that flag on the existing `napi` dependency (still no `tokio`).
- napi maps `i64` to a JS `number` (file sizes above 2^53 are not a concern for a stat probe).
- Paths cross the boundary as napi `String` (UTF-8). A Windows path with an unpaired surrogate is lossy in Rust where `fs.stat` would accept it; recorded as a known ceiling, not guarded.
- `fs::metadata` on a Windows delete-pending file either succeeds with the real size (like Node, keep polling) or errors (`-1`, reverse succeeds); both are outcomes the existing reverse test tolerates with its 15s headroom and retries.
- Under `itFrozen`, the Rust probe resolves off the libuv threadpool exactly as `fs.promises.stat` does today.
- Sibling L2 threads the addon into deps with the same shape; if its key differs when merging `origin/spike-next-rs`, adopt L2's key (mechanical rename, tests unchanged).

### Risks

| Risk | Answered by |
|---|---|
| Routing lands before the fixture exports `fileSize`, breaking L1's fixture-addon tests (`addon.fileSize is not a function`) | U2 order: T4 red, routing green, the L1 fixture tests go red for that exact reason, fixture gains `fileSize` in the same commit |
| Silent JS fallback masquerading as Rust success | T4 (missing file succeeds only via the stub), T6 (existing file reported gone), `calls` assertions, CLI exit contrast |
| A sync export would stall `timer(timeout)` on a hung stat | KTD2 async; T3 asserts the return is a Promise |
| Windows delete-pending under `fs::metadata` differs from Node | KTD1 single `-1` branch; T7 reverse "not available later" test under the `ci:rs` windows row |
| Frozen-clock file tests and the threadpool probe | T7 `itFrozen` file tests under `ci:rs` on all three OSes |
| Coverage: the `addon` branch unreachable in the JS-only run | T4/T6 run it in-process via the fixture; `npm run test:coverage` keeps `.nycrc.json` thresholds |
| L2 parallel edits to `waitOnImpl` deps, `crates/*/src/lib.rs`, `fake-addon.js`, `engine.mocha.js`, guides | All L3 edits additive; merge `origin/spike-next-rs` before ready and rerun the Verification Contract |
| T5 is green on first run | Expected: it pins the untouched `js` branch; after U2 GREEN, temporarily invert the selector and confirm T5 goes red, then restore |

---

## Implementation Units

Order: U1 -> U2 -> U3. Test and code land in the same Conventional Commit. U1 first so `ci:rs` is green at every commit (routing before the export would fail `rust-strict` with a missing `fileSize`).

### U1. Rust probe in core and async napi export

- **Goal.** `wait_on_core::file_size` exists with the `getFileSize` contract; the built addon answers `fileSize(path)` with a Promise of a number.
- **Requirements.** R-L3-1, R-L3-2.
- **Dependencies.** None.
- **Files.** `crates/wait-on-core/src/lib.rs`, `crates/wait-on-napi/src/lib.rs`, `test/engine.mocha.js` (T3).
- **Approach.** KTD1, KTD2, KTD6. Core: one public function plus the T1/T2 cases in its `#[cfg(test)]` module. Napi: a task type, its `Task` impl (compute delegates to core, resolve passes through), and the `#[napi]` export returning `AsyncTask`. No `Cargo.toml` change unless the feature assumption fails. T3 requires `addonPath({})` directly (skips when the file does not exist) and writes a temp file with known content under `os.tmpdir()`.
- **Patterns to follow.** `version_is_workspace_version` test in `crates/wait-on-core/src/lib.rs`; the skip-if-no-prebuild pattern of "should load the real addon and answer version() when a host prebuild exists" in `test/engine.mocha.js`.
- **Test scenarios.**
  - T1 (`cargo test`, `crates/wait-on-core`): existing file written with 5 bytes -> `5`; missing path -> `-1`; a path under a regular file (`ENOTDIR`) -> `-1`; a directory -> `>= 0`; `#[cfg(unix)]` dangling symlink -> `-1`. RED: `cargo test --workspace` fails to compile (no `file_size`).
  - T2 (`cargo test`, `#[cfg(unix)]`): file inside a directory chmod `000` -> `-1`; returns early (skip) when `fs::metadata` on that path succeeds (root); permissions restored before the assert. Carve-out: Windows delete-pending cannot be forced deterministically; KTD1's single `Err -> -1` branch means T1's missing-path cell exercises the same code path.
  - T3 (`test/engine.mocha.js`, "should answer fileSize from the built addon"): `this.skip()` unless `fs.existsSync(addonPath({}))`; `addon.fileSize(tmpFile)` is a Promise resolving to the written byte length; `addon.fileSize(missing)` resolves `-1`. RED with a local pre-L3 build: `addon.fileSize is not a function`.
- **Verification.** Cargo fmt, clippy `-D warnings`, test and deny green; after `npm run build:napi`, T3 runs (not skipped) and passes; `npm test` unchanged.

### U2. Route the file probe by engine, recording stub addon

- **Goal.** Under a loaded addon, `createFileResource$` probes through `addon.fileSize`; under `js` and `rust` fallback it uses `getFileSize`; L1's fixture-addon tests keep passing.
- **Requirements.** R-L3-3, R-L3-4, R-L3-5, R-L3-7.
- **Dependencies.** U1.
- **Files.** `lib/wait-on.js` (`waitOnImpl` keeps the resolved engine and adds `addon` to deps; `createFileResource$` selects the probe), `test/fixtures/fake-addon.js` (`calls`, async `fileSize`), `test/engine.mocha.js` (`ENGINE_VARS`, `runCLI` resource argument, new `describe`).
- **Approach.** KTD3, KTD4, KTD5. Missing path for T4/T5 = a nonexistent file under `os.tmpdir()` with the pid in its name; both a bare path and the same path with a `file:` prefix. T6 uses `__filename` with `reverse: true`. Plain `it` (fixed sizes, no time dependence worth freezing).
- **Patterns to follow.** `withEnv`/`runCLI`/`callbackError` in `test/engine.mocha.js`; the exit-code contrast of "should exit 0 from the CLI with the same output as js when the addon is missing".
- **Test scenarios (`test/engine.mocha.js`, describe "file: probe routing").**
  - T4 "should succeed on a missing file under rust when the stub addon answers the probe" (API): `WAIT_ON_ENGINE=rust`, `WAIT_ON_NATIVE_LIBRARY_PATH` = fixture, resources `[missingPath]`, `timeout: 1000`, `interval: 100`, `window: 100`; resolves; the fixture's `calls` includes `missingPath`. Repeat with `file:` + missingPath; `calls` includes the stripped path. RED: `Timed out waiting for: <path>`.
  - T4 (CLI) "should exit 0 from the CLI on a missing file under rust with the stub addon, and 1 under js": the rust spawn exits 0; the same spawn with `WAIT_ON_ENGINE: 'js'` exits 1 with `Timed out` on stderr. RED: both exit 1.
  - T5 "should never call the stub and time out on a missing file under js": `WAIT_ON_ENGINE=js` + fixture path, `timeout: 300`; rejects with a message starting `Timed out waiting for`; `calls.length` unchanged. Green on first run by design (see Risks).
  - T6 "should succeed in reverse mode on an existing file when the stub reports -1 under rust": `WAIT_ON_ENGINE=rust`, fixture, `WAIT_ON_FAKE_FILE_SIZE=-1`, resources `[__filename]`, `reverse: true`, `timeout: 1000`; resolves; `calls` includes `__filename`. RED: JS sees the real size and times out.
  - Existing L1 tests "should take the addon-present branch with a fixture addon" and "should exit 0 from the CLI with a loadable fixture addon": red after routing (`addon.fileSize is not a function`), green once the fixture exports `fileSize`.
- **Verification.** `npm test`; `npm run test:coverage` meets `.nycrc.json` thresholds with the `addon` branch of `createFileResource$` covered; `npm run ci:rs` green locally.

### U3. Full-suite gate under Rust and guides

- **Goal.** Every existing file test passes under `rust-strict` with the real addon on ubuntu, macos and windows; the guides state that `file:` is answered by Rust and where.
- **Requirements.** R-L3-6, R-L3-8.
- **Dependencies.** U1, U2.
- **Files.** `docs/guides/architecture.md` ("Rust engine layout": the addon exposes `version()`, `noop()`, `fileSize(path)`; replace "No resource check runs in Rust yet" with the file probe description; "Resource checks in Rust": `file:` (L3) done, window/loop still JS until L7), `docs/guides/testing.md` (fixture paragraph: `fake-addon.js` records `calls`, answers `fileSize` with `WAIT_ON_FAKE_FILE_SIZE` default `1`; engine test row adds file probe routing and the real-addon `fileSize` case), `docs/guides/development.md` (one sentence: rebuild the addon after pulling; a stale prebuild without `fileSize` errors under `rust`). `test/rust-pending.js` untouched.
- **Approach.** Docs-only carve-out for the guide edits. T7 is the gate: run `npm run ci:rs` locally and read the mocha output for the file tests; the PR's `rust` job repeats it on all three OSes.
- **Patterns to follow.** `docs/guides/contributing-dual-engine.md` docs-as-done checklist; the status-marker style already in the guides.
- **Test scenarios.**
  - T7: `npm run ci:rs` green; its mocha pass shows T3 executed, 0 pending, and the api file tests (frozen "available", "become available later", reverse "not available later", reverse timeout, promise twins) plus the `cli-conformance` file vectors passing under `rust-strict`. A red file test here is a stop-and-report (R-L3-6), never a pending-list add.
  - Docs check: no "No resource check runs in Rust yet" sentence remains in `docs/guides/`.
- **Verification.** `npm run ci:rs` locally; PR `rust` job green on ubuntu, macos, windows.

---

## Verification Contract

Run from the repo root; all must pass before the PR opens, and again after merging `origin/spike-next-rs`.

- `npm test`: lint, types, mocha under JS; outcome unchanged apart from the new tests.
- `npm run test:coverage`: `.nycrc.json` thresholds met with the new `createFileResource$` branch covered by the fixture tests.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (T1, T2 run), `cargo deny check`.
- `npm run build:napi`, then T3 runs (not skipped).
- `npm run ci:rs`: green; mocha pass under `rust-strict` shows 0 pending and every file test passing (T7).
- CI on the PR: `build` (ubuntu+windows x node 22/24/26), `rust` (ubuntu, macos, windows), all `napi` rows, `package`; commitlint and PR-title checks green.

Matrix coverage (engine x mode x prefix):

| Engine | forward, bare path | forward, `file:` | reverse, bare path | reverse, `file:` |
|---|---|---|---|---|
| `js` | existing api/cli suites + T5 | existing api "should log timeout error when log is enabled" (`file:` missing path) | existing api suite | carve-out: same `extractPath`, same JS probe |
| `rust`, stub addon | T4 API + CLI | T4 (`file:` variant) | T6 | carve-out: prefix stripping proven by T4 `file:` cell, reverse `map` proven by T6 |
| `rust-strict`, real addon | T7 + L1 real-addon test on `__filename` | carve-out: prefix stripping is shared JS `extractPath`, proven by the T4 `file:` cell; real probe proven by T3/T7 bare path | T7 (incl. Windows delete-pending) | T7 |
| `rust` fallback (`addon: null`) | L1 fallback tests (poison, junk) route through the same null check as `js` | carve-out: same branch | carve-out: same branch | carve-out: same branch |

---

## Definition of Done

- R-L3-1..R-L3-8 met; T1..T7 present, named for behavior, each seen red for the right reason before green (T5 by the post-GREEN inversion noted in Risks).
- Verification Contract fully green locally and on the PR.
- `test/rust-pending.js` still `[]`; no `.only`/`.skip` in the diff (conditional `this.skip()` in T3 and the root-skip guard in T2 only).
- `lib/wait-on.js` diff limited to keeping the resolved engine, one deps key and the probe selection; `scan`, reverse `map`, verbose lines unchanged.
- Guides updated per U3; `docs/plans/**` never deleted; this plan's Resume notes appended; spine and sibling plans untouched.
- Cleanup: no abandoned-attempt code in the diff (no sync export beside the async one, no second fixture file, no env knobs beyond `WAIT_ON_FAKE_FILE_SIZE`); no generated files outside gitignored paths; temp files from T1/T2/T3 removed by the tests.
- `origin/spike-next-rs` merged (no rebase, no force-push) before ready; PR opened against `spike-next-rs` with `Closes #55`; not merged by the lane.

---

## Resume notes

<!-- Lane L3 notes only. Append below; never edit the spine plan or sibling lane plans. -->
