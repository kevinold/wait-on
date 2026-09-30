---
title: "[L1] Cargo workspace, napi addon, engine switch and dual-driver test run"
type: feat
date: 2026-09-30
topic: rust-port
lane: L1
kind: preview
branch: rs-53-scaffold
closes: kevinold/wait-on#53
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
---

# [L1] Cargo workspace, napi addon, engine switch and dual-driver test run

Requirements-only lane plan for sub-issue #53. Authored from the issue text (no issue
comments existed at authoring time) and the spine plan's KD-S1, KD-S2, KD-S4, KD-S5, KD-S7,
KD-S9. The spine plan is controlling and is never edited by this lane.

## Goal

Stand up the Rust side of the two-engine repo so later lanes (L2–L10) only port checks:
a Cargo workspace, a napi addon the JS package can load, an env-driven engine switch with
a safe fallback, and a mocha run that exercises every suite under both engines with an
explicit pending list. JS engine behavior is unchanged.

Proves: **PO1** (bridge shape — addon loads behind the unchanged `lib/wait-on.js` API),
**PO13** (dual driver — same mocha suites, two engines), **PO21** (toolchain pin).

## Scope boundaries

- Allowed paths: `crates/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`,
  `lib/`, `bin/`, `test/`, `index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`,
  `.gitignore`, `.nycrc.json`, `eslint.config.mjs`, `docs/guides/`, `docs/plans/`,
  `docs/solutions/`, `AGENTS.md`, `README.md`, `benchmarks/`, `scripts/`.
- New package allowed: `@napi-rs/cli` (devDependency only). No new runtime dependency.
- **No `.github/workflows/` edits.** If a workflow change turns out to be required, stop and
  report it to the PM (do not post on #35, do not edit workflows).
- No resource check is ported in this lane. Under the Rust engine every resource still uses
  the JS checks; the addon only has to load and answer `version()`.
- Public API, CLI flags, `WAIT_ON_SCHEMA`, and `index.d.ts` surface do not change.

## Requirements

- **R-L1-1 Cargo workspace (KD-S4).** Root `Cargo.toml` workspace with members
  `crates/wait-on-core` (pure Rust lib, no napi) and `crates/wait-on-napi` (napi-rs binding,
  `crate-type = ["cdylib"]`, depends on `wait-on-core`). `Cargo.lock` committed.
- **R-L1-2 Toolchain (KD-S5, PO21).** `rust-toolchain.toml` pins a specific stable version
  with `rustfmt` and `clippy` components; MSRV (`rust-version` in the workspace) equals the
  pin. The CI `rust` job runs a bare `rustup toolchain install`, so the file alone must be
  enough to provision the toolchain.
- **R-L1-3 Supply-chain gate.** `deny.toml` so `cargo deny check` passes on the committed
  lockfile (licenses allow-list compatible with MIT distribution, advisories, bans, sources
  restricted to crates.io).
- **R-L1-4 Addon surface.** The addon exports at least `version()` returning the crate
  version string (plus the no-op named in the spine's L1 row, if cheap). No other surface.
- **R-L1-5 Prebuilds loader (KD-S4).** Small hand-written loader in `lib/` that resolves
  `prebuilds/<platform>-<arch>[-musl]/wait-on.node` relative to the package root (musl
  detected at runtime on linux), with no new runtime dependency. `prebuilds/` is
  gitignored and not committed.
- **R-L1-6 Engine selection (KD-S1).** Read `WAIT_ON_ENGINE`:
  - unset or `js` → JS engine; the addon is never loaded.
  - `rust` → load the addon; on load failure fall back to JS silently (no stdout/stderr
    change for the CLI).
  - `rust-strict` → load the addon; on load failure `waitOn` rejects / calls back with an
    error naming the engine and the failure (the CLI exits non-zero with that message).
  - any other value → a clear error rather than a silent default.
  Under a loaded Rust engine, resources not yet ported keep using the JS checks.
- **R-L1-7 npm scripts (KD-S7).**
  - `build:napi` — build the host (or `-- --target <triple> [extra-args]`) addon into
    `prebuilds/<platform>-<arch>[-musl]/wait-on.node` via `@napi-rs/cli`. It must accept the
    CI `napi` job's argument shape (`--target <triple>`, and `-x` on musl rows).
  - `ci:rs` — cross-platform (ubuntu, macos, windows; no POSIX-only shell): `cargo fmt
    --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`,
    host `build:napi`, then the mocha suites with `WAIT_ON_ENGINE=rust-strict`. Env setting
    must work under Windows `cmd` (use a node script in `scripts/` or equivalent, not
    `VAR=x cmd`).
- **R-L1-8 Pending list (KD-S2).** One explicit Rust pending list (e.g.
  `test/rust-pending.js`) naming tests that cannot pass on Rust yet. Under
  `WAIT_ON_ENGINE=rust*`, listed tests are skipped (reported as pending, not silently
  dropped); under JS the list has no effect. The list is expected to start empty or near
  empty because every check still runs in JS. A listed test that is not found in the
  suites is an error, so stale entries cannot accumulate.
- **R-L1-9 Dual-driver run (PO13).** `npm test` (JS) is unchanged in outcome. `ci:rs` runs
  the same `test/**/*.mocha.js` under `rust-strict`, including CLI subprocess tests
  (the env var propagates to spawned `bin/wait-on`).
- **R-L1-10 Packaging hygiene.** `.npmignore` / `files` keep `crates/`, `Cargo.*`,
  `rust-toolchain.toml`, `deny.toml`, `target/`, `scripts/` out of the published package;
  `prebuilds/` is included when present (L9 fills it). `target/` gitignored.
- **R-L1-11 Lint/coverage.** New `lib/` and `scripts/` code is linted; `npm run
  test:coverage` thresholds (`.nycrc.json`) stay met under the JS run.
- **R-L1-12 Guides (KD-S9).** Update `docs/guides/` `architecture.md`, `development.md`,
  `testing.md`, `ci.md` with the real layout, commands, engine switch, pending list, and the
  `ci:rs` / `build:napi` status (no longer "planned").

## Test-first map (every risk names its test)

Tests live at the front doors (AGENTS.md TDD). Engine-switch tests drive `waitOn` and the
CLI with `WAIT_ON_ENGINE` set in a child process or scoped env, not by mocking lib modules.

| # | Behavior / risk | Test (RED first) |
|---|---|---|
| T1 | `WAIT_ON_ENGINE` unset and `js` → JS path, addon never loaded | API test: resolves on an existing file; assert addon not loaded (e.g. an exported engine-introspection value or `require.cache` has no `.node`) |
| T2 | `rust` + addon missing → falls back to JS and succeeds | API + CLI test with the loader pointed at an empty prebuilds dir; resolves / exit 0, stderr unchanged |
| T3 | `rust-strict` + addon missing → error | API test rejects with an engine-load error; CLI exits non-zero and stderr names the failure |
| T4 | `rust` / `rust-strict` + addon present → addon loaded, `version()` matches crate version | test runs only when a host prebuild exists (skip, not fail, otherwise); asserts path taken |
| T5 | invalid `WAIT_ON_ENGINE` value → clear error | API + CLI test |
| T6 | loader path resolution per platform/arch/musl | unit-level test of the loader's path function across the dispatch cells (darwin, linux gnu, linux musl, win32) |
| T7 | pending list: listed test skipped under rust, runs under js; unknown entry errors | mocha test of the pending-list hook |
| T8 | `ci:rs` runs mocha with `rust-strict` cross-platform | command observed failing before the script exists, then green locally (`npm run ci:rs`) |
| T9 | Rust side | `cargo test` unit test for `version()` in `wait-on-napi` (or core) |
| T10 | publish contents | `npm pack --dry-run` before/after: no `crates/`, `target/`, `Cargo.*` |

Matrix (AGENTS.md): engine value {unset, js, rust, rust-strict, invalid} × addon {present,
missing} × front door {API, CLI}. Carve-out: `unset`/`js` × present/missing collapse to one
cell each because the addon is never touched on those values (T1 proves it).

## Risks / open questions

- **napi CI job now builds 8 targets.** Defining `build:napi` activates the existing `napi`
  job for every row, including musl-via-zig (`-x`) and `aarch64-pc-windows-msvc`. PR checks
  must be green, so `build:napi` must handle every row or the PR is blocked. Answered by
  the PR's `napi` checks; if a row needs a workflow change → stop and report to the PM.
- **Windows `ci:rs`.** Env var and path handling must not assume POSIX shell. Answered by T8
  on the `rust` job's windows row.
- **Addon version vs package version.** Crate version need not equal `package.json`
  version; T4 asserts crate version only.
- **Coverage thresholds** with a loader whose addon-present branch cannot run in the JS
  `build` job: may need a reasoned `.nycrc.json` exclusion or a test that runs both branches
  with a fixture `.node`-less loader. Answered by `npm run test:coverage` meeting `.nycrc.json` thresholds.

## Done when

- `npm test` green (JS engine, behavior unchanged).
- `npm run ci:rs` green locally and in the `rust` CI job (ubuntu, macos, windows).
- Loader and fallback covered by tests T1–T7.
- Every PR check green; PR base `spike-next-rs`, body contains `Closes #53`. Never merged by
  the lane.

## Resume notes

<!-- Lane L1 notes only. Append below; never edit the spine plan or sibling lane plans. -->
