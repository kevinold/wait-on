# CI

Two files: [`.github/workflows/node.js.yml`](../../.github/workflows/node.js.yml) (workflow `CI`) and [`.github/workflows/rs-prerelease.yml`](../../.github/workflows/rs-prerelease.yml) (workflow `RS Prerelease`). `CI` runs on `push` to `master`, `next`, `*.x`, `spike-next-rs` and on every `pull_request`. Release, commitlint, and PR-title workflows are covered by [`.github/RELEASING.md`](../../.github/RELEASING.md).

## Jobs

| Job | Runs on | Needs | Does |
|---|---|---|---|
| `build` | `ubuntu-latest`, `windows-latest` × Node `22.x`, `24.x`, `26.x` | — | `npm ci --engine-strict`, `npm run build --if-present`, `npm test` |
| `rust` | `ubuntu-latest`, `macos-latest`, `windows-latest`, Node `24.x` | — | `rustup toolchain install` (gated on `rust-toolchain.toml`), `Swatinem/rust-cache@v2` and `taiki-e/install-action@v2` with `tool: cargo-deny,cargo-vet` (gated on `Cargo.toml`), `npm ci --engine-strict`, `npm run --if-present ci:rs` |
| `napi` | 8-row target matrix below, `fail-fast: false`, Node `24.x` | — | `rustup toolchain install` + `rustup target add <target>` (gated on `rust-toolchain.toml`), rust-cache keyed by target and `mlugg/setup-zig@v2` when `matrix.zig` (gated on `Cargo.toml`), `npm ci --engine-strict`, `npm run --if-present build:napi -- --target <target> <extra-args>`, upload `prebuilds/**` as `prebuilds-<target>` (`if-no-files-found: ignore`) |
| `package` | `ubuntu-latest`, Node `24.x` | `napi` | download `prebuilds-*` (merged) into `prebuilds/`, `npm ci --engine-strict`, `npm run --if-present ci:rs:package`, upload `wait-on-*.tgz` + `SHA256SUMS` as `package` (`if-no-files-found: ignore`) |
| `prerelease` | calls `rs-prerelease.yml` | `build`, `rust`, `package` | only on `push` to `refs/heads/spike-next-rs` in `kevinold/wait-on`; `permissions: contents: write` |

`rs-prerelease.yml` (`on: workflow_call` and bare `workflow_dispatch`; top-level `permissions: contents: read`) has one job, `prerelease`, guarded `if: github.repository == 'kevinold/wait-on'` with `contents: write`: checkout, download artifact pattern `package` (merged) into `dist/`, then, only if `dist/wait-on-*.tgz` exists, `gh release create rs-<package.json version>-<sha7> dist/wait-on-*.tgz dist/SHA256SUMS --prerelease --target <sha>` using `GITHUB_TOKEN`. It never publishes to npm.

## npm script hook contract

CI calls these with `npm run --if-present`; an undefined script is a green no-op. Lanes change CI behavior only by defining them in `package.json`. `ci:rs` and `build:napi` exist (L1); `ci:rs:package` exists (L9).

| Script | Called by | Input | Must produce | Status |
|---|---|---|---|---|
| `ci:rs` | `rust` (3 OSes) | toolchain, cargo cache, `cargo-deny`, `cargo-vet`, `npm ci` already done | exit code only; runs `cargo vet --locked` first (the `package.json` entry, so an unvetted crate fails before any build), then builds the host addon itself, runs fmt, clippy `-D warnings`, test, deny, mocha under `WAIT_ON_ENGINE=rust-strict`, then the startup benchmark (`cargo xtask bench-startup`, L8), which fails the job when the Rust overhead exceeds `benchmarks/startup-baseline.json`'s threshold | exists (L1, `cargo xtask ci` since L12; vet gate L11) |
| `build:napi` | `napi` (8 rows) | `-- --target <triple> [extra-args]` (forwarded to `napi build`; `-x` on musl rows) | the target's addon under `prebuilds/<platform>-<arch>[-musl]/wait-on.node` (uploaded as `prebuilds/**`) | exists (L1, `cargo xtask build-napi` since L12); its `TARGETS` table (`xtask/src/build_napi.rs`) is the one list of the eight targets (L9) |
| `ci:rs:package` | `package` | all targets' `prebuilds/**` already downloaded into `prebuilds/` | `wait-on-*.tgz` and `SHA256SUMS` at the repo root; install-matrix checks and size report | exists (L9, `cargo xtask package` since L12); see [below](#cirspackage) |

If a lane needs a different `build:napi` argument shape, the `extra-args` matrix column is the one place to adapt, via an operator PR.

## ci:rs:package

`npm run ci:rs:package [-- --host-only]` (`cargo xtask package`) runs, stopping at the first failure:

1. Guard: every target dir from `build_napi.rs` `TARGETS` must hold `prebuilds/<dir>/wait-on.node`, else it exits 1 listing the missing ones. `--host-only` (developer runs, never CI) requires only the host dir.
2. `npm pack --json` (stale `wait-on-*.tgz` removed first); the file list must hold every required addon and nothing from `target/`, `crates/`, `scripts/`, `docs/`, `test/`, `benchmarks/`, `xtask/`, `.cargo/`, `Cargo.*`; `package.json` must declare no `preinstall`/`install`/`postinstall`/`prepare` script and no `optionalDependencies`.
3. Size report (packed, unpacked, JS-only unpacked, per-target bytes) and `SHA256SUMS` (`sha256sum -c` format).
4. Install cells, each in a fresh temp project with `WAIT_ON_NATIVE_LIBRARY_PATH` removed: `npm install --ignore-scripts`, the same with `--omit=optional`, and `pnpm add --ignore-scripts` (pnpm pinned in `xtask/src/package.rs`, fetched with `npm exec`). `xtask/assets/prebuild-probe.js` then runs under `WAIT_ON_ENGINE=rust-strict`: it loads the installed engine, waits on a local tcp port through the API and the CLI, and prints the addon path, which must be the host dir inside the installed package.
5. AE1 container cells: `node:24-trixie-slim` (glibc) and `node:24-alpine` (musl) images install the tarball at build time with `ignore-scripts=true`, then run with `--read-only --network none -e WAIT_ON_ENGINE=rust-strict`. A ready cell must exit 0 with the addon from `linux-<arch>[-musl]`; a timeout cell (`--no-listener`) must exit non-zero with `Timed out waiting for`. Without docker, or without the linux prebuilds for the runner arch, the cells skip with one line locally and fail under `CI`.

Sizes with all eight prebuilds from `spike-next-rs` (run 36773818018, 2026-09-30):

| | bytes |
|---|---|
| packed (tarball) | 3,359,787 |
| unpacked | 8,793,755 |
| JS-only unpacked | 61,283 |
| JS-only packed (no `prebuilds/`) | 18,892 |
| `darwin-arm64` | 1,220,256 |
| `darwin-x64` | 1,169,072 |
| `linux-x64` | 1,365,872 |
| `linux-arm64` | 1,444,016 |
| `linux-x64-musl` | 926,784 |
| `linux-arm64-musl` | 892,808 |
| `win32-x64` | 894,976 |
| `win32-arm64` | 818,688 |

No size threshold is enforced; the numbers are PO5's decision input.

## napi target matrix

| Target | Runner | zig | extra-args |
|---|---|---|---|
| `aarch64-apple-darwin` | `macos-14` | false | |
| `x86_64-apple-darwin` | `macos-14` | false | |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | false | |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | false | |
| `x86_64-unknown-linux-musl` | `ubuntu-24.04` | true | `-x` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | true | `-x` |
| `x86_64-pc-windows-msvc` | `windows-latest` | false | |
| `aarch64-pc-windows-msvc` | `windows-11-arm` | false | |

Native runners wherever one exists. `x86_64-apple-darwin` cross-compiles on arm64 macOS via `rustup target add`. musl rows cross-compile with zig (`-x` is napi-rs `--cross-compile` via cargo-zigbuild).

## No-op gating

Every Rust-specific step has a step-level guard: toolchain steps `if: hashFiles('rust-toolchain.toml') != ''`, cache/cargo-deny+cargo-vet install/zig steps `if: hashFiles('Cargo.toml') != ''`. Script steps always run and no-op through `--if-present`. So `rust`, `napi`, and `package` stay green with those steps `skipped` until a lane adds `Cargo.toml` + `rust-toolchain.toml`. A lane that defines `ci:rs` must add both files in the same PR, or `ci:rs` runs without a toolchain.

## Why `rs-prerelease.yml` is reusable

GitHub fires `workflow_run` and `workflow_dispatch` only for workflow files on the default branch (`master`); this file lives on `spike-next-rs`. So it is a `workflow_call` workflow invoked by the `prerelease` job in the same run, which also lets `download-artifact` fetch `package` without a run id. Its `workflow_dispatch` stays inert until the file reaches the default branch. Download uses `pattern: package` (not `name:`) so a run with no package artifact succeeds and the tarball guard skips the release.

## Why lanes cannot edit workflows

KD-S7 in the [spine plan](../plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md): CI owns no lane logic. Spine lanes may not edit `.github/workflows/`; they change behavior through the hook scripts above. Any CI change a lane discovers it needs is filed as an operator PR, never folded into a lane.
