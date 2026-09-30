# wait-on developer guides

Developer manual for the two-engine repo on the `spike-next-rs` branch. Every spine lane updates the pages its change affects in the same PR.

## The model

The pure-JS engine (`lib/wait-on.js`) is the default and stays authoritative. A Rust engine is being built beside it and will be opt-in via `WAIT_ON_ENGINE=rust` (`rust-strict` turns an addon load failure into an error). One contract, two drivers: the existing mocha suites are the parity contract and CI runs them under each engine, with an explicit pending list that shrinks lane by lane. The work lands on `spike-next-rs` as single-PR lanes L1–L10 driven from spine issue `kevinold/wait-on#35`; see the [spine plan](../plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md). Today only the JS engine exists (package `10.0.0-rc.1`).

## Pages

| Page | Covers |
|---|---|
| [architecture.md](architecture.md) | JS engine today, target Rust layout, engine selection and fallback |
| [development.md](development.md) | Prerequisites, setup, commands, running each engine |
| [testing.md](testing.md) | Suites, fake clock, Windows notes, dual-engine runs, pending list |
| [ci.md](ci.md) | CI jobs, npm script hook contract, napi target matrix, prerelease workflow |
| [releasing.md](releasing.md) | Node channels (pointer), Rust test prereleases on the fork, deferred items |
| [contributing-dual-engine.md](contributing-dual-engine.md) | Change workflow, spine lanes vs operator PRs, docs-as-done checklist |

Rules for agents and contributors (TDD, conventions) live in [AGENTS.md](../../AGENTS.md).
