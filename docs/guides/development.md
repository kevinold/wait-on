# Development

## Prerequisites

- Node `>=22.19.0` (`engines` in [`package.json`](../../package.json); CI installs with `npm ci --engine-strict`).
- For Rust work: rustup (reads the pinned channel and components from `rust-toolchain.toml`) `cargo-deny` (`cargo install cargo-deny --locked`) and `cargo-vet` (`cargo install cargo-vet --locked`). Optional, for the [Rust coverage](testing.md#rust-coverage) check: `cargo-llvm-cov` (`cargo install cargo-llvm-cov --locked`) and the `llvm-tools-preview` component (`rustup component add llvm-tools-preview`). `npm test` needs none of them.

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
| `npm run ci:rs` | Rust gate on the host: `cargo vet --locked` first (in the `package.json` entry), then `scripts/ci-rs.js`: fmt, clippy `-D warnings`, `cargo test`, `cargo deny check`, host addon, mocha under `rust-strict`, then the startup benchmark | exists |
| `npm run bench:startup [-- --runs N] [--record]` | startup overhead of the Rust engine over JS (`scripts/bench-startup.js`); needs a host prebuild; `--record` rewrites this host's entry in `benchmarks/startup-baseline.json` | exists (L8) |
| `npm run build:napi [-- --target <triple> [-x]]` | build the host (or one target's) addon into `prebuilds/` (`scripts/build-napi.js`) | exists |
| `cargo llvm-cov -p wait-on-core --summary-only --ignore-filename-regex '(waiter/tests\.rs\|/tests/)'` | line and region coverage of `wait-on-core` without the test bodies; loop code stays at 100% ([testing.md](testing.md#rust-coverage)); CI enforcement is L13 | exists (local) |
| `node benchmarks/http-ffi.js [--iterations N] [--engines js,rust-strict]` | per-check http overhead, JS vs Rust (Rust rows need a host prebuild) | exists |
| `npm run ci:rs:package [-- --host-only]` | pack, install-matrix and container checks, size report ([ci.md](ci.md#cirspackage)); `--host-only` needs only the host prebuild | exists |

Hook inputs and outputs: [ci.md](ci.md#npm-script-hook-contract).

## Vetting a new or bumped crate

`npm run ci:rs` fails first when `Cargo.lock` holds a third-party crate version that no imported audit, local audit, or exemption in `supply-chain/` covers (trust sources and counts: [architecture.md](architecture.md#supply-chain)). After adding or bumping a crate:

1. `cargo vet` lists what is unvetted; `cargo vet suggest` gives the cheapest path for each (a diff from an audited version, or a full inspect) and notes when another trusted source or publisher would cover it.
2. Review the code: `cargo vet diff <crate> <audited> <new>` for a bump, `cargo vet inspect <crate> <version>` for a new crate.
3. Record the result with `cargo vet certify <crate> <version>` (or `certify <crate> <from> <to>` for a diff). This writes `supply-chain/audits.toml`.
4. Commit the `supply-chain/` changes with the `Cargo.lock` change and check `cargo vet --locked` passes.

Exemption policy: exempt a crate only when no imported source covers it and certifying is not practical in this change. Each `[[exemptions.<crate>]]` entry carries a `notes` reason. Do not clear a red gate with `cargo vet regenerate exemptions`: it writes exemptions without reasons, and the gate checks only that an exemption exists, so PR review of `supply-chain/config.toml` enforces the reason. Prefer certifying a small diff over exempting. Add a new import (`cargo vet import <registry-name>`, or `cargo vet import <name> <audits.toml URL>` for a source outside the registry, as ZcashFoundation is) only when it removes exemptions.

## Building the addon

`npm run build:napi` runs `napi build --release` (from `@napi-rs/cli`) on `crates/wait-on-napi` for the rustc host triple, into `target/napi/<dir>/`, then copies the addon to `prebuilds/<dir>/wait-on.node`. `-- --target <triple>` builds one of the eight supported targets; extra args such as `-x` (cross-compile; napi installs `cargo-zigbuild` on first use and needs `zig` on `PATH`) are forwarded to `napi build`. Nothing is written to the repo root. The first build compiles reqwest, rustls and ring (a C compiler is needed for ring; no cmake, NASM or OpenSSL).

`prebuilds/` is not committed, so rebuild the addon after pulling Rust changes. A stale prebuild that lacks a newer export (for example `wait`, which the engine requires: without it `rust` falls back to JS and `rust-strict` fails) fails the real-addon tests, which run in plain `npm test` whenever a host prebuild exists, as well as any run under `WAIT_ON_ENGINE=rust*`.

## Running each engine locally

JS runs with no setup. For Rust, build the addon, then set `WAIT_ON_ENGINE` in the shell:

```bash
npm run build:napi
WAIT_ON_ENGINE=rust-strict npm run test:mocha   # or: npm run ci:rs for the full gate
```

`rust` falls back to JS silently when no addon is built; use `rust-strict` to be sure the addon loaded.
