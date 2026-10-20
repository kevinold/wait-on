---
title: Make the -i interval contract scenario independent of the file stability window - Plan
type: fix
date: 2026-10-03
artifact_contract: ce-unified-plan/v1
product_contract_source: legacy-requirements
execution: code
lane: L20
issue: 108
spine: 35
---

# Make the -i interval contract scenario independent of the file stability window - Plan

Enriched, implementation-ready plan for sub-issue #108 (lane L20 of spine #35, fix-forward for
L17 #98 `base-regressed`). Executed by `/lfg` → `/ce-work`.

## Goal Capsule

- **Objective:** `features/cli-flags.feature` "-i sets the interval" measures the interval
  deterministically, so `npm run contract` passes on every run under both engines.
- **Means:** change the scenario's resource to one with no stability window that becomes
  ready after the first poll; add the step it needs. Contract (`features/`) only.
- **Authority:** sub-issue #108.
- **Stop conditions:** an engine (`lib/`, `crates/`) change becomes necessary; a public API /
  CLI / schema / `index.d.ts` change becomes necessary; a `.github/workflows/` edit becomes
  necessary.
- **Allowed paths:** `features/`, `test/`, `docs/guides/`, `docs/plans/`, `docs/solutions/`.

## Product Contract

Product Contract preservation: unchanged in meaning (Problem Frame, R1-R5 and their IDs, Scope
Boundaries, Done). The one evidence-driven value choice, the listen delay, is annotated at R1
and argued at KTD2.

### Problem Frame

`features/cli-flags.feature:98` waits on an *existing file* with `-i 1500 -t 5000` and expects
"it took about 1500ms". `window` (default 750) is raised to `interval`, so the file check needs
`now >= t + window` where `t` is stamped after the first async stat. At the 1500ms tick that
holds only when that tick's stat latency ≥ the first stat's latency, so elapsed is bimodal:
~1.5s or ~3.0s. PR #107 CI saw 1.5s; push run 37087466305 saw 3205ms and 3207ms on the same
tree (`ece3a34` = `e6e8368`). Not a consumer regression; the scenario measures the window, not
the interval.

### Requirements

- R1. "-i sets the interval" uses a resource with no stability window that becomes ready
  after the first poll and before the second: a TCP server that starts listening ~300ms after
  wait-on starts, with `-i 1500 -t 5000`, expecting "it took about 1500ms". (Delay value:
  KTD2 derives it from the harness tolerance; see its conflict call-out.)
- R2. A new Node `@cli` step "a TCP server that starts listening after {int}ms": reserves a
  free port up front (listen(0) / existing `freePort` helper), registers it as the scenario's
  resource, starts listening after the given delay on the real clock, and is cleaned up by the
  world's existing cleanup path. Reuse `features/support/servers.js` helpers; no new deps.
- R3. Audit every `features/*.feature` scenario whose expected elapsed sits on a
  `window == interval` file boundary and fix any others the same way. Known-safe: `-w 1500
  -i 100` (100ms ticks resolve the boundary within one tick), api-options default
  (interval 250, window 750: a lost boundary lands at 1000ms, inside tolerance), api-options
  `interval 100, window 600`. Record the audit result in the PR body.
- R4. The rewritten scenario still runs wherever it ran before (cjs fixture, `js` and
  `rust-strict` engines); cucumber-rs (`cargo test -p wait-on-features`) still parses
  `cli-flags.feature` and keeps passing.
- R5. Tolerance (`TOLERANCE_MS`) is not loosened.

### Test-first (AGENTS.md TDD)

- RED: rewrite the scenario first; `npm run contract` (cucumber `--strict`) fails with the
  undefined step "a TCP server that starts listening after <D>ms". Record the failure.
- GREEN: add the step; the scenario passes.
- Stability proof: `npm run contract` (both engines) locally ≥ 5 times, zero timing failures;
  record the observed elapsed values for this scenario in the PR body.
- Full check: `npm test` and `npm run contract` green.

### Scope Boundaries

- No engine change (`lib/`, `crates/`). The `window == interval` coin flip is pre-existing 9.x
  behaviour shared by both engines: record it under Resume notes as a follow-up candidate only.
- No public API / CLI / schema / `index.d.ts` change; no `.github/workflows/` edits.
- Docs: update `docs/guides/testing.md` only if it lists the scenario or step inventory.
- Considered and not built: a count-based HTTP server (non-2xx first, then 200), which would be
  independent of CLI startup; #108 settled the TCP delayed-listen mechanism, and KTD2's delay
  covers every startup the tolerance accepts. Revisit if CI shows startup ≥ 1000ms.

### Done

Open PR into `spike-next-rs` on kevinold/wait-on with `Closes #108`, every check green, never
merged by the lane.

## Planning Contract

- **KTD1. Mechanism: a TCP server that starts listening after a delay, `-i 1500 -t 5000`,
  expecting ~1500ms.** (session-settled: user-directed — chosen over keeping the file resource
  with `-w 100`: a TCP check has no stability window and is ready on the first successful
  connect, so a resource refused at the first poll and listening at the second isolates the
  interval.) `tcp:` polls tick first-at-delay then every interval in both engines
  (`lib/engine-js.js`, `crates/wait-on-core/src/waiter.rs`); a refused loopback connect fails
  immediately. Governs R1, R2.

- **KTD2. Delay value: 1000ms, not ~300ms.** With S = CLI startup after spawn (node start +
  module load, + addon load under `rust-strict`), the listen delay D must satisfy
  `S < D < S + 1500`. Elapsed is measured from spawn (`features/support/world.js`), and the
  "it took about" bounds are `[1400, 2500]` (`TOLERANCE_MS` in
  `test/helpers/cli-conformance.js`: early 100, late 1000), so the harness already accepts S up
  to ~1000ms. Local macOS S is ~100ms (spawn→exit 98-105ms against a listening port), so 300
  works locally, but any row with S ≥ 300ms makes the first poll succeed and elapsed ≈ S < 1400.
  D = 1000 keeps the first poll refused for every S the tolerance accepts, and the second poll
  (≥ ~1600ms) lands ≥ 500ms after listen.
  **Conflict call-out:** issue #108 settled "~300ms". The mechanism stays as settled; only the
  number differs, because 300 is workable on fast machines but not on the slow CI rows (windows,
  `rust-strict`) this fix exists for. The PR body states this derivation so the operator can
  confirm or override.

- **KTD3. No engine change, no tolerance change.** (session-settled: user-directed — chosen
  over fixing `window == interval` stat timing in the engines, and over widening
  `TOLERANCE_MS`: the coin flip is pre-existing 9.x behaviour in both engines, follow-up only;
  wider bounds would hide the bimodality rather than remove it.) Governs R5.

- **KTD4. Step lives in `features/support/steps-options.js`, server helper in
  `features/support/servers.js`.** `steps-engine.js` is the closed `@engine` vocabulary
  mirrored by `crates/wait-on-features/tests/features.rs`; a `@cli`-only step there would imply
  a Rust twin. `steps-options.js` already holds the other timed-server Given ("an HTTP server
  answering {int} after {int}ms") that `cli-flags.feature` uses.

- **KTD5. Port reservation reuses `servers.freePort()`** (listen(0), close, return the port),
  as "nothing listening on a free port" does. The small reuse race during the delay is the one
  the conformance harness already accepts.

- **KTD6. cucumber-rs needs nothing.** `features.rs` `filter_run` keeps only `@engine`
  scenarios; `cli-flags.feature` is `@cli`. It still parses every file; the rewritten scenario is
  a plain Scenario, so `<resource 1>` in its step text is allowed (`docs/guides/testing.md`).

## Implementation Units

### U1. Rewrite "-i sets the interval" on a delayed TCP server

**Goal:** the scenario's elapsed time is `startup + interval`, never a function of file stat
latency.

**Requirements:** R1, R2, R4, R5; KTD1, KTD2, KTD4, KTD5.

**Dependencies:** none.

**Files:**
- `features/cli-flags.feature` — the "-i sets the interval" scenario.
- `features/support/servers.js` — delayed TCP server helper.
- `features/support/steps-options.js` — the new Given.

**Approach:**
1. Replace `Given an existing file` with `Given a TCP server that starts listening after 1000ms`;
   keep the When/Then lines (`-i 1500 -t 5000`, exits 0, about 1500ms).
2. In `servers.js` add a delayed TCP server: reserve a port with `getFreePort`, arm a real-clock
   timer that starts the existing TCP server on that port, and return `{ port, close }` where
   `close` clears the timer and closes the server only if it started. Let `tcpServer` take an
   optional port (default 0) so listening and socket tracking stay shared.
3. In `steps-options.js` add the Given: serve the delayed server on `127.0.0.1` and register
   `tcp:127.0.0.1:<port>` with `{ port }`, mirroring "a TCP server on a free port".

**Execution note:** test-first — see the scenario fail as an undefined step under `--strict`
before adding the step.

**Patterns to follow:** "a TCP server on a free port" and "nothing listening on a free port" in
`features/support/steps-engine.js`; `slowHttpServer` + `serveHttp` for a timed server registered
through `world.serve`; `closer` in `servers.js`.

**Test scenarios:**
- RED: the rewritten scenario fails under `--strict` with the undefined step.
- GREEN: the scenario exits 0 within `[1400, 2500]` under `js` and `rust-strict`.
- Path proof: the 1400 lower bound is reachable only if the first poll was refused (a
  first-poll success exits at ≈ S < 1000), so the elapsed assertion proves the interval path ran.
- Cleanup: a scenario that fails before the delay elapses leaves no listener or timer behind and
  the After hook does not throw.
- Stability: `npm run contract` ≥ 5 local runs, zero timing failures, elapsed values recorded.

**Verification:** the scenario passes repeatedly on both engines; lint covers the support files.

## Verification Contract

- `npm run contract`: green under `js` and `rust-strict`, ≥ 5 consecutive local runs; per-run
  elapsed for "-i sets the interval" recorded in the PR body.
- `npm test` green (lint + types + mocha).
- `cargo test -p wait-on-features --test features` green: cucumber-rs still parses
  `cli-flags.feature` and runs zero `@cli` scenarios (R4).
- R3 audit, recorded in the PR body:
  - `cli-flags` "-w sets the stability window" (`-w 1500 -i 100`): safe, 100ms ticks.
  - `cli-flags` "-l", "--no-log", "-v" (`-i 100 -w 100`): `window == interval` but no elapsed
    assertion; `-t 2000` absorbs a lost boundary.
  - `cli-flags` "-d delays the first check": TCP, no window.
  - `api-options` default (interval 250, window 750 → lost boundary at 1000, inside
    `[650, 1750]`) and `interval 100, window 600`: safe.
  - `api-log-lines` (`interval 100, window 100`): no elapsed assertions.
  - `cli-basics` / `cli-config` "took about 500ms" and `cli-flags` "-t takes a unit suffix":
    timeout scenarios, not file windows.
  - Result: only "-i sets the interval" sat on `window == interval` with an elapsed assertion.
- CI: PR matrix runs the contract on ubuntu + windows, push adds macos
  (`docs/solutions/best-practices/trimmed-pr-matrix-lets-platform-bugs-escape-to-the-base.md`).
- `docs/guides/testing.md` lists feature files and tags, not steps: no update required.

## Definition of Done

- RED recorded: the rewritten scenario fails under `--strict` with the undefined step.
- Only `features/cli-flags.feature`, `features/support/servers.js`,
  `features/support/steps-options.js` and docs change; nothing under `lib/`, `crates/`,
  `.github/workflows/`, `index.d.ts`, `README.md`.
- `npm run contract` green on both engines, ≥ 5 local runs, elapsed values in the PR body.
- `npm test` and `cargo test -p wait-on-features --test features` green.
- R3 audit and the KTD2 delay derivation in the PR body.
- Conventional Commit messages; test and code in the same commit; this plan committed.
- PR into `spike-next-rs` on kevinold/wait-on with `Closes #108`; not merged by the lane.

## Resume notes

- Follow-up candidate (not this lane): with `window == interval`, file readiness at the first
  window tick depends on relative stat latency of the first and later polls, so elapsed is
  ~interval or ~2×interval in both engines. Pre-existing 9.x behaviour; a fix would stamp `t`
  at poll start or compare against tick count rather than wall time.
- 2026-10-03 (L20 run): RED was the undefined step under `--strict`; GREEN on every fixture and
  engine. 5 local `npm run contract` runs (macOS arm64): js 1620-1637ms, rust-strict
  1578-1602ms for "-i sets the interval", zero failures. The delay is 1000ms, not the issue's
  ~300ms (KTD2).
- Open review residual (cross-model adversarial read): if CLI startup reached 1400-2500ms, the
  first poll would connect and still land inside the tolerance. A verbose-output assertion
  could prove the refused-then-connected path, but detail lines differ between engines, so it
  was not added here. Revisit if CI shows startup above ~1000ms.
