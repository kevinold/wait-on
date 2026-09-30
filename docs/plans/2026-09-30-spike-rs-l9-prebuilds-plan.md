---
title: "[L9] multi-target prebuilds, install matrix and read-only run"
type: build
date: 2026-09-30
topic: rust-port
lane: L9
kind: preview
branch: rs-61-prebuilds
closes: kevinold/wait-on#61
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
status: requirements-only
execution: code
---

# [L9] multi-target prebuilds, install matrix and read-only run

Requirements-only lane plan for sub-issue #61, authored from the issue text (no comments at
authoring time). `/lfg` deepens it into an implementation-ready plan before code. The spine
plan is controlling and is never edited by this lane.

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`, the `package` CI job produces one
`wait-on-*.tgz` holding every PO4 target's addon plus `SHA256SUMS`, having proven that the
tarball installs and loads with lifecycle scripts disabled under npm and pnpm and with
`--no-optional`, and that AE1 runs in a read-only container. Packed and unpacked sizes are
recorded. Pushes to `spike-next-rs` then publish a real fork prerelease (the tarball guard in
`rs-prerelease.yml` stops skipping). Proves PO4, PO5, PO7, PO8.

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions** (report to the PM, do not work around):
- a `.github/workflows/` change is needed (including a different `build:napi` argument shape
  in the `napi` matrix's `extra-args` column, or a runner / tool the `package` job lacks);
- a public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change is needed;
- a new **npm runtime** dependency is needed (the loader stays hand-written, KD-S4).

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`;
one PR, base `spike-next-rs` on `kevinold/wait-on`, body `Closes #61`; never merges.

## Product Contract

### Starting state (verified 2026-09-30)

- `scripts/build-napi.js` (L1) already maps all eight PO4 triples to
  `prebuilds/<platform>-<arch>[-musl]/wait-on.node`; the eight `napi` CI rows are green on
  `spike-next-rs`.
- `lib/engine.js` resolves the host prebuild dir (musl via `process.report`) and `require`s it.
- `package.json` `files` already includes `prebuilds/`; `.gitignore` ignores `prebuilds/`
  (`.npmignore` exists, so npm does not read `.gitignore`).
- `ci:rs:package` is undefined, so the `package` job and the prerelease are green no-ops.
- The `package` job runs on `ubuntu-latest` (linux-x64 glibc) after downloading every
  `prebuilds-<target>` artifact merged into `prebuilds/`, then uploads `wait-on-*.tgz` and
  `SHA256SUMS` from the repo root.

### Requirements

**Build (PO4)**

- R-L9-1 `npm run build:napi -- --target <triple>` produces
  `prebuilds/<platform>-<arch>[-musl]/wait-on.node` for each PO4 target: `darwin-arm64`,
  `darwin-x64`, `linux-x64`, `linux-arm64`, `linux-x64-musl`, `linux-arm64-musl`,
  `win32-x64`, `win32-arm64` (glibc linux dirs carry no suffix, matching `lib/engine.js`).
  The target list lives in one place and the loader's dir naming and the build's dir naming
  cannot drift. `linux-armv7` stays out (JS fallback covers it).
- R-L9-2 The built binary actually matches its dir: the `napi` artifact for each target lands
  at the expected path after the `package` job's merged download (the artifact root is
  `prebuilds/`, not flattened).

**Pack (PO5)**

- R-L9-3 `npm run ci:rs:package` runs `npm pack` and leaves exactly one `wait-on-*.tgz` and a
  `SHA256SUMS` covering it at the repo root (what `package` uploads and `rs-prerelease.yml`
  attaches).
- R-L9-4 Before packing it fails, naming the missing dirs, when any PO4 target's
  `wait-on.node` is absent, so CI can never ship a partial bundle. A local run with only the
  host prebuild needs an explicit, documented opt-in to proceed.
- R-L9-5 The tarball contains every PO4 `prebuilds/<dir>/wait-on.node` and no build
  intermediates (`target/`, `crates/`, `scripts/`, `Cargo.*`, `benchmarks/`, `docs/`, test
  fixtures); asserted from `npm pack` output, not assumed from `files`.
- R-L9-6 Size report: packed and unpacked tarball size, plus per-target addon size, printed by
  `ci:rs:package` and recorded in the guide with the baseline JS-only package size for
  comparison (PO5 decision input). No size threshold is enforced in this lane unless the
  plan's deepening finds a reason.

**Install and load (PO7, R6, R7, R9)**

- R-L9-7 From the packed tarball, into a fresh temp project, install with lifecycle scripts
  disabled and load the addon, under each cell:
  - npm with `--ignore-scripts`;
  - npm with `--ignore-scripts --no-optional` (or `--omit=optional`, whichever npm 10+/11
    honors);
  - pnpm with scripts disabled (pnpm 10 default, asserted explicitly).
  "Loads" means: with `WAIT_ON_ENGINE=rust-strict`, the installed `wait-on` resolves the
  host's prebuild from **inside the installed package** (not the repo checkout) and a
  resource check succeeds through the CLI (exit 0) and the API.
- R-L9-8 The tarball declares no `install` / `preinstall` / `postinstall` script and no
  optionalDependencies; asserted from the packed `package.json`.
- R-L9-9 No network fetch at install beyond the registry resolution of the existing runtime
  deps (`joi`, `rxjs`, `undici`); in particular nothing downloads a binary.

**Read-only container (PO8, AE1)**

- R-L9-10 AE1: an image built from the tarball runs `npm ci` (or `npm install <tgz>`) with
  `ignore-scripts=true` at build time; the container then runs with `--read-only` and
  `--network none` and `WAIT_ON_ENGINE=rust-strict`, executes `wait-on` against a
  `tcp:` resource served inside the same container (the AE1 stand-in for `tcp:db:5432`),
  and exits 0. A matching negative cell (no listener, short `--timeout`) exits non-zero with
  the timeout message, proving the check ran rather than short-circuiting.
- R-L9-11 The container is linux on the job's arch; a musl (alpine) image cell exercises the
  `-musl` prebuild and the loader's musl detection; a glibc (debian slim) cell exercises the
  glibc dir.
- R-L9-12 Where `docker` is absent (local macOS without Docker, Windows), the container cells
  skip with one clear line, never fail; under CI on the `package` job they must run (a skip
  there is a failure).

**Docs and hygiene**

- R-L9-13 `docs/guides/ci.md`: `ci:rs:package` status becomes defined; what it checks, the
  cells, the size table. `docs/guides/releasing.md`: the prerelease now carries a real
  multi-platform tarball, how testers install it, the size numbers. Keep edits small
  (sibling lanes edit these pages).
- R-L9-14 JS engine unchanged; `npm test` and `npm run ci:rs` stay green; `.npmignore` /
  `files` excludes any new tooling file from the published package.

### Tests that answer the risks (write each before its code)

- T-L9-1 Target table (mocha, `test/scripts.mocha.js`): every PO4 triple maps to the dir
  `prebuildDir` produces for that platform/arch/libc; the table has exactly the eight PO4
  targets; an unknown triple errors.
- T-L9-2 Missing-target guard (mocha): with a temp `prebuilds/` holding a subset,
  `ci:rs:package`'s check fails listing the missing dirs; with all eight (fixture files)
  it passes; the host-only opt-in passes with the host dir only.
- T-L9-3 Tarball contents (mocha, fixture prebuilds, real `npm pack --dry-run --json`):
  every PO4 `wait-on.node` present, no excluded paths (R-L9-5), no install scripts and no
  optionalDependencies (R-L9-8).
- T-L9-4 Size report shape (mocha): packed/unpacked bytes and per-target rows emitted for
  the fixture bundle.
- T-L9-5 Install matrix (runs in `ci:rs:package` against the real host addon; skip in the
  plain mocha run when the host prebuild is not built): npm `--ignore-scripts`, npm
  `--no-optional`, pnpm — each proves the addon loaded from the installed package path
  (path proof, e.g. `require.resolve` of the loaded file or a `rust-strict` run whose
  `WAIT_ON_NATIVE_LIBRARY_PATH` is unset and repo `prebuilds/` is out of reach).
- T-L9-6 AE1 read-only (runs in `ci:rs:package`; skip without docker, fail under CI):
  positive and negative cells on glibc and musl images (R-L9-10, R-L9-11).
- T-L9-7 Skip behaviour (mocha): with `docker` absent from `PATH`, the container step
  reports a skip and exits 0 locally, and exits non-zero when `CI` is set.

Matrix: package manager {npm, pnpm} × optional {default, `--no-optional`} (pnpm cell
default only unless deepening finds a reason) × scripts {disabled}; container libc
{glibc, musl} × outcome {ready, timeout}. Host arch only (linux-x64 in CI); other targets
are proven present and sized, not loaded (carve-out: the `package` runner cannot execute
darwin/windows/arm64 binaries; the `napi` rows already built them for their target).

### Scope Boundaries

- Allowed paths: `crates/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`,
  `lib/`, `bin/`, `test/`, `index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`,
  `.gitignore`, `.nycrc.json`, `eslint.config.mjs`, `docs/guides/`, `docs/plans/`,
  `docs/solutions/`, `AGENTS.md`, `README.md`, `benchmarks/`, `scripts/`. Never
  `.github/workflows/`; never the spine plan or sibling lane plans.
- Out of scope: resource checks (L2–L6), the Rust polling loop (L7), benchmarks (L8), npm
  publication and the end-to-end prerelease verification from the GitHub Release (L10),
  `linux-armv7`, code signing.

### Outstanding Questions (each names the test that answers it)

- Does `upload-artifact` with `path: prebuilds/**` keep `<dir>/wait-on.node` under the
  artifact root, so the merged download reconstructs `prebuilds/<dir>/`? Answered by
  T-L9-2's guard running on the real `package` job (it fails listing dirs if flattened). If
  it is flattened, stop: that is a workflow change.
- pnpm availability on `ubuntu-latest` + Node 24 (corepack vs a pinned `npx pnpm@10`): the
  deepened plan picks one; T-L9-5 fails if pnpm cannot run.
- Can a darwin-arm64 host reach the host-only opt-in path end to end (developer loop)?
  T-L9-2 host-only case.
- Musl container on an x64 runner needs the `linux-x64-musl` prebuild built by the `-x`
  (zig) row to load in `node:*-alpine`; T-L9-6 musl cell answers it.

### Sources

- Issue kevinold/wait-on#61 (text only; no comments); spine plan KD-S4, KD-S7, KD-S8,
  lane row L9 and the PO4 target matrix.
- Requirements plan `docs/plans/2026-09-28-1239-feat-rust-port-plan.md`: KD1 (prebuildify
  single package, hand-written loader), R6, R7, R9, AE1, PO4, PO5, PO7, PO8.
- `scripts/build-napi.js`, `scripts/ci-rs.js`, `lib/engine.js`, `package.json` `files`,
  `.npmignore`, `.github/workflows/node.js.yml` (`napi`, `package`, `prerelease` jobs),
  `.github/workflows/rs-prerelease.yml`, `docs/guides/ci.md`, `docs/guides/releasing.md`.

## Resume notes
