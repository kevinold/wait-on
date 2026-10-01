---
title: A gate moved into cargo xtask must still run before cargo builds xtask
date: 2026-09-30
category: best-practices
module: xtask
problem_type: best_practice
component: rust-engine
severity: high
applies_when:
  - Porting an npm or shell script that carries a pre-build gate into a cargo xtask subcommand
  - Adding or bumping a dependency of the xtask crate
  - Changing the ci:rs script line or the .cargo/config.toml xtask alias
root_cause: config_error
resolution_type: config_change
related_components: [package.json, .cargo/config.toml, xtask/src/ci.rs, test/rust-scaffold.mocha.js, supply-chain]
tags: [cargo-vet, xtask, supply-chain, ci, build-scripts, porting, codex-review, spike-next-rs]
---

## Context

Lane L12 (#75; PR not yet open as of this writing) ported the Rust tooling scripts to a `cargo xtask` crate and routed the npm scripts through it. Before the port, the gate sat in the npm script line: `"ci:rs": "cargo vet --locked && node scripts/ci-rs.js"`. Lane L11 put it there because CI behaviour changes go through npm scripts, not `.github/workflows/` (session history). The port first moved `cargo vet --locked` inside `cargo xtask ci` as its first step. That looked equivalent, and a unit test pinned the order. It was not equivalent. The old JS script needed no compile, so the npm line really did vet first. The xtask version does not.

The in-process reviewers and the lane's own tests missed it. They checked that vet ran, not that it ran first. Only the cross-model (codex) adversarial review caught it, as a P1 titled "Cargo compiles xtask before its vet gate" (session history). This follows on from the "prove the gate goes red" guidance in `docs/solutions/best-practices/cargo-vet-store-setup-and-maintenance.md`.

## Guidance

**A gate that must run before a build cannot live inside a program that has to be built first.**

1. `cargo xtask` is an alias for `cargo run` (`.cargo/config.toml:2`: `xtask = "run --package xtask --"`). Before `xtask ci` can run anything, cargo compiles xtask and every third-party crate in its dependency tree, build scripts included.
2. That tree is not empty. `xtask/Cargo.toml` depends on `serde_json` and `ring = "0.17.14"`, and `ring` has a `cc` build script. Today `ring` is exempted in `supply-chain/config.toml`, so the gate passes. The hole matters for the next unvetted or bumped crate added to xtask.
3. Vet only needs `Cargo.lock` and `supply-chain/`, so run it in the npm line before cargo touches xtask. It also stays first inside `ci` so a direct `cargo xtask ci` still vets:
   - `package.json`: `"ci:rs": "cargo vet --locked && cargo xtask ci"`
   - `xtask/src/ci.rs` `steps()` starts with `Step::Cargo(strings(&["vet", "--locked"]))`. That step is a convenience for direct runs. It is not the gate.
4. Pin the npm line in a test, because the xtask unit test cannot see it. `test/rust-scaffold.mocha.js` asserts that `scripts['ci:rs']` equals `'cargo vet --locked && cargo xtask ci'`, and it pins the alias. The ordering test in `xtask/src/ci.rs` (`ci_runs_vet_fmt_lint_test_deny_build_mocha_then_bench`) would still pass with the gate in the wrong place, because it only compares the step list.
5. When porting a script, check where the gate sits in the execution timeline, not only in the step list. Ask what has to be compiled or run before the gate gets control.

## Why This Matters

A supply-chain gate exists so an unreviewed crate cannot run code on the CI machine, and a build script is code. If the gate needs that crate built first, the check runs after the thing it was meant to stop. The failure is silent. `npm run ci:rs` stays green while every crate is vetted, and the step-order unit test stays green too. Nothing fails until an unvetted crate is already in the tree, and by then it has run.

## When to Apply

- Porting any "check, then build" script into `cargo xtask` or any other entry point that builds before it runs. The same applies to any `cargo run`, `cargo build` or `cargo check` that compiles third-party code.
- Adding a dependency to `xtask/Cargo.toml`. It widens what would run before an in-xtask gate.
- Editing the `ci:rs` line or the `xtask` alias.
- Reviewing a port: ask for the red-gate proof below, not only a green run.

## Examples

Before the port:

```json
"ci:rs": "cargo vet --locked && node scripts/ci-rs.js"
```

Weakened port (vet only inside xtask, so cargo builds xtask and its build scripts first):

```json
"ci:rs": "cargo xtask ci"
```

Fix:

```json
"ci:rs": "cargo vet --locked && cargo xtask ci"
```

Red-gate proof, as run in this lane:

```sh
# xtask/Cargo.toml: add a scratch unvetted crate, e.g. leftpad = "0.2"; cargo fetch
npm run ci:rs; echo $?
# 255, leftpad:0.2.0 missing ["safe-to-deploy"], before any xtask compile output
# then revert xtask/Cargo.toml and Cargo.lock
```

## Secondary notes from the same port

- Node's `execFile` `timeout` has no Rust std equivalent. `output_within` in `xtask/src/host.rs` spawns the child, polls `try_wait` against a deadline, kills it when the deadline passes, and drains stderr on a thread so a chatty child cannot block on a full pipe.
- On Windows, Rust `canonicalize` returns `\\?\C:\...` verbatim paths and Node's `realpath` does not. `strip_verbatim` in `xtask/src/package.rs` strips any `\\?\` prefix. It assumes drive paths, so `\\?\UNC\...` shares are not handled.

## Related

- `docs/solutions/best-practices/cargo-vet-store-setup-and-maintenance.md`: store setup, and proving the gate goes red.
- Issue #75 (lane L12). Plan: `docs/plans/2026-10-01-spike-rs-l12-xtask-plan.md`.
