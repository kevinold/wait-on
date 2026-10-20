---
title: Repoint the jest-dev-server dependent at the suites that exercise wait-on - Plan
type: fix
date: 2026-10-20
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# Repoint the jest-dev-server dependent at the suites that exercise wait-on - Plan

## Goal Capsule

- Objective: `npm run dependents -- --only jest-dev-server --include-optional` runs jest-puppeteer suites that really call `waitOn` through jest-dev-server, three ways (baseline, `js`, `rust-strict`), on macOS and Linux, and the result is recorded.
- Means: the `jest-dev-server` entry in `xtask/assets/dependents.json` gets a `run` command that targets `packages/jest-environment-puppeteer` and `packages/expect-puppeteer`, an explicit Chrome install build step, and `os: ["linux", "darwin"]` (KTD1-KTD3); its xtask unit tests go red first (KTD6).
- Authority: the brief (user-directed settled decisions), `docs/solutions/best-practices/dependents-harness-npm-and-candidate-list.md`, `docs/guides/testing.md#dependents-harness`.
- Stop conditions: a public API, CLI, schema or `index.d.ts` change; a `lib/` or `crates/` edit; a `.github/workflows/` edit; a new dependency; a real regression found under `js` or `rust-strict` (report it in the PR and the recorded run, do not fix it here).
- Allowed paths: `xtask/assets/dependents.json`, `xtask/src/dependents.rs`, `docs/guides/testing.md`, `docs/solutions/`, `docs/plans/`, `CONCEPTS.md`.

## Product Contract

### Summary

The optional `jest-dev-server` dependent entry runs `node node_modules/jest/bin/jest.js --runInBand packages/jest-dev-server`, which reports "No tests found" because `packages/jest-dev-server` has no test files, so a green row proves nothing about wait-on. The entry is repointed at the jest-puppeteer suites whose global setup calls jest-dev-server `setup`, which calls `waitOn({ resources: ["tcp:0.0.0.0:4444"], timeout })` (jest-puppeteer v11.0.0, `packages/jest-dev-server/src/index.ts`, via `packages/jest-environment-puppeteer/src/global-init.ts`). The entry stays optional and never gates CI.

### Problem Frame

The harness's second entry was added as a placeholder ("not yet run" in `docs/guides/testing.md`) and its command exercises nothing. Validating the spike's packed package against a second real dependent needs a command that actually drives wait-on, a Chrome install that survives npm 11's `allowScripts` block on puppeteer's postinstall, and a platform list the maintainer can run on (macOS arm64 verified locally with wait-on 8.0.1 and 9.5.1: 17 suites pass, 3 skipped, 126 tests, about 50 s after about 1 m 40 s of install and build).

### Requirements

**Manifest entry**

- R1. The `jest-dev-server` entry's `run` is exactly one command, `npm test -- packages/jest-environment-puppeteer packages/expect-puppeteer`, run from the clone root through npm so `node_modules/.bin` is on `PATH`.
- R2. The entry stays `optional: true`: a plain `npm run dependents` never selects it; `--include-optional` or `--only jest-dev-server` does.
- R3. The entry's `os` is `["linux", "darwin"]`: `select` picks it on `linux` and `darwin` when included, never on `win32`, and `--only jest-dev-server` on `win32` errors naming the entry and its platforms.
- R4. The entry's `build` runs `node node_modules/puppeteer/install.mjs` then `npm run build`, in that order, before the baseline run.
- R5. Every `build` and `run` command of every manifest entry parses through `node_args`, and the `--` in R1's command reaches npm unchanged.

**Validation and docs**

- R6. The entry runs three ways on macOS with the packed tarball, and once on Linux in a container, with the verdict table, proof result, exit code, host, Node and npm versions and tarball version recorded in `docs/guides/testing.md`.
- R7. `docs/guides/testing.md` "Entries:" no longer says "linux only" or "not yet run", and `docs/solutions/best-practices/dependents-harness-npm-and-candidate-list.md` records what the entry runs, why `packages/jest-dev-server` alone is empty, and the Chrome install step.
- R8. No edit under `.github/workflows/`, `lib/`, `crates/`; no new dependency; the dependents harness stays on-demand.

### Key Decisions

- KD1. The entry stays optional and never gating (session-settled: user-directed — chosen over a required row: the Chrome download is about 150 MB plus OS libraries). Governs R2, R8.
- KD2. macOS joins Linux as a supported host (session-settled: user-directed — chosen over linux-only: the suites were verified on macOS arm64). Governs R3, R6.
- KD3. No workflow edits; the PR targets `kevinold/wait-on` `spike-next-rs` and is never merged by the worker (session-settled: user-directed — chosen over changing CI: dependents stay on-demand). Governs R8.

## Planning Contract

- KTD1. Run command: `npm test -- packages/jest-environment-puppeteer packages/expect-puppeteer` (session-settled: user-directed — chosen over `node node_modules/jest/bin/jest.js --runInBand packages/jest-dev-server`: that package has no tests, "No tests found"). `run_env` sets `npm_config_ignore_scripts=true`, which blocks `pretest`/`posttest` only, so `npm test` itself still runs.
- KTD2. Build: `["node node_modules/puppeteer/install.mjs", "npm run build"]` (session-settled: user-directed — chosen over relying on puppeteer's postinstall: npm 11 `allowScripts` blocks it even with `scripts: true`). `Entry.build` is already a list and `run_entry` already loops it, so no harness code changes.
- KTD3. `os: ["linux", "darwin"]` (session-settled: user-directed — chosen over `["linux"]`: verified on macOS arm64). `select` already matches any listed platform; the two tests that use `darwin` as the unsupported platform move to `win32`, the platform the entry must never run on (CI is ubuntu + windows).
- KTD4. `subdir` stays `packages/jest-dev-server`: npm workspaces hoist `wait-on` to the clone's root `node_modules`, where `resolve_check` expects `require.resolve('wait-on/package.json')` to land. If the macOS run fails at the resolve check, change `subdir` to `.` in U1 and record why.
- KTD5. Linux validation is a manual `linux/amd64` container run under OrbStack docker (mount the repo read-only, copy it in, build the linux-x64 prebuild inside, run), because the docker wrapper (kevinold/wait-on#114) is still open. Nothing in this PR automates it.
- KTD6. The xtask unit tests are the RED for the manifest; the three-way run is validation, not a test, since the harness is on-demand and needs network and Chrome.

## Implementation Units

### U1. Manifest entry and its xtask unit tests

- **Goal:** `xtask/assets/dependents.json` carries the new `jest-dev-server` entry and the xtask unit tests pin every changed field.
- **Requirements:** R1-R5; KTD1-KTD4, KTD6.
- **Dependencies:** none.
- **Files:** `xtask/src/dependents.rs` (test module only), `xtask/assets/dependents.json`.
- **Approach:**
  1. RED: rename `manifest_has_the_anchor_first_and_jest_dev_server_optional_on_linux` for linux and darwin and assert the new `os`, `build`, `run`, plus unchanged `subdir`, `swap`, `optional`, `scripts`; feed every entry's `build` and `run` through `node_args` and assert `Ok`.
  2. Update `optional_entries_run_only_when_included_and_on_their_os` and `only_on_an_unsupported_os_errors_instead_of_running_nothing` to use `win32` as the unsupported platform and `darwin` as a supported one.
  3. Extend `manifest_commands_run_through_node` with the R1 command's `--` passthrough.
  4. Confirm the failures are the field and selection assertions, then GREEN by editing the manifest entry only.
- **Execution note:** test-first; no non-test Rust change.
- **Test scenarios:**
  - Manifest entry fields equal R1, R3, R4 values; anchor entry unchanged.
  - `include_optional` on `darwin` and `linux` picks both entries; on `win32` only the anchor.
  - `--only jest-dev-server` on `win32` errors naming `jest-dev-server`, `linux` and `darwin`.
  - `node_args("npm test -- a b", npm)` yields `[npm, "test", "--", "a", "b"]`.
  - Every manifest `build`/`run` command parses.
- **Verification:** xtask tests red before the manifest edit, green after; `npm test` green.

### U2. Validation runs

- **Goal:** the entry passes three ways on macOS and once on Linux, with the rust-strict proof, and the results are captured for U3.
- **Requirements:** R6; KTD4, KTD5.
- **Dependencies:** U1.
- **Files:** none in the repo (raw logs stay in the scratchpad).
- **Approach:**
  1. macOS arm64: build the host prebuild (`npm run build:napi`, in this repo; distinct from the entry's own `npm run build` inside the clone), then run the harness with `--only jest-dev-server --include-optional`. Expect baseline on jest-puppeteer's pinned wait-on, the overrides swap, every `npm ls` copy at the tarball version, the resolve check passing, then `js` and `rust-strict` with the proof passing, exit 0.
  2. A cold run may fail with "Server has taken more than 4000ms to start" (jest-puppeteer `launchTimeout` 4000); rerun once and record both outcomes. A resolve-check failure fires KTD4's fallback.
  3. Linux: an OrbStack container run with `--platform linux/amd64` (Chrome for Testing ships no linux-arm64 build, so an arm64 container cannot launch Puppeteer's Chrome), with Node 22+, Rust, git and Chrome's shared libraries; copy the repo in from a read-only mount, build the linux-x64 prebuild inside the container, run the same command, and record the row as linux-x64.
  4. Record per host: platform-arch, Node, npm, tarball version, baseline/js/rust-strict marks, proof result, exit code. A regression row is a stop condition (KTD-level stop in the Goal Capsule).
- **Test expectation:** none -- validation of an on-demand harness, not repo behaviour; the unit tests are in U1.
- **Verification:** both hosts show the one command passing three ways and the rust-strict proof passing, exit 0. When the regression stop fires instead, the failing table is recorded (U3) and reported in the PR, and that recorded failure completes U2.

### U3. Docs

- **Goal:** the testing guide and solutions doc say what the entry runs, why, and what it produced.
- **Requirements:** R6, R7.
- **Dependencies:** U2.
- **Files:** `docs/guides/testing.md`, `docs/solutions/best-practices/dependents-harness-npm-and-candidate-list.md`, `CONCEPTS.md` (only if the "Dependents run" entry needs the optional/per-OS notion).
- **Approach:**
  1. Rewrite the `jest-dev-server` part of the guide's "Entries:" sentence (optional, linux and darwin, scripts, overrides swap, explicit Chrome install then build, the run command and why).
  2. Add a "Recorded run, 2026-10-20" paragraph per host in the shape of the 2026-10-02 one.
  3. Extend the solutions doc's candidate-list bullet: the suites that exercise wait-on and the "No tests found" trap, the `allowScripts` block and `install.mjs` step, the 4000 ms cold-start flake, and that jest-dev-server detects a timeout by the prefix "Timed out waiting for", a consumer-visible string both engines must keep.
- **Test expectation:** none -- docs only.
- **Verification:** command strings and versions in the guide match the manifest and U2 output.

## Verification Contract

| Check | Proves |
|---|---|
| `cargo test -p xtask` (red before manifest edit, green after) | U1 fields and selection |
| `npm test` | lint, types, mocha untouched |
| `npm run dependents -- --only jest-dev-server --include-optional` on macOS arm64 after `npm run build:napi` | R6 macOS, rust-strict proof |
| Same command once inside an OrbStack Linux container | R6 Linux |

A green table without the rust-strict proof is exit 1 by `verdict`, so the proof is what makes a run count.

## Definition of Done

- U1-U3 landed on `fix/dependents-jest-dev-server`; test and manifest change in the same commit; commit messages pass the commitlint hook.
- PR into `kevinold/wait-on` base `spike-next-rs`, CI green, not merged by the worker.
- No edits under `.github/workflows/`, `lib/`, `crates/`; no new dependency; `docs/` only added to.
- Recorded runs for macOS and Linux in `docs/guides/testing.md`; solutions doc updated in the same PR.
- Cleanup: no leftover kept temp dirs or containers, no raw logs committed, no `#[ignore]` or abandoned-attempt code in the diff.
