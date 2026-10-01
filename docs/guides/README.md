# wait-on developer guides

Developer manual for the two-engine repo on the `spike-next-rs` branch. Every spine lane updates the pages its change affects in the same PR.

## The model

The pure-JS engine (`lib/engine-js.js`, behind the `lib/wait-on.js` front door) is the default and stays authoritative. A Rust engine is being built beside it, opt-in via `WAIT_ON_ENGINE=rust` (`rust-strict` turns an addon load failure into an error). One contract, two drivers: the mocha suites are the front-door parity contract and CI runs them under each engine, with no pending list; engine behaviour itself is specified by Rust tests in `crates/wait-on-core`, held at 100% line and region coverage (L13, [inventory](testing.md#js-vs-rust-inventory)). The work lands on `spike-next-rs` as single-PR lanes L1–L13 driven from spine issue `kevinold/wait-on#35`; see the [spine plan](../plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md). Under the Rust engine one `waitOn` is one `addon.wait` call: the polling loop and every resource check run in Rust, and that path loads neither `rxjs` nor `undici` (L7; package `10.0.0-rc.1`).

## Build, test, ship

`cargo xtask` is the front door for Rust-side work; the npm scripts CI calls are one-line aliases to it.

| Task | Command | Page |
|---|---|---|
| Set up a clone | `npm ci`, `cargo xtask hooks` | [development.md](development.md#setup) |
| Test the JS engine | `npm test` | [testing.md](testing.md) |
| Build the Rust addon | `npm run build:napi` (`cargo xtask build-napi`) | [development.md](development.md#building-the-addon) |
| Test the Rust engine | `WAIT_ON_ENGINE=rust-strict npm run test:mocha`; full gate `npm run ci:rs` (`cargo xtask ci` + `cargo xtask cov`) | [development.md](development.md#running-each-engine-locally), [testing.md](testing.md#rust-coverage) |
| Package and check the tarball | `npm run ci:rs:package -- --host-only` (`cargo xtask package`) | [ci.md](ci.md#cirspackage) |
| Ship a test prerelease | merge to `spike-next-rs`; CI publishes `rs-<version>-<sha7>` on the fork | [releasing.md](releasing.md#rust-test-prereleases-fork) |
| Verify a prerelease | the end-to-end runbook | [releasing.md](releasing.md#end-to-end-prerelease-verification) |

## Pages

| Page | Covers |
|---|---|
| [architecture.md](architecture.md) | Front door and modules, the Rust polling loop, JS vs Rust differences, engine selection and fallback |
| [development.md](development.md) | Prerequisites, setup, commands, running each engine |
| [testing.md](testing.md) | Suites, JS vs Rust test inventory, Rust coverage gate and PEM fixtures, fake clock, Windows notes, dual-engine runs |
| [ci.md](ci.md) | CI jobs, PR vs push matrix, npm script hook contract, napi target matrix, tarball sizes, prerelease workflow |
| [releasing.md](releasing.md) | Node channels (pointer), Rust test prereleases on the fork, end-to-end prerelease verification runbook and results, supply-chain delta, what the cutover still needs |
| [contributing-dual-engine.md](contributing-dual-engine.md) | Change workflow, spine lanes vs operator PRs, docs-as-done checklist |

Rules for agents and contributors (TDD, conventions) live in [AGENTS.md](../../AGENTS.md).
