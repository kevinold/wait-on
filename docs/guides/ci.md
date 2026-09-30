# CI

Two files: [`.github/workflows/node.js.yml`](../../.github/workflows/node.js.yml) (workflow `CI`) and [`.github/workflows/rs-prerelease.yml`](../../.github/workflows/rs-prerelease.yml) (workflow `RS Prerelease`). `CI` runs on `push` to `master`, `next`, `*.x`, `spike-next-rs` and on every `pull_request`. Release, commitlint, and PR-title workflows are covered by [`.github/RELEASING.md`](../../.github/RELEASING.md).

## Jobs

| Job | Runs on | Needs | Does |
|---|---|---|---|
| `build` | `ubuntu-latest`, `windows-latest` × Node `22.x`, `24.x`, `26.x` | — | `npm ci --engine-strict`, `npm run build --if-present`, `npm test` |
| `rust` | `ubuntu-latest`, `macos-latest`, `windows-latest`, Node `24.x` | — | `rustup toolchain install` (gated on `rust-toolchain.toml`), `Swatinem/rust-cache@v2` and `taiki-e/install-action@v2` with `tool: cargo-deny` (gated on `Cargo.toml`), `npm ci --engine-strict`, `npm run --if-present ci:rs` |
| `napi` | 8-row target matrix below, `fail-fast: false`, Node `24.x` | — | `rustup toolchain install` + `rustup target add <target>` (gated on `rust-toolchain.toml`), rust-cache keyed by target and `mlugg/setup-zig@v2` when `matrix.zig` (gated on `Cargo.toml`), `npm ci --engine-strict`, `npm run --if-present build:napi -- --target <target> <extra-args>`, upload `prebuilds/**` as `prebuilds-<target>` (`if-no-files-found: ignore`) |
| `package` | `ubuntu-latest`, Node `24.x` | `napi` | download `prebuilds-*` (merged) into `prebuilds/`, `npm ci --engine-strict`, `npm run --if-present ci:rs:package`, upload `wait-on-*.tgz` + `SHA256SUMS` as `package` (`if-no-files-found: ignore`) |
| `prerelease` | calls `rs-prerelease.yml` | `build`, `rust`, `package` | only on `push` to `refs/heads/spike-next-rs` in `kevinold/wait-on`; `permissions: contents: write` |

`rs-prerelease.yml` (`on: workflow_call` and bare `workflow_dispatch`; top-level `permissions: contents: read`) has one job, `prerelease`, guarded `if: github.repository == 'kevinold/wait-on'` with `contents: write`: checkout, download artifact pattern `package` (merged) into `dist/`, then, only if `dist/wait-on-*.tgz` exists, `gh release create rs-<package.json version>-<sha7> dist/wait-on-*.tgz dist/SHA256SUMS --prerelease --target <sha>` using `GITHUB_TOKEN`. It never publishes to npm.

## npm script hook contract

CI calls these with `npm run --if-present`; an undefined script is a green no-op. Lanes change CI behavior only by defining them in `package.json`. `ci:rs` and `build:napi` exist (L1); `ci:rs:package` does not yet.

| Script | Called by | Input | Must produce | Status |
|---|---|---|---|---|
| `ci:rs` | `rust` (3 OSes) | toolchain, cargo cache, `cargo-deny`, `npm ci` already done | exit code only; builds the host addon itself, runs fmt, clippy `-D warnings`, test, deny, mocha under `WAIT_ON_ENGINE=rust-strict`, then the startup benchmark (`scripts/bench-startup.js`, L8), which fails the job when the Rust overhead exceeds `benchmarks/startup-baseline.json`'s threshold | exists (L1, `scripts/ci-rs.js`) |
| `build:napi` | `napi` (8 rows) | `-- --target <triple> [extra-args]` (forwarded to `napi build`; `-x` on musl rows) | the target's addon under `prebuilds/<platform>-<arch>[-musl]/wait-on.node` (uploaded as `prebuilds/**`) | exists (L1, `scripts/build-napi.js`); matrix hardening planned (lane L9) |
| `ci:rs:package` | `package` | all targets' `prebuilds/**` already downloaded into `prebuilds/` | `wait-on-*.tgz` and `SHA256SUMS` at the repo root; install-matrix checks and size report | Status: planned (lane L9) |

If L9 needs a different `build:napi` argument shape, the `extra-args` matrix column is the one place to adapt, via an operator PR.

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

Every Rust-specific step has a step-level guard: toolchain steps `if: hashFiles('rust-toolchain.toml') != ''`, cache/cargo-deny/zig steps `if: hashFiles('Cargo.toml') != ''`. Script steps always run and no-op through `--if-present`. So `rust`, `napi`, and `package` stay green with those steps `skipped` until a lane adds `Cargo.toml` + `rust-toolchain.toml`. A lane that defines `ci:rs` must add both files in the same PR, or `ci:rs` runs without a toolchain.

## Why `rs-prerelease.yml` is reusable

GitHub fires `workflow_run` and `workflow_dispatch` only for workflow files on the default branch (`master`); this file lives on `spike-next-rs`. So it is a `workflow_call` workflow invoked by the `prerelease` job in the same run, which also lets `download-artifact` fetch `package` without a run id. Its `workflow_dispatch` stays inert until the file reaches the default branch. Download uses `pattern: package` (not `name:`) so a run with no package artifact succeeds and the tarball guard skips the release.

## Why lanes cannot edit workflows

KD-S7 in the [spine plan](../plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md): CI owns no lane logic. Spine lanes may not edit `.github/workflows/`; they change behavior through the hook scripts above. Any CI change a lane discovers it needs is filed as an operator PR, never folded into a lane.
