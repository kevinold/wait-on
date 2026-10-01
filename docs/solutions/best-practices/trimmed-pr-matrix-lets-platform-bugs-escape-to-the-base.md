---
title: A trimmed PR CI matrix lets platform-only bugs escape to the base branch
date: 2026-10-01
category: best-practices
module: .github/workflows/node.js.yml
problem_type: best_practice
component: infrastructure
severity: high
applies_when:
  - Pull requests run fewer OS/target rows than pushes to save CI minutes
  - A lane changes tooling that spawns processes, touches paths, env vars or line endings
retire_when: "The PR matrix is restored to the full push matrix (L10 #62)"
tags: [github-actions, matrix, windows, fail-fast, crlf, env-case, xtask, spike-next-rs]
---

# A trimmed PR matrix lets platform-only bugs escape to the base branch

## Symptom

L12 (#75, `cargo xtask`) was green on its ubuntu-only PR run, then turned `spike-next-rs` red on Windows after merge, twice:

1. `failed to remove file ...\target\debug\xtask.exe — Access is denied (os error 5)`: xtask's inner cargo rebuilt the running `xtask.exe`, which Windows locks (fixed in #81 with `CARGO_TARGET_DIR=<target>/xtask-inner`).
2. A byte-identical JSON baseline came out with CRLF on Windows checkout, and a lookup of `PATH` returned `None` because Windows names it `Path`.

The ubuntu and macOS `rust` jobs showed `cancelled`, not `success`: default `fail-fast` stopped them when Windows failed, so the post-merge run verified nothing on those OSes either.

## Root cause

- The PR matrix (#77) ran `rust` only on ubuntu; Windows-only behaviour first ran after merge.
- `strategy.fail-fast` defaults to true, so one OS failure hides the others' results.

## Fix

- Keep at least one Windows `rust` row on pull requests (#83). It costs one job per PR and is cheaper than a post-merge red plus a fix cycle.
- Set `fail-fast: false` on OS matrices so every row reports its own result.
- In tooling: never rebuild the running binary (separate inner target dir); force `eol=lf` in `.gitattributes` for byte-compared fixtures; treat env keys case-insensitively on Windows.
- A post-merge red halts the queue: fix forward on a `fix-<issue>-...` branch with `Refs #<issue>`, then verify on the full matrix before the next merge.

Related: [[workflow-on-non-default-branch-never-fires]], [[cargo-xtask-alias-compiles-deps-before-the-vet-gate]].
