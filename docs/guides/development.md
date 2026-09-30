# Development

## Prerequisites

- Node `>=22.19.0` (`engines` in [`package.json`](../../package.json); CI installs with `npm ci --engine-strict`).
- For Rust work: rustup (reads the pinned channel from `rust-toolchain.toml`) and `cargo-deny`. Neither is needed until a lane adds the Cargo workspace.

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
| `npm run lint` | eslint over `lib/**/*.js`, `test/**/*.js`, `bin/wait-on` | exists |
| `npm test` | `lint` + `test:types` + `test:mocha` | exists |
| `npm run test:mocha` | `mocha --exit "test/**/*.mocha.js"` | exists |
| `npm run test:types` | `tsc -p test/tsconfig.json` (type tests for `index.d.ts`) | exists |
| `npm run test:coverage` | nyc + mocha, thresholds from `.nycrc.json` | exists |
| `npm run ci:rs` | Rust gate on the host (fmt, clippy, test, deny, host addon, mocha under `rust-strict`) | Status: planned (lane L1) |
| `npm run build:napi -- --target <triple>` | build one target's addon into `prebuilds/` | Status: planned (lane L9) |
| `npm run ci:rs:package` | pack all targets, install-matrix checks, size report | Status: planned (lane L9) |

Hook inputs and outputs: [ci.md](ci.md#npm-script-hook-contract).

## Building the addon

Build the host addon with the Cargo workspace and napi-rs.

Status: planned (lane L1)

## Running each engine locally

JS runs today with no setup. Rust runs by setting `WAIT_ON_ENGINE=rust` (or `rust-strict`) once the addon exists.

Status: planned (lane L1)
