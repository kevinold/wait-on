# Development

## Prerequisites

- Node `>=22.19.0` (`engines` in [`package.json`](../../package.json); CI installs with `npm ci --engine-strict`).
- For Rust work: rustup (reads the pinned channel and components from `rust-toolchain.toml`) and `cargo-deny` (`cargo install cargo-deny --locked`). `npm test` needs neither.

## Setup

```bash
git clone https://github.com/kevinold/wait-on.git
cd wait-on
git checkout spike-next-rs
npm ci
npm test
```

## Commands

| Command | Does | Status |
|---|---|---|
| `npm run lint` | eslint over `lib/**/*.js`, `test/**/*.js`, `scripts/**/*.js`, `benchmarks/**/*.js`, `bin/wait-on` | exists |
| `npm test` | `lint` + `test:types` + `test:mocha` | exists |
| `npm run test:mocha` | `mocha --exit "test/**/*.mocha.js"` | exists |
| `npm run test:types` | `tsc -p test/tsconfig.json` (type tests for `index.d.ts`) | exists |
| `npm run test:coverage` | nyc + mocha, thresholds from `.nycrc.json` | exists |
| `npm run ci:rs` | Rust gate on the host (`scripts/ci-rs.js`): fmt, clippy `-D warnings`, `cargo test`, `cargo deny check`, host addon, mocha under `rust-strict` | exists |
| `npm run build:napi [-- --target <triple> [-x]]` | build the host (or one target's) addon into `prebuilds/` (`scripts/build-napi.js`) | exists |
| `node benchmarks/http-ffi.js [--iterations N] [--engines js,rust-strict]` | per-check http overhead, JS vs Rust (Rust rows need a host prebuild) | exists |
| `npm run ci:rs:package [-- --host-only]` | pack, install-matrix and container checks, size report ([ci.md](ci.md#cirspackage)); `--host-only` needs only the host prebuild | exists |

Hook inputs and outputs: [ci.md](ci.md#npm-script-hook-contract).

## Building the addon

`npm run build:napi` runs `napi build --release` (from `@napi-rs/cli`) on `crates/wait-on-napi` for the rustc host triple, into `target/napi/<dir>/`, then copies the addon to `prebuilds/<dir>/wait-on.node`. `-- --target <triple>` builds one of the eight supported targets; extra args such as `-x` (cross-compile; napi installs `cargo-zigbuild` on first use and needs `zig` on `PATH`) are forwarded to `napi build`. Nothing is written to the repo root. The first build compiles reqwest, rustls and ring (a C compiler is needed for ring; no cmake, NASM or OpenSSL).

`prebuilds/` is not committed, so rebuild the addon after pulling Rust changes. A stale prebuild that lacks a newer export (for example `fileSize` or `runCommand`) fails the real-addon tests, which run in plain `npm test` whenever a host prebuild exists, as well as any run under `WAIT_ON_ENGINE=rust*`.

## Running each engine locally

JS runs with no setup. For Rust, build the addon, then set `WAIT_ON_ENGINE` in the shell:

```bash
npm run build:napi
WAIT_ON_ENGINE=rust-strict npm run test:mocha   # or: npm run ci:rs for the full gate
```

`rust` falls back to JS silently when no addon is built; use `rust-strict` to be sure the addon loaded.
