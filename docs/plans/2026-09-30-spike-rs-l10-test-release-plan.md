---
title: "[L10] test-release readiness and supply-chain delta - Plan"
type: docs
date: 2026-10-01
topic: rust-port
lane: L10
kind: preview
branch: rs-62-test-release
closes: kevinold/wait-on#62
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
requirements_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L10] test-release readiness and supply-chain delta - Plan

Requirements-only lane plan for sub-issue #62, authored from the issue body, its PM comment, and
the PM addendum of 2026-10-01 (spine plan "Operator decisions (2026-10-01)", KD-S10..KD-S15).
Base `832c588` (`spike-next-rs`). The spine plan is controlling and is never edited by this lane.
L10 is the last lane: it runs after L11, L12, L13 have merged.

---

## Goal Capsule

**Objective.** When this lane merges, a maintainer can show from the repo alone that the latest
fork GitHub prerelease tarball installs with scripts disabled and runs the Rust engine end to end
(AE1, AE2). They can also show the supply-chain win in numbers against `joi`/`rxjs`/`undici`, see
that the Rust pending list is empty, and follow `docs/guides/` to build, test, and ship both
engines with `cargo xtask`. The spike PR #51 can then leave draft once the operator restores the
full PR CI matrix.

**Direction (operator, 2026-10-01).** Rust is primary and JS is what gets sunset (KD-S10). New
automation is a `cargo xtask` subcommand with Rust tests or a dependency-free POSIX sh script.
No JS tooling, JS devDependencies, npm lifecycle scripts, Justfile, or Claude Code hooks. Rust
crates stay at 100% line and region coverage (KD-S12).

**Authority.** `AGENTS.md` > spine plan (KD-S1..KD-S15) > this plan.

**Stop conditions** (report to the PM, do not work around):
- a `.github/workflows/` change is needed. Restoring the full PR matrix (#77 trim) is an operator
  PR and is listed in the lane PR body as the remaining step, not done here;
- a public API, CLI, `WAIT_ON_SCHEMA`, or `index.d.ts` change is needed;
- a new npm runtime dependency is needed;
- the fork prerelease fails AE1 or AE2 for a reason inside the engine (that is an engine lane bug:
  report it with the reproduction rather than patching outside this lane's scope).

**Execution profile.** `/lfg` → `/ce-work` from this plan; strict TDD per `AGENTS.md`; one PR to
`kevinold/wait-on`, base `spike-next-rs`, body `Closes #62`; never merges. Allowed paths:
`crates/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `lib/`, `bin/`, `test/`,
`index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`, `.gitignore`, `.nycrc.json`,
`eslint.config.mjs`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `AGENTS.md`, `README.md`,
`benchmarks/`, `scripts/`. **xtask/ is not in the list.** Treat any `xtask/` change as an
allowed-paths question for the PM before making it. Prefer reusing existing subcommands
(`cargo xtask package` already runs AE1 on a local tarball) with arguments over new code.

---

## Product Contract

### Summary

Verify the newest `rs-<version>-<sha7>` prerelease on `kevinold/wait-on` end to end. Download it
from the release URL, check `SHA256SUMS`, install it with scripts disabled, and run AE1 and AE2
with `WAIT_ON_ENGINE=rust`. Record the supply-chain delta and the Rust-side crate and cargo-vet
exemption counts. Confirm the pending list is empty. Finish `docs/guides/releasing.md` and the
guides index so a new developer can build, test, and ship both engines with `cargo xtask`.

### Starting state (verified 2026-10-01, base `832c588`)

- Latest fork prerelease: `rs-10.0.0-rc.1-832c588` (the base commit), with assets `wait-on-*.tgz`
  and `SHA256SUMS` per `docs/guides/releasing.md`.
- `cargo xtask` subcommands include `ci`, `cov`, `build-napi`, `package`, `bench-startup`, and
  `hooks`. `cargo xtask package` packs a local tarball, installs it with `--ignore-scripts`,
  `--omit=optional`, and pnpm, and runs AE1 in read-only, no-network glibc and musl containers
  (skipped without docker outside CI).
- `package.json` runtime `dependencies`: `joi`, `rxjs`, `undici`. The Rust path (L7) loads
  neither `rxjs` nor `undici`, but all three are still declared because the JS engine is the
  default.
- `docs/guides/releasing.md` "End-to-end prerelease verification" says `Status: planned (lane L10)`.
- JS-to-Rust tooling inventory: issue #85 (open).
- PR CI runs a trimmed matrix (KD-S14, #77/#83). Pushes run the full matrix and the prerelease.

### Requirements

- **R-L10-1 Release-URL verification.** The latest fork prerelease tarball is fetched from its
  GitHub Release URL, its SHA-256 matches `SHA256SUMS`, and it installs into a clean temp project
  with scripts disabled (`--ignore-scripts`). The verified tag, sha, and date are recorded.
- **R-L10-2 AE1 from the release.** The installed tarball runs `wait-on tcp:<port>` under
  `WAIT_ON_ENGINE=rust-strict` (so a silent JS fallback fails) in a read-only, no-network
  container. It waits and exits 0, with no install-time script execution.
- **R-L10-3 AE2 from the release.** Against the installed tarball, under `WAIT_ON_ENGINE=rust-strict`,
  `waitOn({ resources: ['https-get://…'], validateStatus: s => s === 200 })` passes on a 200 and
  does not pass on a 204 (times out or rejects). The test proves the JS predicate was called (the
  path ran), not only the outcome.
- **R-L10-4 Engine actually loaded.** Each verification asserts the Rust addon loaded from inside
  the installed package (not the repo checkout and not the JS fallback).
- **R-L10-5 Supply-chain delta recorded.** Record the transitive runtime-dependency count and
  lifecycle-script count for: the `next` baseline (`joi`, `rxjs`, `undici`), and the Rust engine
  path as shipped today (fallback window, all three still declared), and the projected post-cutover
  state (`joi` at most). Record it against the Success Criteria in the requirements plan. Gains
  from the earlier axios, lodash, and minimist removal do not count.
- **R-L10-6 Rust-side counts recorded.** Record the Rust crate count in `Cargo.lock` (the
  runtime crates of `wait-on-core` and `wait-on-napi` separately from dev and xtask crates), and the
  `supply-chain/config.toml` cargo-vet exemption and import counts.
- **R-L10-7 Pending list empty.** Confirm and record that every API, CLI, conformance, and
  property test passes under `WAIT_ON_ENGINE=rust-strict` with no pending or skip list (KD-S12).
- **R-L10-8 Releasing guide finished.** `docs/guides/releasing.md` replaces "Status: planned" with
  a working runbook: the commands for R-L10-1..R-L10-4, the recorded results, the supply-chain
  delta table, a link to #85 (the JS-to-Rust tooling inventory), and a list of what the cutover
  still needs (KD-S11 release tooling evaluation, JS fallback removal, default engine flip, npm
  trusted publisher).
- **R-L10-9 Guides index finished.** `docs/guides/README.md` and the pages it lists let a new
  developer build, test, and ship both engines through `cargo xtask` as the front door. No stale
  references to removed JS scripts or a Justfile. The releasing row in the index reflects the new
  content.
- **R-L10-10 Draft-exit readiness.** The PR body lists the readiness checklist for #51: the items
  above, plus the remaining operator step to restore the full PR matrix (build 2×3, rust 3 OS,
  napi 8 targets, package on PRs) in `.github/workflows/node.js.yml`.
- **R-L10-11 Knowledge lands in repo.** Non-obvious learnings go to `docs/solutions/` via
  `/ce-compound mode:non-interactive` or to Resume notes below, in this PR.

### Named risks → tests (AGENTS.md: a prose check does not count)

- **T-L10-1 Silent JS fallback hides a broken addon.** If automation is added, its Rust test asserts
  it sets `WAIT_ON_ENGINE=rust-strict` and checks the addon load path. Manual runs use `rust-strict`.
- **T-L10-2 Checksum mismatch accepted.** If automation verifies `SHA256SUMS`, a Rust unit test
  feeds a tampered digest and expects failure.
- **T-L10-3 AE2 passes through a fallback.** The AE2 check counts predicate calls (≥1 with 200 and
  with 204) and asserts the 204 run does not succeed.
- **T-L10-4 Delta numbers drift from reality.** If the counts are computed by automation, a Rust
  test checks the counting against a small fixture lockfile with concrete expected numbers.

If the lane ends up docs-only (verification done by running existing `cargo xtask package` steps
against the downloaded tarball and recording results), T-L10-1..T-L10-4 are met by the recorded
command output in the PR body, and no new executable code lands.

### Scope boundaries

- Not here: workflow edits, npm publish, the default-engine flip, JS fallback removal, the
  standalone binary, release-tool replacement (KD-S11).

---

## Resume notes

(append-only; lane notes go here)
