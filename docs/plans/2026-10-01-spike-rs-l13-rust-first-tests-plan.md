---
title: "[L13] Rust-first engine test suite; slim JS to front-door parity - Plan"
type: test
date: 2026-10-01
topic: rust-port
lane: L13
kind: preview
branch: rs-76-rust-first-tests
closes: kevinold/wait-on#76
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L13] Rust-first engine test suite; slim JS to front-door parity - Plan

Requirements-only lane plan for sub-issue #76, authored from the issue body (no comments) at base
`9ae272c`. The spine plan is controlling and is never edited by this lane.

---

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`, Rust tests in `crates/wait-on-core/tests/`
are the specification of engine behaviour for every resource type, the Rust crates are at 100% line
and region coverage enforced by `npm run ci:rs`, and the JS test side keeps only front-door parity
(API / CLI / conformance / property suites under both engines). JS tests that exercise Rust
internals through fake addons are gone.

**Direction (operator, 2026-10-01).** Rust is the primary implementation; the JavaScript engine and
JS tooling are what get sunset. Avoidable JavaScript moves to Rust.

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions** (report to the PM, do not work around):
- a `.github/workflows/` change is needed. Known: CI's `rust` job does not install `cargo-llvm-cov`
  or `llvm-tools-preview`. That install is an operator change (issue text); the lane adds the
  `rust-toolchain.toml` component and wires the gate, and the PR body tells the PM the CI install
  is required before `ci:rs` can go green in CI;
- a public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change is needed;
- a new npm runtime dependency is needed;
- a needed edit lies outside the allowed paths (notably `xtask/` is **not** allowed; the coverage
  gate is wired through the `ci:rs` script in `package.json`, using the existing
  `cargo xtask cov` passthrough).

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`; one PR,
base `spike-next-rs` on `kevinold/wait-on`, body `Closes #76`; never merges. Allowed paths:
`crates/`, `Cargo.toml`, `Cargo.lock`, `supply-chain/`, `deny.toml`, `test/`, `lib/`,
`package.json`, `package-lock.json`, `.nycrc.json`, `rust-toolchain.toml`, `docs/guides/`,
`docs/plans/`, `docs/solutions/`, `AGENTS.md`.

---

## Product Contract

### Starting state (base `9ae272c`)

- `crates/wait-on-core/src/`: `lib.rs` (file size, command), `tcp.rs`, `socket.rs`, `http.rs`,
  `parse.rs`, `waiter.rs` + `waiter/tests.rs`; one integration file `tests/loop.rs`.
- `crates/wait-on-napi/src/`: `lib.rs`, `wait.rs`, `http.rs`, `build.rs`.
- `test/rust-pending.js`: `pending = []` already, with the hook machinery and
  `test/rust-pending.mocha.js` + `test/fixtures/rust-pending/` still present.
- Fake addons: `test/fixtures/fake-addon.js`, `test/fixtures/counting-addon.js`, used by
  `test/engine.mocha.js`, `test/api.mocha.js`, `test/https-proxy.mocha.js`,
  `test/prebuild-probe.mocha.js`; `test/engine-checks.mocha.js` exercises Rust checks via JS.
- `ci:rs` = `cargo vet --locked && cargo xtask ci`; `cargo xtask cov [args]` runs
  `cargo llvm-cov --workspace [args]`. `rust-toolchain.toml` components: `rustfmt`, `clippy`.

### Requirements

- **R-L13-1** Integration suites under `crates/wait-on-core/tests/` for each resource type, each in
  forward and reverse mode, against real local files / listeners / servers on ephemeral ports:
  file (including `window` size stabilization), tcp (including IPv6 `[::1]`), unix socket (Windows
  named pipe where the platform has it, skip-not-fail otherwise), http/https HEAD and GET
  (status-code handling, headers, auth, TLS options `ca`/`cert`/`key`/`passphrase`/`strictSSL`,
  explicit and env proxies incl. `NO_PROXY`), http-over-unix-socket, command.
- **R-L13-2** Timing in those suites uses `tokio::time::pause` (virtual clock), not real sleeps,
  wherever the code under test runs on tokio time. Where a check is driven by real OS time (e.g. a
  child-process timeout) the test uses generous headroom and says why.
- **R-L13-3** 100% coverage gate: `cargo llvm-cov` over the workspace excluding only `xtask/` and
  `build.rs`, with `--fail-under-lines 100 --fail-under-regions 100`, run by `npm run ci:rs`.
  Branch coverage is reported where the toolchain supports it (nightly-only `--branch` is reported
  as unavailable on the pinned stable toolchain, not faked). Lines that cannot be exercised are
  removed or restructured, never excluded with attributes or regexes beyond the two named paths.
- **R-L13-4** `rust-toolchain.toml` adds `llvm-tools-preview` to `components`.
- **R-L13-5** Delete the fake-addon fixtures (`test/fixtures/fake-addon*.js`,
  `test/fixtures/counting-addon.js`) and the JS cases that use them or that exercise Rust internals
  (`test/engine.mocha.js`, `test/engine-checks.mocha.js`, and fake-addon cases elsewhere) — each
  deletion only after a named Rust test covers the same behaviour (inventory in R-L13-8). Engine
  *selection* behaviour reachable from the front door (`WAIT_ON_ENGINE` values, missing-addon
  error) stays as JS tests, rewritten against the real addon if they relied on a fake.
- **R-L13-6** Every API / CLI / conformance / property test passes under `WAIT_ON_ENGINE=rust-strict`
  and under the default JS engine; JS engine behaviour unchanged.
- **R-L13-7** The Rust pending list is retired: `test/rust-pending.js`,
  `test/rust-pending.mocha.js`, `test/fixtures/rust-pending/` and their mocha wiring are deleted,
  and doc references updated (shared file; small, additive edits).
- **R-L13-8** `docs/guides/testing.md` records a JS-vs-Rust test inventory (what is Rust-specified,
  with file/test names; what stays JS parity, and why) and the rule for new work: engine
  behaviour → Rust test first; JS only at the API/CLI front doors.
- **R-L13-9** The parser differential (#245 vectors in `test/parser-properties.mocha.js`) agrees on
  every input between engines.
- **R-L13-10** `npm test` and `npm run ci:rs` pass locally; `npm run` script names CI calls are
  unchanged.

### Test scenarios (named)

- **T-L13-1** each R-L13-1 cell is a named Rust test (`<kind>_<forward|reverse>_<behaviour>`).
- **T-L13-2** `cargo xtask cov -- <gate args>` fails on the base (coverage < 100%) — RED for the
  gate — and passes at the end.
- **T-L13-3** `npm run test:mocha` under `WAIT_ON_ENGINE=rust-strict` is green with no pending list.

### Risks

- Coverage of napi glue (`wait-on-napi`) needs a Rust-side test path that does not need Node;
  restructure glue into testable pure functions where needed.
- Platform-specific `cfg` branches (Windows pipes vs unix sockets) are uncovered on any single
  host; `llvm-cov` on macOS/Linux will not see Windows lines. Resolution belongs in the deepened
  plan (e.g. keep platform code minimal and shared, report-only per-host gaps must not exist under
  the 100% rule — restructure).
- CI cannot enforce the gate until the operator installs `cargo-llvm-cov`; flagged to PM.

---

## Resume notes

- 2026-10-01: plan authored from #76 body; issue had no comments. Pending list already empty at base.
