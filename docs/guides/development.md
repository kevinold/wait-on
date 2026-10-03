# Development

## Prerequisites

- Node `>=22.19.0` (`engines` in [`package.json`](../../package.json); CI installs with `npm ci --engine-strict`).
- For Rust work: rustup (reads the pinned channel and components from `rust-toolchain.toml`) `cargo-deny` (`cargo install cargo-deny --locked`), `cargo-vet` (`cargo install cargo-vet --locked`) and `cargo-llvm-cov` (`cargo install cargo-llvm-cov --locked`) for the [Rust coverage](testing.md#rust-coverage) gate in `ci:rs`; rustup installs the pinned `llvm-tools-preview` component it needs. `npm test` needs none of them.

## Setup

```bash
git clone https://github.com/kevinold/wait-on.git
cd wait-on
git checkout spike-next-rs
npm ci
cargo xtask hooks   # once per clone: commit messages checked locally (AGENTS.md › Commit messages)
npm test
```

## Commands

| Command | Does | Status |
|---|---|---|
| `npm run lint` | eslint over `lib/**/*.js`, `test/**/*.js`, `benchmarks/**/*.js`, `xtask/**/*.js`, `features/**/*.js`, `cucumber.js`, `bin/wait-on` | exists |
| `npm test` | `lint` + `test:types` + `test:mocha` | exists |
| `npm run test:mocha` | `mocha --exit "test/**/*.mocha.js"` | exists |
| `npm run test:types` | `tsc -p test/tsconfig.json` (type tests for `index.d.ts`) | exists |
| `npm run test:coverage` | nyc + mocha, thresholds from `.nycrc.json` | exists |
| `npm run ci:rs` | `cargo vet --locked && cargo xtask ci && cargo xtask cov --exclude xtask --exclude wait-on-features --fail-under-lines 100 --fail-under-regions 100` (vet runs before cargo builds xtask), the Rust gate on the host: `cargo vet --locked`, fmt, clippy `-D warnings`, `cargo test` (with the Rust `@engine` contract runner), `cargo deny check`, host addon, mocha under `rust-strict`, the consumer contract, the startup benchmark, then 100% line and region coverage of `wait-on-core`; stops at the first failure | exists |
| `npm run bench:startup [-- --runs N] [--record]` | startup overhead of the Rust engine over JS (`cargo xtask bench-startup`); needs a host prebuild; `--record` rewrites this host's entry in `benchmarks/startup-baseline.json` | exists (L8) |
| `npm run build:napi [-- --target <triple> [-x]]` | build the host (or one target's) addon into `prebuilds/` (`cargo xtask build-napi`; `--target` without a value is an error) | exists |
| `cargo xtask cov --exclude xtask --exclude wait-on-features --fail-under-lines 100 --fail-under-regions 100` | the coverage gate alone, the last step of `ci:rs` ([testing.md](testing.md#rust-coverage)); `--summary-only` in place of the thresholds prints the table without failing | exists (L13) |
| `node benchmarks/http-ffi.js [--iterations N] [--engines js,rust-strict]` | per-check http overhead, JS vs Rust (Rust rows need a host prebuild) | exists |
| `npm run contract [-- --tgz <path>] [--fixture cjs\|esm\|ts] [--engine js\|rust-strict]` | the consumer contract (`cargo xtask contract`): packs the working tree unless `--tgz`, installs it into each fixture project, runs `features/` with cucumber-js under each engine ([testing.md](testing.md#consumer-contract)); `rust-strict` needs a host prebuild | exists (L16) |
| `npm run dependents [-- --only <name>] [--include-optional] [--tgz <path>] [--keep]` | the real-world dependents harness (`cargo xtask dependents`): clones each `xtask/assets/dependents.json` entry at its pinned tag and runs its own suite on the published wait-on, then on the packed working tree under `js` and `rust-strict` ([testing.md](testing.md#dependents-harness)); on demand only, needs network, `git` and a host prebuild | exists (L19) |
| `npm run ci:rs:package [-- --host-only]` | `cargo xtask package`: pack, install-matrix and container checks, size report ([ci.md](ci.md#cirspackage)); `--host-only` needs only the host prebuild | exists |

Hook inputs and outputs: [ci.md](ci.md#npm-script-hook-contract).

## xtask

The Rust-side tooling is `cargo xtask` (crate `xtask/`, alias in `.cargo/config.toml`); the npm scripts above call it. `cargo xtask --help` lists the eleven subcommands: `ci`, `fmt`, `lint`, `test`, `cov` (`cargo llvm-cov --workspace`, needs `cargo-llvm-cov`), `build-napi`, `package`, `contract`, `dependents`, `bench-startup`, `hooks` (sets `core.hooksPath` to `.githooks` so `.githooks/commit-msg` checks commit messages). Run `package` through `npm run ci:rs:package`, `contract` through `npm run contract` and `dependents` through `npm run dependents`. Its tests: `cargo test -p xtask` (needs `node` on `PATH`; the `npm pack` test skips unless run through npm). `xtask/assets/prebuild-probe.js` stays JS because it runs under Node inside installed packages and containers. The cargo calls xtask makes itself (`ci`, `fmt`, `lint`, `test`, `cov`) build into `target/xtask-inner/` (or `$CARGO_TARGET_DIR/xtask-inner`), so they never relink the running `xtask` binary, which Windows locks.

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
