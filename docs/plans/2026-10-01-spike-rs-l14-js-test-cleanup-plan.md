---
title: "[L14] Delete spike JS tests already covered by Rust - Plan"
type: test
date: 2026-10-01
topic: rust-port
lane: L14
kind: preview
branch: rs-91-js-test-cleanup
closes: kevinold/wait-on#91
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L14] Delete spike JS tests already covered by Rust - Plan

Lane plan for sub-issue #91 at base `832c588`. The spine plan is controlling and is never edited by
this lane.

Product Contract preservation: changed. The first version of #91 asked to port JS tests to Rust
(an xtask `bench-ffi`, Rust probe tests, moved `engine.mocha.js` cases). The operator narrowed it
on 2026-10-01 to a delete-only audit; this plan follows the narrowed issue and drops the porting
requirements.

---

## Goal Capsule

**Objective.** No JS test file created during the spike stays in the repo when every behaviour it
asserts is already specified by a Rust test; files that still prove something Rust does not stay,
with the uncovered assertion named.

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S15 > this plan.

**Stop conditions:** a `.github/workflows/` change is needed; an edit outside the allowed paths
(`test/`, `benchmarks/`, `xtask/`, `crates/`, `Cargo.*`, `supply-chain/`, `package*.json`,
`eslint.config.mjs`, `.nycrc.json`, `.npmignore`, `docs/guides|plans|solutions/`) is needed.

**Execution profile.** One PR on `kevinold/wait-on`, base `spike-next-rs`, body `Closes #91`;
never merges. No porting, no new tooling, no new tests.

---

## Product Contract

### Requirements

- **R-L14-1 Scope.** In scope: files from
  `git diff --diff-filter=A origin/next...origin/spike-next-rs -- test benchmarks`. Files that
  already existed on `next` are out of scope.
- **R-L14-2 Audit.** Map each assertion of each in-scope test file to an existing Rust test
  (`crates/*/src`, `crates/*/tests`, `xtask`). Delete the file, and any fixture or helper only it
  uses, when every assertion is covered. Otherwise keep it and name the uncovered assertion in the
  PR body.
- **R-L14-3 Gates.** Rust coverage stays at 100%; `npm test` and `cargo xtask ci` stay green.
- **R-L14-4 Docs.** `docs/guides/testing.md` inventory reflects the outcome.

---

## Audit (base `832c588`)

In-scope files: `benchmarks/http-ffi.js`, `benchmarks/startup-baseline.json`,
`test/benchmarks.mocha.js`, `test/engine.mocha.js`, `test/prebuild-probe.mocha.js`,
`test/rust-scaffold.mocha.js`, `test/fixtures/{counting-addon,extra-ca-api,hung-http-api}.js`,
`test/helpers/{engine-env,stub-proxy,tls-fixture}.js`.

| File | Verdict | Uncovered assertion (example; no Rust test asserts it) |
|---|---|---|
| `test/rust-scaffold.mocha.js` | keep | "pins the same Rust version in rust-toolchain.toml and the workspace rust-version": no Rust test reads `rust-toolchain.toml`, `.cargo/config.toml` or the `package.json` `ci:rs`/`build:napi`/`ci:rs:package`/`bench:startup`/`lint` strings; same for `llvm-tools-preview`, the `xtask` alias and the `scripts/` contents |
| `test/benchmarks.mocha.js` | keep | "should summarize samples as median and p95" and the `js`/`rust-strict` smoke runs: they exercise `benchmarks/http-ffi.js`; `xtask/src/bench.rs` tests only the startup benchmark's `median` (no p95) and never runs `http-ffi.js` |
| `test/prebuild-probe.mocha.js` | keep | "should print the loaded addon path and pass API and CLI checks against the host prebuild" (and the timeout and missing-addon cases): `xtask/src/package.rs` tests check container cell arguments and the docker context copy of the probe, but never execute it |
| `test/engine.mocha.js` | keep | "should fall back to JS silently when the addon is missing", "should reject naming the value and the allowed values", "should load neither rxjs nor undici on a bare require": engine selection, load errors and the module graph live in `lib/engine.js` / `lib/wait-on.js`; no Rust test can observe them |
| `benchmarks/http-ffi.js` | keep | not a test; the tool `test/benchmarks.mocha.js` (kept) exercises, cited in `docs/guides/architecture.md` |
| `benchmarks/startup-baseline.json` | keep | not a test; read by `cargo xtask bench-startup` (`xtask/src/bench.rs`) |
| `test/fixtures/counting-addon.js` | keep | used by `engine`, `api`, `https-proxy` mocha files and `extra-ca-api.js` |
| `test/fixtures/extra-ca-api.js`, `hung-http-api.js` | keep | used by `test/engine.mocha.js` (kept) |
| `test/helpers/engine-env.js` | keep | used by `engine`, `frozen-clock`, `https-proxy`, `cli` mocha files |
| `test/helpers/stub-proxy.js` | keep | used by `https-proxy`, `cli` mocha files |
| `test/helpers/tls-fixture.js` | keep | used by `engine`, `https-proxy` mocha files and `extra-ca-api.js` |

Outcome: no file is deletable under R-L14-2. The lane changes only docs: this plan and a
`docs/guides/testing.md` note recording the audit. No code, test or config change, so the TDD cycle
has nothing to run; the gates (R-L14-3) are unchanged by construction and CI confirms them.

---

## Resume notes

- 2026-10-01: first plan version (porting scope) authored from #91; superseded the same day by the
  operator's delete-only clarification of #91. No porting work had started.
- 2026-10-01: audit done at `832c588` (table above). Rust-side search: `xtask/src/*.rs`,
  `xtask/tests/*.rs`, `crates/*/src`, `crates/*/tests` for the files and strings each JS assertion
  reads. Nothing deleted.
