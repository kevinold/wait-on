---
title: "[L12] cargo xtask replaces the JavaScript build and CI scripts - Plan"
type: build
date: 2026-10-01
deepened: 2026-10-01
topic: rust-port
lane: L12
kind: preview
branch: rs-75-xtask
closes: kevinold/wait-on#75
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L12] cargo xtask replaces the JavaScript build and CI scripts - Plan

Deepened lane plan for sub-issue #75 (base `43399ce`, verified 2026-10-01 against `scripts/*.js`, `test/scripts.mocha.js`, `package.json`, `Cargo.toml`, `Cargo.lock`, `deny.toml`, `supply-chain/`, `.github/workflows/node.js.yml`, `docs/guides/*.md`, `mise.toml`). The spine plan is controlling and is never edited by this lane.

Product Contract preservation: R-L12-1..R-L12-16 and T-L12-1..T-L12-4 keep their IDs and meaning; R-L12-7 and R-L12-12 are reworded only for the operator steer of 2026-10-01 (a root `Justfile` is the Rust front door, so xtask loses its `ci` subcommand and the npm aliases call `just`); new requirements take R-L12-17 onward. OQ1-OQ4 are resolved into KTD1-KTD4 below.

---

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`, `npm run ci:rs`, `npm run build:napi`, `npm run ci:rs:package` and `npm run bench:startup` run through a root `Justfile` (`just --list` is the index) whose one-liners call cargo directly and whose three large tasks (`build-napi`, `package`, `bench-startup`) call a Rust `cargo xtask` crate, with the same observable behavior as today's Node scripts, and the ported JavaScript is gone.

**Means.** KD2 Justfile front door, KD1 xtask for logic too big for a recipe, KTD1 probe carried as the one surviving JS asset, KTD2 serde_json + ring, KTD7 Justfile mechanics proven on just 1.40.0.

**Direction (operator, 2026-10-01).** Rust is the primary implementation; the JavaScript engine and JS tooling are what get sunset. Avoidable JavaScript moves to Rust. One discoverable Rust front door (`just`).

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions** (report to the PM, do not work around):
- a `.github/workflows/` change is needed (CI must keep calling the same `npm run` names with the same argument shapes, including the `napi` job's `build:napi -- --target <triple> [extra-args]`); the `just` install in CI is an operator PR, not this lane (D1);
- a public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change is needed;
- a new **npm runtime** dependency is needed.

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`; one PR, base `spike-next-rs` on `kevinold/wait-on`, body `Closes #75`; never merges. Allowed paths: `Justfile`, `mise.toml`, `xtask/`, `.cargo/`, `Cargo.toml`, `Cargo.lock`, `crates/`, `supply-chain/`, `deny.toml`, `scripts/`, `benchmarks/`, `test/`, `package.json`, `package-lock.json`, `eslint.config.mjs`, `.npmignore`, `.gitignore`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `AGENTS.md`.

---

## Product Contract

### Summary

Add a root `Justfile` with recipes `fmt`, `lint`, `test`, `deny`, `vet`, `cov`, `build-napi`, `package`, `bench-startup`, `mocha-rust` and `ci`, and an `xtask/` workspace crate (alias `xtask = "run --package xtask --"` in `.cargo/config.toml`) with subcommands `build-napi [--target <triple>] [napi args]`, `package [--host-only]` and `bench-startup [--runs N] [--record]` ported from `scripts/build-napi.js`, `scripts/ci-rs-package.js` + `scripts/prebuild-probe.js` and `scripts/bench-startup.js`. `scripts/ci-rs.js` becomes the `ci` recipe. Rust unit tests carry every case `test/scripts.mocha.js` covers today. `package.json` scripts become one-line `just ...` aliases. The ported JS and `test/scripts.mocha.js` are deleted; the probe survives as `xtask/assets/prebuild-probe.js` because it must exercise the installed JS package under Node (KTD1).

### Problem Frame

Four Node scripts (555 lines) plus a 423-line mocha file implement the Rust gate, addon build, packaging checks and startup benchmark. They are JS tooling in a lane whose direction is "Rust primary, JS sunset", and each developer discovers them by reading `package.json`. The operator wants one discoverable Rust front door (`just --list`) with cargo one-liners inline and only the three real programs in Rust, and CI must keep calling the same `npm run` names because lanes never edit workflows (KD-S7).

### Starting state (verified 2026-10-01, base `43399ce`)

- `scripts/`: `build-napi.js` (81), `ci-rs.js` (38), `ci-rs-package.js` (276), `bench-startup.js` (100), `prebuild-probe.js` (61), plus `reindex-codebase-memory.sh` (agent tooling, stays). `test/scripts.mocha.js` (423) tests their pure helpers and spawns the probe and the benchmark.
- `package.json`: `build:napi` = `node scripts/build-napi.js`; `ci:rs` = `cargo vet --locked && node scripts/ci-rs.js`; `ci:rs:package` = `node scripts/ci-rs-package.js`; `bench:startup` = `node scripts/bench-startup.js`; `lint` globs include `scripts/**/*.js`; `files` allow-list is `bin/ lib/ prebuilds/ exampleConfig.js index.d.ts` (so `xtask/` and `.cargo/` are already unshipped by construction).
- `.github/workflows/node.js.yml`: `rust` (ubuntu/macos/windows: `rustup toolchain install`, rust-cache, cargo-deny + cargo-vet, `npm ci`, `npm run --if-present ci:rs`), `napi` (8 rows incl. `ubuntu-24.04-arm` and `windows-11-arm`, `build:napi -- --target <t> <extra>`), `package` (ubuntu-latest, no rustup step, no rust cache, `ci:rs:package`). No step installs `just`.
- Cargo workspace: `crates/wait-on-core`, `crates/wait-on-napi`; edition 2024, rust-version 1.98.1 (= `rust-toolchain.toml`), `publish = false`. `Cargo.lock` holds `serde`, `serde_core`, `serde_derive`, `itoa`, `memchr`, `cfg-if`, `libc`, `ring`, `walkdir`, `windows-sys`; it does not hold `serde_json`, `zmij`, `indexmap`, `sha2`, `tempfile`, `anyhow`, `clap`. `deny.toml` allows MIT/Apache-2.0/ISC/Unicode-3.0/BSD-3-Clause/CDLA-Permissive-2.0. `supply-chain/config.toml` has 7 imports and 130 hand-noted exemptions (`cargo vet regenerate exemptions` is forbidden: `docs/solutions/best-practices/cargo-vet-store-setup-and-maintenance.md`).
- `benchmarks/startup-baseline.json`: `threshold {relative 0.25, floorMs 50}`, `runs 20`, one `recorded` key `darwin-arm64`; written as `JSON.stringify(x, null, 2) + "\n"`, `floorMs` is an integer literal.
- `mise.toml` has a `[tools]` table (codebase-memory-mcp only). `just` 1.40.0 is on the dev host; nothing pins it.
- `lib/engine.js` `prebuildDir({platform, arch, musl})` = `${platform}-${arch}[-musl]`; `isMusl` reads the process report.

### Requirements

**Front door (Justfile)**

- R-L12-17 A root `Justfile` defines `fmt` (`cargo fmt --all --check`), `lint` (`cargo clippy --workspace --all-targets -- -D warnings`), `test` (`cargo test --workspace`), `deny` (`cargo deny check`), `vet` (`cargo vet --locked`), `cov` (`cargo llvm-cov --workspace`, consumed by L13; this lane does not install cargo-llvm-cov), `build-napi *args`, `package *args`, `bench-startup *args` (each forwarding to the xtask subcommand of the same name), `mocha-rust` (the whole mocha suite under `WAIT_ON_ENGINE=rust-strict`) and `ci`. Every recipe carries a doc comment so `just --list` is the index.
- R-L12-7 `ci` is the full Rust gate as a recipe dependency chain, in order, stopping at the first failure: `vet`, `fmt`, `lint`, `test` (now including xtask's tests), `deny`, host `build-napi`, `mocha-rust`, `bench-startup` last (a benchmark failure never masks a test failure). just echoes each recipe line before running it.
- R-L12-18 Recipes are shell-neutral: one command per line, no `&&`, pipes, quoting tricks or env-assignment prefixes; `set windows-shell := ["cmd.exe", "/c"]` so Windows needs no `sh`; `mocha-rust` sets `WAIT_ON_ENGINE` through an exported recipe parameter (`$WAIT_ON_ENGINE="rust-strict"`) and loads `test/helpers/assert-rust-strict.js` with mocha `--require`, which throws unless `WAIT_ON_ENGINE` is `rust-strict`, so a lost env var fails loudly on every OS instead of silently running the JS engine.
- R-L12-19 `mise.toml` pins `just` for developers (`[tools] just = "1.40.0"`); `docs/guides/development.md` lists it as a prerequisite for Rust work (`npm test` still needs none of it).
- R-L12-12 `package.json` `build:napi`, `ci:rs`, `ci:rs:package`, `bench:startup` are one-line `just build-napi`, `just ci`, `just package`, `just bench-startup` aliases; `npm run <name> -- <args>` reaches the recipe's variadic parameter unchanged (CI's `build:napi -- --target <triple> -x` shape included). `cargo vet --locked` moves from the `package.json` entry into the `vet` recipe and stays first in `ci`.

**Workspace wiring**

- R-L12-1 `xtask/` is a workspace member (`publish = false`, workspace edition/rust-version/license) and `.cargo/config.toml` defines `xtask = "run --package xtask --"`, so `cargo xtask <cmd>` works from the repo root on Linux, macOS and Windows.
- R-L12-2 xtask runs every child process without a shell (behaves the same under Windows cmd) and propagates a failing child's exit code (non-zero, never 0; a signal-killed child exits 1). An unknown or missing subcommand prints usage and exits non-zero.
- R-L12-3 Any third-party crate xtask adds passes `cargo deny check` and `cargo vet --locked` (imports refreshed or exemptions hand-written with `notes` in `supply-chain/`). Prefer std and crates already in `Cargo.lock`.
- R-L12-4 xtask is never shipped: `npm pack` still excludes it (the `package` not-shipped guard extends to `xtask/` and `.cargo/`).

**`build-napi` (replaces `scripts/build-napi.js`)**

- R-L12-5 `cargo xtask build-napi [--target <triple>] [extra napi args...]` keeps the JS contract: no `--target` means the rustc host (`rustc -vV` `host:` line); the eight PO4 triples map to `darwin-arm64`, `darwin-x64`, `linux-x64`, `linux-arm64`, `linux-x64-musl`, `linux-arm64-musl`, `win32-x64`, `win32-arm64`; an unknown triple fails naming the supported list; it runs `napi build --release --manifest-path crates/wait-on-napi/Cargo.toml --output-dir target/napi/<dir> --target <triple> <extra>` and copies the single built `.node` to `prebuilds/<dir>/wait-on.node`, failing when the output dir holds zero or several `.node` files.
- R-L12-6 The Rust dir naming cannot silently drift from `lib/engine.js` `prebuildDir`: a test fails if the two disagree for any PO4 target.

**`package` (replaces `scripts/ci-rs-package.js` + `scripts/prebuild-probe.js`)**

- R-L12-8 `cargo xtask package [--host-only]` keeps every L9 behavior: refuse a partial bundle naming the missing `prebuilds/<dir>/wait-on.node` (all eight, or only the host's with `--host-only`); remove stale `wait-on-*.tgz`; `npm pack --json` once; fail on unpacked prebuilds, shipped intermediates (`target/ crates/ scripts/ docs/ test/ benchmarks/ xtask/ .cargo/ Cargo.*`), declared lifecycle scripts (`preinstall install postinstall prepare`) or `optionalDependencies`; print the size report; write `SHA256SUMS` (`<hex>  <filename>\n`, `sha256sum -c` compatible); run the npm / npm `--omit=optional` / pnpm@10.34.6 install cells with scripts off and `WAIT_ON_NATIVE_LIBRARY_PATH` scrubbed, asserting the host addon loads from inside the installed package; run the AE1 glibc (`node:24-trixie-slim`) + musl (`node:24-alpine`) read-only, no-network container cells, skipped without docker locally, failing without docker (or without the needed linux prebuild) when `CI` is set.
- R-L12-9 npm is still reached as `node $npm_execpath` (fail with a "run this through npm" message when unset), so `npm run ci:rs:package` behaves as before.
- R-L12-10 The probe that runs inside install cells and containers keeps its JSON line contract (`addonPath, realpath, pkgDir, api, cli, cliError`) and its verdict rules (ready: `api` true and CLI exit 0 from the expected dir; timeout: `api` and CLI each time out on their own). It exercises the *installed JS package's* API and CLI under Node, carried per KTD1.

**`bench-startup` (replaces `scripts/bench-startup.js`)**

- R-L12-11 `cargo xtask bench-startup [--runs N] [--record]` keeps the method: local tcp listener on `127.0.0.1:0` that accepts and closes; one untimed warm-up per engine; N interleaved spawns of `node bin/wait-on tcp:127.0.0.1:<port> -t 10000` under `WAIT_ON_ENGINE=js` and `rust-strict` (30 s each); medians; verdict `overhead <= max(relative * jsMs, floorMs)` from `benchmarks/startup-baseline.json`; the same `bench:startup (<runs> runs per engine) js median X ms, rust median Y ms, overhead Z ms, allowed W ms: ok|FAIL` line (one decimal); exit non-zero on FAIL or on a failed run (`WAIT_ON_ENGINE=<engine> run failed (exit <code>): <stderr>`); `--runs` must be a positive integer; `--record` rewrites only this host's `recorded["<platform>-<arch>"]` entry (Node naming, e.g. `darwin-arm64`, `win32-x64`) with values rounded to 0.1 and a `YYYY-MM-DD` date, preserving the rest of the file byte for byte, and logs `recorded <key> in benchmarks/startup-baseline.json`.

**Scripts, tests and lint**

- R-L12-13 `scripts/` holds no ported JS (`reindex-codebase-memory.sh` stays); `test/scripts.mocha.js` is deleted; `lint` no longer globs `scripts/**/*.js` and gains `xtask/**/*.js`; repo JS line count drops by at least the deleted files minus the 61-line probe.
- R-L12-14 `benchmarks/http-ffi.js` stays JavaScript: it measures the JS `waitOn` front door per engine (the issue's carve-out), with `test/benchmarks.mocha.js` unchanged.
- R-L12-15 JS engine behavior unchanged; `npm test` and the rust-strict mocha run stay green; `npm test` needs neither `just` nor a Rust toolchain (just-dependent mocha tests skip when `just` is absent).

**Docs**

- R-L12-16 `docs/guides/development.md`, `ci.md` and `testing.md` describe just + xtask (recipes, subcommands, where tests live, `cargo test -p xtask`), and no guide or `AGENTS.md` line still points at a deleted script.

### Key Decisions

- KD1 **Tooling in a Rust `xtask/` workspace crate** with subcommands `build-napi`, `package`, `bench-startup`, aliased `xtask = "run --package xtask --"` in `.cargo/config.toml` (session-settled: user-directed — chosen over keeping Node scripts in `scripts/`: Rust primary; JS tooling sunset). The 2026-10-01 steer removes the `ci` subcommand: `ci` is a recipe (KD2). Governs R-L12-1, R-L12-2, R-L12-5, R-L12-8, R-L12-11.
- KD2 **A root `Justfile` is the front door for every Rust task**; one-liners call cargo directly, only logic too big for a recipe lives in xtask and is called from a recipe (session-settled: user-directed — chosen over `package.json` calling `cargo xtask` directly: operator wants one discoverable Rust front door). Governs R-L12-17, R-L12-7, R-L12-18, R-L12-19, R-L12-12.
- KD3 **`package.json` scripts stay as one-line aliases (`just ...`) and no `.github/workflows` edit** (session-settled: user-directed — chosen over editing workflows: lanes never edit workflows, KD-S7). The CI `just` install is an operator PR (D1). Governs R-L12-12, R-L12-15.
- KD4 **`cargo vet --locked` stays in the CI gate**, first in `ci` (session-settled: user-directed — chosen over dropping vet: L11 supply-chain gate). Governs R-L12-3, R-L12-7.
- KD5 **Delete the ported JS and `test/scripts.mocha.js`; drop `scripts/**` from eslint** (session-settled: user-directed — chosen over JS shims: JS line count must drop). Conflict call-out: one JS file survives outside `scripts/` (`xtask/assets/prebuild-probe.js`, KTD1) with three mocha tests, because a Rust probe cannot exercise the installed JS API inside `node:24` containers; the letter of R-L12-13 holds (`scripts/` is JS-free, the mocha file is gone). Governs R-L12-13.
- KD6 **`benchmarks/http-ffi.js` stays JavaScript** (session-settled: user-directed — chosen over porting wholesale: it measures the JS front door). Governs R-L12-14.

### Scope Boundaries

- In: `Justfile`, `mise.toml` pin, `xtask/` crate (three subcommands, probe asset, tests), `.cargo/config.toml`, workspace membership, `Cargo.lock` + `supply-chain/` for `serde_json` (with `preserve_order`), `zmij`, `indexmap` and their deps, `package.json` aliases and lint globs, deletion of four scripts and `test/scripts.mocha.js`, new `test/justfile.mocha.js`, `test/prebuild-probe.mocha.js`, `test/helpers/assert-rust-strict.js`, one-line additions to `test/rust-scaffold.mocha.js`, guides and `AGENTS.md`.
- Out: `.github/workflows/` edits (incl. installing `just` or a toolchain in the `package` job); release automation; `benchmarks/http-ffi.js` port; `lib/` or `bin/` changes; `scripts/reindex-codebase-memory.sh`; installing `cargo-llvm-cov` or wiring coverage thresholds (L13).
- Non-goals considered and not built: a `fmt-fix` recipe (`cargo fmt --all` is one command); `.npmignore` entries for `xtask/`/`.cargo/` (the `files` allow-list is the single owner, proven by the not-shipped test); a clap/arg-parsing crate (three subcommands, five flags); a `tempfile` crate (std `temp_dir` + pid + counter); a Rust probe binary cross-built per container libc (needs a toolchain the `package` job lacks and cannot call the installed JS API).

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Probe carriage (resolves OQ1).** `scripts/prebuild-probe.js` moves unchanged to `xtask/assets/prebuild-probe.js`; `package` references it by path from the crate root (`CARGO_MANIFEST_DIR`) for install cells and copies it into each docker build context, as today. Its three subprocess tests move to `test/prebuild-probe.mocha.js`, keeping the junction-linked temp project and `test/fixtures/fake-addon-checks.js`. `lint` gains `xtask/**/*.js`. Rejected: `include_str!` written to disk at run time (an extra write per cell for no gain).
- KTD2. **JSON and SHA-256 crates (resolves OQ2).** `serde_json` with the `preserve_order` feature (pulls `zmij` for float formatting and `indexmap`; `serde`, `itoa`, `memchr` are already locked) for `npm pack --json`, `@napi-rs/cli/package.json` and the baseline. SHA-256 through `ring::digest` (`ring` is already in `Cargo.lock`, vetted, and already compiles for the host on every runner that compiles xtask: the `rust` job builds it on ubuntu/macos/windows, and each `napi` row including `windows-11-arm` builds the addon for its own host triple). Rejected: `sha2` (several new crates to vet for one digest; fallback if a row's host build of ring fails). The baseline is a typed struct (`threshold {relative, floorMs}`, `runs`, `recorded` as an insertion-ordered `serde_json::Map<String, Sample>`) whose numeric fields are `serde_json::Number`, written with `to_string_pretty` + `\n`, which matches `JSON.stringify(x, null, 2)`; a rounded sample with no fraction is written as an integer `Number` so `100` never becomes `100.0`. `preserve_order` keeps existing `recorded` keys in file order and appends a new host last, as JS `{...recorded, [key]: sample}` does, so R-L12-11's byte-for-byte rule holds for multi-host files too.
- KTD3. **napi CLI source (resolves OQ3).** `build-napi` reads `node_modules/@napi-rs/cli/package.json` `bin.napi` and spawns `<node> <that path> <napi args>`; `<node>` is `npm_node_execpath` when set (parity with `process.execPath` under npm), else `node` from `PATH`. The same resolver serves Node spawns in `package` and `bench-startup`.
- KTD4. **Host naming (resolves OQ4).** `std::env::consts::OS`/`ARCH` map to Node names (`macos`→`darwin`, `windows`→`win32`, `x86_64`→`x64`, `aarch64`→`arm64`, linux passes through); musl from `cfg!(target_env = "musl")`; `prebuild_dir(platform, arch, musl)` reproduces `lib/engine.js`. Drift check (T-L12-2): a Rust test spawns `node -e` that requires `lib/engine.js` and prints `prebuildDir` for all eight PO4 targets, compared with the Rust map.
- KTD5. **Process model.** `std::process::Command` only (no shell); a helper runs a child with inherited stdio and returns its exit code (`None` → 1); the dispatcher exits with it. Repo root is `CARGO_MANIFEST_DIR`'s parent. Temp dirs are `std::env::temp_dir()/wait-on-<cell>-<pid>-<n>`. Binary-level tests in `xtask/tests/cli.rs` run the built binary via `env!("CARGO_BIN_EXE_xtask")`, never a nested `cargo` (which blocks on the build-directory lock under `cargo test`).
- KTD6. **Argument parsing.** Hand-rolled over `std::env::args`: `build-napi` strips `--target <t>` and forwards the rest verbatim (today's `parseArgs`); `package` recognises `--host-only`; `bench-startup` recognises `--runs <N>` and `--record`. Unknown subcommand → usage on stderr, exit 2.
- KTD7. **Justfile mechanics (verified on just 1.40.0).** Recipe args beginning with `-` reach a `*args` variadic without `--`; `$WAIT_ON_ENGINE="rust-strict"` as a `mocha-rust` parameter exports the env var without shell syntax; `just -n <recipe>` prints the composed command lines in dependency order and is the test oracle for recipe order and arg forwarding; `set windows-shell := ["cmd.exe", "/c"]` removes the `sh` dependency on Windows. `mocha-rust` runs `node node_modules/mocha/bin/mocha.js --require test/helpers/assert-rust-strict.js --exit "test/**/*.mocha.js"`; the require guard is the proof that the env reached mocha on every OS.
- KTD8. **Date stamp.** `--record` derives `YYYY-MM-DD` from `SystemTime` epoch seconds through a pure civil-from-days function tested with fixed vectors (no clock crate; tests never read the real clock).
- KTD9. **`ci` composition.** `ci` is a dependency list (`vet fmt lint test deny build-napi mocha-rust bench-startup`); `build-napi` with no args builds the host. There is no step list in Rust; `just -n ci` is the order test.

### Assumptions

- `rustup` on `ubuntu-latest` auto-installs the `rust-toolchain.toml` channel on the first `cargo` call in the `package` job (rustup ≥ 1.28.1 default); if it does not, that is a stop condition (R1).
- GitHub's `windows-latest`, `windows-11-arm`, `macos-14`, `ubuntu-24.04(-arm)` runners can compile xtask's host build (ring's C sources already compile there today).
- The `rust`, `napi` and `package` jobs run `npm ci` before the hook, so `node_modules/@napi-rs/cli` and `node_modules/mocha` exist and `npm_execpath`/`npm_node_execpath` are inherited through `just` → `cargo` → xtask.
- mozilla/google imports may cover `serde_json` and `indexmap`; `zmij` (MIT, newer) likely needs a hand-written exemption with `notes`. Any uncovered crate gets one.

### Risks & Dependencies

- D1 **`just` is not installed in CI** until a separate operator PR adds it to the `rust`, `napi` and `package` jobs. Until that PR is in `spike-next-rs`, this PR's `rust`, `napi` and `package` jobs fail at `just: command not found`; merge `origin/spike-next-rs` into the branch when it lands. The operator PR is a merge prerequisite for L12 (stated in the PR body) and should install just 1.40.0, the version KTD7 verified and `mise.toml` pins. Check: T-L12-4 (all three jobs green on the PR after the merge). The `build` job (`npm test`) never needs `just`.
- R1 **`package` job relies on rustup auto-install of the pinned toolchain** (no install step, no cache): the first `cargo xtask package` call must install 1.98.1 and cold-compile xtask (serde_json, ring). Check: T-L12-4 `package` job green; a `toolchain ... is not installed` failure is a stop condition (suggest folding the install step into D1's operator PR).
- R2 **xtask compiles on every napi row** (host build incl. `windows-11-arm`, musl rows on hosts with zig): deps are serde_json (pure Rust) and ring (already built for those hosts). Check: all eight `napi` rows green (T-L12-4); fallback is `sha2` (KTD2).
- R3 **cargo vet / deny for new crates**: `serde_json`, `zmij`, `indexmap` (+ deps) must be covered; refresh `imports.lock` with `cargo vet` (never `regenerate exemptions`), write any exemption by hand with `notes`, and keep the store format-stable under cargo-vet 0.10.0 and 0.10.2 and LF. Check: `cargo vet --locked` and `cargo deny check` green locally and in `ci`.
- R4 **Baseline JSON byte-identical round-trip** (`floorMs` integer, 2-space pretty print, trailing newline, key order). Check: U3 scenario "re-serialising the committed baseline is byte-identical".
- R5 **Windows shell and env**: recipes under `cmd.exe /c`, `WAIT_ON_ENGINE` export. Check: `test/helpers/assert-rust-strict.js` fails `mocha-rust` loudly if the env is lost; the windows `rust` job and two windows `napi` rows green (T-L12-4).
- R6 **`cargo test --workspace` now spawns node and the xtask binary**: tests needing `npm_execpath` skip without it; no test spawns `cargo` (KTD5). Check: `cargo test -p xtask` green both standalone and under `just ci`.
- R7 **Probe path on Windows** (`xtask/assets/...` under `CARGO_MANIFEST_DIR`, junction symlinks in tests). Check: `test/prebuild-probe.mocha.js` on the windows `build` job.

---

## Implementation Units

### U1. xtask crate scaffold, alias and dispatcher

**Goal.** `cargo xtask <cmd>` runs from the repo root, dispatches three subcommands, and propagates child exit codes.

**Requirements.** R-L12-1, R-L12-2, R-L12-3 (no new crates yet); KD1.

**Dependencies.** None.

**Files.** `Cargo.toml` (members), `Cargo.lock`, `.cargo/config.toml` (new), `xtask/Cargo.toml` (new), `xtask/src/main.rs` (new), `xtask/src/host.rs` (new: repo root, node resolver, run helper, Node naming), `xtask/tests/cli.rs` (new), `test/rust-scaffold.mocha.js`.

**Approach.** Add `xtask` as a workspace member with workspace package fields and `publish = false`; `main.rs` parses the first arg and dispatches to `build_napi`, `package`, `bench_startup` (stubs until their unit lands), printing usage and exiting 2 otherwise. `host.rs` holds the `run(cmd, args, cwd, env)` helper returning the child's code (`None` → 1) and the Node-name mapping (KTD4, KTD5). Keep clippy `-D warnings` clean.

**Patterns to follow.** `crates/wait-on-core/Cargo.toml` workspace field inheritance; `test/rust-scaffold.mocha.js` for reading `Cargo.toml` lines in mocha.

**Test scenarios.**
- `xtask/tests/cli.rs`: run the binary with no args → exit code 2, stderr contains `build-napi`, `package`, `bench-startup`.
- `xtask/tests/cli.rs`: run with `frobnicate` → exit code 2, stderr names the unknown subcommand.
- `xtask/src/host.rs` unit: `run` of `node -e "process.exit(7)"` → returns 7 (T-L12-3).
- `xtask/src/host.rs` unit: `run` of `node -e "process.exit(0)"` → returns 0.
- `xtask/src/host.rs` unit: `run` passing the argument `a && b > c` to a node script that echoes it → output is exactly `a && b > c` (no shell).
- `xtask/src/host.rs` unit: `node_name(os, arch)` for `("macos","aarch64")` → `("darwin","arm64")`, `("windows","x86_64")` → `("win32","x64")`, `("linux","aarch64")` → `("linux","arm64")`, `("linux","x86_64")` → `("linux","x64")`; an unsupported pair → error.
- `xtask/src/host.rs` unit: `prebuild_dir("linux","x64",true)` → `linux-x64-musl`; `("darwin","arm64",false)` → `darwin-arm64`.
- `test/rust-scaffold.mocha.js`: `Cargo.toml` members include `xtask` and `.cargo/config.toml` defines `xtask = "run --package xtask --"`.

**Verification.** `cargo test -p xtask`, `npm test`, clippy clean.

### U2. `build-napi` subcommand and the prebuild-dir drift check

**Goal.** `cargo xtask build-napi [--target <triple>] [extra]` reproduces `scripts/build-napi.js`.

**Requirements.** R-L12-5, R-L12-6, R-L12-3 (adds `serde_json`); KD1, KTD3, KTD4.

**Dependencies.** U1.

**Files.** `xtask/Cargo.toml` (serde_json), `Cargo.lock`, `supply-chain/imports.lock` (+ `config.toml` only if an exemption is needed), `xtask/src/build_napi.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`.

**Approach.** Port `TARGETS` as a const array of (triple, platform, arch, musl); `parse_args` strips `--target <t>`; `host_triple` parses `rustc -vV`; `plan` returns output dir, prebuild path and the napi arg vector; `run` spawns `<node> <bin.napi>` (KTD3) with cwd repo root, then requires exactly one `.node` in the output dir and copies it, printing `built prebuilds/<dir>/wait-on.node`. Add serde_json and update the vet store as its own RED/GREEN (R3). `scripts/build-napi.js` stays until U6 because `ci-rs.js` and `ci-rs-package.js` still import it.

**Patterns to follow.** `scripts/build-napi.js`; `docs/guides/development.md` "Vetting a new or bumped crate".

**Test scenarios.**
- Unit: each of the eight triples plans `prebuilds/<dir>/wait-on.node` with the PO4 dir (table-driven).
- Unit: `riscv64gc-unknown-linux-gnu` → error message contains the input and `x86_64-unknown-linux-gnu`.
- Unit: `host_triple` of a four-line `rustc -vV` sample → `aarch64-apple-darwin`.
- Unit: plan for `x86_64-unknown-linux-gnu` → output dir `<root>/target/napi/linux-x64` and napi args exactly `build --release --manifest-path <root>/crates/wait-on-napi/Cargo.toml --output-dir <out> --target x86_64-unknown-linux-gnu`.
- Unit: plan with extra `-x` → last napi arg is `-x`.
- Unit: `parse_args(["--target","x86_64-unknown-linux-musl","-x"])` → target set, extra `["-x"]`; `parse_args([])` → no target, no extra; `["--target"]` → error.
- Unit: an output dir with zero `.node` files → error naming the dir and `none`; with two → error listing both.
- Unit (T-L12-2 drift): `node -e` printing `prebuildDir` from `lib/engine.js` for all eight targets equals the Rust `prebuild_dir` for each.
- Unit (live): host dir equals `node -p` of `process.platform + '-' + process.arch` (musl suffix from the glibc report).
- `xtask/tests/cli.rs`: `build-napi --target riscv64gc-unknown-linux-gnu` → non-zero exit, stderr lists supported triples (no napi spawn happens).
- Check (R3): `cargo vet --locked` observed failing on the new lock before the store change and passing after; `cargo deny check` green.

**Verification.** `cargo test -p xtask`; `cargo vet --locked`; `cargo deny check`; on the dev host `cargo xtask build-napi` produces the host prebuild.

### U3. `bench-startup` subcommand

**Goal.** `cargo xtask bench-startup [--runs N] [--record]` reproduces `scripts/bench-startup.js`, with a byte-identical baseline round-trip.

**Requirements.** R-L12-11; KTD2, KTD8.

**Dependencies.** U1 (U2 for a host prebuild when running it).

**Files.** `xtask/src/bench.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`.

**Approach.** Typed `Baseline` per KTD2; `median`, `verdict`, `with_recording`, `civil_date(epoch_secs)` as pure functions; `std::net::TcpListener` on `127.0.0.1:0` with an accept thread that drops connections; interleaved spawns of `<node> bin/wait-on tcp:127.0.0.1:<port> -t 10000` with `WAIT_ON_ENGINE` per engine and captured stderr, measured with `Instant`; a failed run returns the JS-shaped message and exit 1; FAIL sets exit 1 after printing the summary; `--record` writes the file and logs the key. `scripts/bench-startup.js` stays until U6 because `ci-rs.js` names it.

**Patterns to follow.** `scripts/bench-startup.js`; `test/scripts.mocha.js` `bench:startup` cases; AGENTS.md clock rule.

**Test scenarios.**
- Unit: `median([3,1,2])` → 2; `median([4,1,3,2])` → 2.5; `median([7])` → 7.
- Unit: verdict `js 200, rust 240, {0.25, 50}` → ok, allowed 50, overhead 40.
- Unit: verdict `js 100, rust 145` → ok, allowed 50 (floor wins).
- Unit: verdict `js 100, rust 160` → not ok, message contains `60`, `50`, `100`, `160` and `FAIL`.
- Unit: `with_recording` on a baseline with `linux-x64` adding `darwin-arm64` → both keys present, `linux-x64` untouched, original unchanged.
- Unit (R4): parse `benchmarks/startup-baseline.json`, serialise with the writer → bytes identical to the committed file.
- Unit (R4): a baseline whose `recorded` keys are `win32-x64` then `darwin-arm64` (non-lexicographic), recording `linux-x64` → serialised keys stay `win32-x64`, `darwin-arm64`, then `linux-x64`, and the two untouched entries are byte-identical.
- Unit: a sample `{100.0, 102.7, 2.7}` serialises as `"jsMs": 100` (no `.0`) and `"rustMs": 102.7`.
- Unit: `civil_date(0)` → `1970-01-01`; `civil_date(1_790_726_400)` → `2026-09-30`; `civil_date(1_709_164_800)` → `2024-02-29`.
- Unit: `--runs 0`, `--runs -1`, `--runs abc` → error `--runs must be a positive integer, got <v>`; no `--runs` → baseline `runs` (20); `--runs 5 --record` → 5 and record.
- `xtask/tests/cli.rs` (carried from mocha): `bench-startup --runs 1` with `WAIT_ON_NATIVE_LIBRARY_PATH` pointing at a missing file → non-zero exit, output contains `rust-strict` and the missing path (real node spawn).

**Verification.** `cargo test -p xtask`; `cargo xtask bench-startup --runs 3` on the dev host prints the summary line and exits 0 with a host prebuild.

### U4. `package` subcommand and the probe asset

**Goal.** `cargo xtask package [--host-only]` reproduces `scripts/ci-rs-package.js` with the probe carried as `xtask/assets/prebuild-probe.js`.

**Requirements.** R-L12-8, R-L12-9, R-L12-10, R-L12-4; KTD1, KTD2.

**Dependencies.** U1, U2 (`TARGETS`, host dir).

**Files.** `xtask/Cargo.toml` (ring at the locked version), `Cargo.lock`, `xtask/src/package.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`, `xtask/assets/prebuild-probe.js` (moved from `scripts/prebuild-probe.js`), `scripts/ci-rs-package.js` (one-line probe path update so the JS stays green until U6), `test/scripts.mocha.js` (probe describe moved out), `test/prebuild-probe.mocha.js` (new), `package.json` (`lint` gains `xtask/**/*.js`).

**Approach.** Port every pure helper with the same names and messages; the not-shipped prefix list gains `xtask/` and `.cargo/`; `pack` output is a `serde_json` struct (`filename`, `size`, `unpackedSize`, `files[{path,size}]`); `sha256sums_line` via ring; `install_cells` build `node $npm_execpath ...` argument vectors with the override scrubbed; `probe_verdict`, `docker_decision`, `container_cells` are pure; `main` follows the JS control flow. `assert_installed_addon` normalizes Windows verbatim `\\?\` prefixes from `canonicalize` on both sides. The probe tests move verbatim into `test/prebuild-probe.mocha.js` pointing at the new path.

**Patterns to follow.** `scripts/ci-rs-package.js`; `test/scripts.mocha.js` `ci:rs:package` block; `docs/guides/ci.md` "ci:rs:package".

**Test scenarios.**
- Unit: `expected_prebuild_dirs()` → exactly the eight PO4 dirs in table order.
- Unit: `required_dirs(host_only=true)` → one entry equal to the host dir; `false` → eight.
- Unit: temp prebuilds root with three dirs present → `missing_prebuilds` names the other five; `format_missing` contains `prebuilds/<dir>/wait-on.node` for each; all eight present → empty.
- Unit: `check_pack` with all eight prebuilds and nothing else → no problems; missing `win32-x64` plus `scripts/prebuild-probe.js` → problems name both; `xtask/src/main.rs` and `.cargo/config.toml` → both flagged `must not ship` (R-L12-4); `Cargo.lock` → flagged.
- Unit (skips without `npm_execpath`): a temp project with the repo `package.json`, stub shipped files, eight prebuilds and intermediates (`target/x crates/x scripts/x docs/x test/x benchmarks/x xtask/x .cargo/x Cargo.toml`) → `node $npm_execpath pack --dry-run --json` parsed, `check_pack` empty, every prebuild path present.
- Unit: `check_manifest` with `postinstall` → names it; with `prepare` → names it; with `optionalDependencies` → names it; the repo `package.json` → empty.
- Unit: `size_report` with eight 600000-byte addons, `lib/wait-on.js` 150000, `package.json` 50000, packed 700000, unpacked 5000000 → `jsOnlyUnpacked` 200000 and eight target rows.
- Unit: `sha256sums_line` of a file containing `abc` named `wait-on-1.0.0.tgz` → `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  wait-on-1.0.0.tgz\n`.
- Unit: `parse_args(["--host-only"])` → host only; `[]` → not.
- Unit: `install_cells` → names `npm`, `npm-omit-optional`, `pnpm`; every cell's first arg is the npm path, contains `--ignore-scripts` and the tgz, env lacks `WAIT_ON_NATIVE_LIBRARY_PATH` and keeps `PATH`; only the second has `--omit=optional`; the third has `exec --package pnpm@10.34.6 pnpm add`; caller's env untouched.
- Unit: `assert_installed_addon` accepts `<project>/node_modules/wait-on/prebuilds/<host>/wait-on.node`, rejects the repo's own prebuild path (message names it) and a wrong dir (message names the host dir).
- Unit: the verbatim-prefix normalizer on `\\?\C:\p\x` → `C:\p\x`; on `/p/x` → `/p/x`.
- Unit: `container_cells(arch="x64")` → four cells `glibc-ready, glibc-timeout, musl-ready, musl-timeout`, images trixie-slim ×2 and alpine ×2, expected dirs `linux-x64`/`linux-x64-musl`, run args include `--read-only`, `--network none`, `-e WAIT_ON_ENGINE=rust-strict`, `<tag> node /app/prebuild-probe.js`, npmrc `ignore-scripts=true\n`, Dockerfile has `FROM <image>`, `prebuild-probe.js`, `--omit=optional`; timeout cells end `--no-listener --timeout 1000`; `arch="arm64"` → `linux-arm64`/`linux-arm64-musl`.
- Unit: the docker context plan copies `xtask/assets/prebuild-probe.js` and the asset exists.
- Unit: `docker_decision` matrix: `(found=false, ci=false)` → skip with a one-line reason naming docker; `(false,true)` → fail naming CI; `(true,true)` → run; `(true,false)` → run.
- Unit: `probe_verdict` ready branch: all good → pass; `api: "boom"` → mentions `api`; `cli: 1` → mentions `cli`; `cli: null` → mentions `cli`; wrong expected dir → mentions `linux-x64`; a Windows-style realpath with backslashes still matches.
- Unit: `probe_verdict` timeout branch: both timed out → pass; `cliError` without the timeout text → mentions `cli`; `api: true` → mentions `api`; `cli: 0` → mentions `cli`.
- `xtask/tests/cli.rs`: `package` with `npm_execpath` unset → non-zero exit, stderr contains `run this through npm`.
- `xtask/tests/cli.rs`: `package --host-only` with `npm_execpath` set and an empty temp `prebuilds` root → non-zero exit naming the host's `prebuilds/<dir>/wait-on.node`.
- `test/prebuild-probe.mocha.js` (moved, three cases): ready fixture → exit 0 and JSON line with `addonPath`, `realpath`, `pkgDir`, `api true`, `cli 0`; `--no-listener --timeout 300` with refused fixture → non-zero, stderr and `api`/`cliError` contain `Timed out waiting for`; missing addon under rust-strict → non-zero, stderr contains `WAIT_ON_ENGINE=rust-strict`.

**Verification.** `cargo test -p xtask`; `npm test` (probe tests, lint over `xtask/**/*.js`); `cargo xtask package --host-only` via npm on the dev host completes.

### U5. Justfile, mise pin and the rust-strict guard

**Goal.** `just --list` is the Rust front door; `just ci` composes the full gate; Windows needs no `sh`; a lost `WAIT_ON_ENGINE` fails loudly.

**Requirements.** R-L12-17, R-L12-7, R-L12-18, R-L12-19; KD2, KD4, KTD7, KTD9.

**Dependencies.** U2, U3, U4.

**Files.** `Justfile` (new), `mise.toml`, `test/helpers/assert-rust-strict.js` (new), `test/justfile.mocha.js` (new).

**Approach.** Write the recipes of R-L12-17 with doc comments, `set windows-shell := ["cmd.exe", "/c"]`, variadic `*args` on the three xtask recipes, `$WAIT_ON_ENGINE="rust-strict"` on `mocha-rust` and `ci` as the dependency chain of R-L12-7. The guard helper throws with a one-line message unless `process.env.WAIT_ON_ENGINE === 'rust-strict'`. `test/justfile.mocha.js` skips when `just --version` cannot be spawned and uses `just -n` as the oracle.

**Patterns to follow.** KTD7; `test/helpers/engine-env.js` for spawning; `test/rust-scaffold.mocha.js` for file-shape checks.

**Test scenarios.**
- `test/justfile.mocha.js`: `just --list` output contains each of `fmt lint test deny vet cov build-napi package bench-startup mocha-rust ci`.
- `test/justfile.mocha.js`: `just -n ci` prints, in order, `cargo vet --locked`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `cargo xtask build-napi`, the mocha line with `--require test/helpers/assert-rust-strict.js` and `--exit`, and `cargo xtask bench-startup` last.
- `test/justfile.mocha.js`: `just -n build-napi --target x86_64-unknown-linux-musl -x` prints `cargo xtask build-napi --target x86_64-unknown-linux-musl -x`.
- `test/justfile.mocha.js`: `just -n package --host-only` → `cargo xtask package --host-only`; `just -n bench-startup --runs 1 --record` → `cargo xtask bench-startup --runs 1 --record`.
- `test/justfile.mocha.js`: `Justfile` contains `set windows-shell := ["cmd.exe", "/c"]` and `$WAIT_ON_ENGINE="rust-strict"` on `mocha-rust`; no recipe line contains `&&`, `|` or a `NAME=value` prefix.
- `test/justfile.mocha.js`: `node --require test/helpers/assert-rust-strict.js -e ""` with `WAIT_ON_ENGINE=rust-strict` → exit 0; unset → non-zero and stderr names `WAIT_ON_ENGINE`; `js` → non-zero.
- `test/justfile.mocha.js`: `mise.toml` `[tools]` pins `just`.

**Verification.** `npm test` (with and without `just` on `PATH`); `just --list`; on the dev host `just ci` runs the full chain green.

### U6. Switch the npm aliases, delete the JS, update the docs

**Goal.** `npm run ci:rs`, `build:napi`, `ci:rs:package`, `bench:startup` go through `just`; the ported JS and its mocha file are gone; guides and `AGENTS.md` describe just + xtask.

**Requirements.** R-L12-12, R-L12-13, R-L12-15, R-L12-16, R-L12-14 (untouched, asserted); KD3, KD5, KD6.

**Dependencies.** U5.

**Files.** `package.json` (four aliases, `lint` globs), `scripts/build-napi.js`, `scripts/ci-rs.js`, `scripts/ci-rs-package.js`, `scripts/bench-startup.js` (deleted), `test/scripts.mocha.js` (deleted), `docs/guides/development.md`, `docs/guides/ci.md`, `docs/guides/testing.md`, `docs/guides/architecture.md` (only if it names a deleted script), `AGENTS.md`, `test/justfile.mocha.js` (alias assertions).

**Approach.** RED: a `test/justfile.mocha.js` case asserting the four `package.json` entries equal `just ci`, `just build-napi`, `just package`, `just bench-startup` and that `lint` globs exclude `scripts/**/*.js` and include `xtask/**/*.js`; then switch, delete, and sweep the guides: `development.md` (prerequisites: just via mise; Commands rows for the four aliases; `just --list`; `cargo test -p xtask`), `ci.md` (hook contract rows name the recipes and xtask subcommands; the `ci:rs:package` section names `cargo xtask package` and `xtask/assets/prebuild-probe.js`; not-shipped list adds `xtask/` and `.cargo/`; note that the CI `just` install is the operator's), `testing.md` (suite table: `xtask` `#[test]`s and `xtask/tests/cli.rs` replace `test/scripts.mocha.js`; add `test/prebuild-probe.mocha.js` and `test/justfile.mocha.js`; startup benchmark section names `just bench-startup`), `AGENTS.md` ("Two engines": Rust tasks run through the root `Justfile`, larger ones in `xtask/`; Commands: lint globs). `benchmarks/http-ffi.js` and `test/benchmarks.mocha.js` are not touched.

**Patterns to follow.** KD-S9 (docs are part of done); existing guide tables.

**Test scenarios.**
- `test/justfile.mocha.js`: `package.json` scripts `ci:rs`, `build:napi`, `ci:rs:package`, `bench:startup` equal the four `just` one-liners exactly.
- `test/justfile.mocha.js`: `package.json` `lint` contains `xtask/**/*.js` and not `scripts/**/*.js`.
- `test/justfile.mocha.js`: no file matches `scripts/*.js`, `test/scripts.mocha.js` does not exist, `scripts/reindex-codebase-memory.sh` exists.
- `test/benchmarks.mocha.js` unchanged and green (R-L12-14).
- Docs check: a search of `docs/guides` and `AGENTS.md` for `scripts/` returns only `reindex-codebase-memory.sh`.

**Verification.** `npm test`; `npm run ci:rs`, `npm run build:napi`, `npm run ci:rs:package -- --host-only`, `npm run bench:startup` on the dev host (T-L12-4); the docs search above.

---

## Verification Contract

- `npm test` green with and without `just` on `PATH`, and without a Rust toolchain (just-dependent tests skip, never fail).
- `cargo test -p xtask` green standalone (no `npm_execpath`: the pack test skips) and under `npm run ci:rs`.
- `cargo vet --locked` and `cargo deny check` green after `serde_json`/`zmij`/`indexmap` land; the vet gate observed red on the new lock before the store update.
- `just --list` shows every R-L12-17 recipe; `npm run ci:rs` (= `just ci`) runs vet, fmt, lint, test, deny, host build, mocha under rust-strict, bench, in that order, on the dev host.
- `npm run build:napi`, `npm run ci:rs:package -- --host-only` (install cells pass; container cells run when docker is present) and `npm run bench:startup` succeed via just → xtask on the dev host (T-L12-4).
- Baseline round-trip test proves `benchmarks/startup-baseline.json` is byte-identical through the writer (R4).
- CI on the PR: `build` green immediately; `rust` (3 OS), `napi` (8 rows) and `package` green after D1's operator PR is merged into the branch; the windows rows prove R5.

---

## Definition of Done

- R-L12-1..R-L12-19 met; T-L12-1 cases present as Rust tests (or the three probe cases in `test/prebuild-probe.mocha.js`), T-L12-2 drift test, T-L12-3 exit-code tests and T-L12-4 observations recorded in the PR body.
- Every dispatch branch has a test: xtask subcommand dispatch (three valid, unknown, none), `TARGETS` (eight + unknown), `docker_decision` (four cells), `probe_verdict` (ready and timeout, each pass/fail branch), `check_pack`/`check_manifest` problem kinds, `--host-only` both values.
- `Justfile`, `mise.toml`, `xtask/`, `.cargo/config.toml` committed; `supply-chain/` changes hand-noted, LF, format-stable; `Cargo.lock` updated.
- Cleanup: `scripts/*.js` and `test/scripts.mocha.js` deleted; no `.only`/`.skip`/disabled tests; no stray `wait-on-*.tgz`, `SHA256SUMS`, `prebuilds/` or temp projects in the diff; no abandoned-attempt code; no `.github/workflows/` change.
- Guides and `AGENTS.md` updated in the same PR; `/ce-compound` run if the just/Windows or vet work produced a non-obvious learning.
- PR opened against `spike-next-rs` with `Closes #75`, dependency on the operator `just` PR (D1) stated in the body.

---

## Resume notes

- 2026-10-01: PM steer folded in before implementation — root `Justfile` is the front door (KD2); xtask keeps only `build-napi`, `package`, `bench-startup`; npm aliases call `just`; `just` pinned in `mise.toml`; CI `just` install is an operator PR (D1). Issue #75 body still contains the older "cargo xtask aliases" sentence; the steer supersedes it.
- 2026-10-01 (later): PM steer supersedes the Justfile steer — NO Justfile and no `just`; `cargo xtask` is the single front door. Effects on this plan:
  - KD2, R-L12-17, R-L12-18, R-L12-19, KTD7, KTD9, D1 and U5 as written are superseded. No `Justfile`, no `mise.toml` pin, no `test/justfile.mocha.js`, no `test/helpers/assert-rust-strict.js` (none were ever added).
  - U5 becomes: xtask subcommands `ci`, `fmt`, `lint`, `test`, `cov` (cargo llvm-cov, for L13) alongside `build-napi`, `package`, `bench-startup`; `cargo xtask --help` lists all eight; each with Rust tests (step list/order as a pure fn, `--help` CLI test, dispatch branches).
  - `ci` runs, stopping at the first failure: `cargo vet --locked` (kept first, per KD4/L11: an unvetted crate fails before any build), `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, host `build-napi` (in-process), mocha under `WAIT_ON_ENGINE=rust-strict`, then `bench-startup` last (kept from today's `ci:rs` so `npm run ci:rs` behaves as before; the steer's list omits it — flagged in the PR body).
  - R-L12-12: `package.json` `ci:rs` = `cargo xtask ci`, `build:napi` = `cargo xtask build-napi`, `ci:rs:package` = `cargo xtask package`, `bench:startup` = `cargo xtask bench-startup`. U6 tests assert these.
  - CI needs no new tool, so the operator-PR dependency (D1) is gone; R1 (package job relies on rustup auto-install) stays.
  - Known deviation from U3: no 30 s per-spawn kill in `bench-startup` (wait-on's own `-t 10000` bounds each run) — PR residual.
- 2026-10-01 (post-merge, #80 → #81 → this PR): follow-ups after L12 merged.
  - The 30 s per-spawn kill landed in #80's review fixes, and `ci:rs` became `cargo vet --locked && cargo xtask ci` (vet before cargo compiles xtask). R1 is cleared: the `package` job passed on the first push run.
  - Windows-only failures the PR runs could not show. PR CI runs one representative row per job, so Windows `rust` first ran on push: (1) inner cargo relinking the running `xtask.exe` (fixed in #81 with `<target>/xtask-inner`); (2) `PATH` vs Windows `Path` env lookup, and (3) `startup-baseline.json` checked out as CRLF (fixed here with `host::env_get`/`env_set`/`env_remove` and a root `.gitattributes` `*.json text eol=lf`). Details: `docs/solutions/best-practices/cargo-xtask-alias-compiles-deps-before-the-vet-gate.md`.
  - Local `npm run ci:rs` can fail with `EADDRINUSE ::1:3998` (and 3000/3001/3011) when sibling lane worktrees run mocha at the same time. It is not a lane defect; re-run or run the one file.
  - Operator knowledge rule: anything saved to agent memory must also land in the repo, in `docs/solutions/` (via `/ce-compound mode:non-interactive`) or these Resume notes, in the same PR.
