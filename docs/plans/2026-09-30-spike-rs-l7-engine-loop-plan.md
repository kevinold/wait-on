---
title: "L7: polling loop in Rust, one napi call per waitOn - Plan"
type: feat
date: 2026-09-30
topic: rust-port
spine: kevinold/wait-on#35
sub_issue: kevinold/wait-on#59
lane: L7
branch: rs-59-engine-loop
base: spike-next-rs
parent_plans:
  - docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
  - docs/plans/2026-09-28-1239-feat-rust-port-plan.md
  - docs/plans/2026-09-30-spike-rs-l5-tls-proxy-unix-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-issue-59
execution: code
---

# L7: polling loop in Rust, one napi call per waitOn - Plan

Lane plan for sub-issue #59. Requirements only: `/lfg` (via `/ce-plan` deepening and `/ce-work`) owns the technical design. Spine rules (KD-S1..KD-S9 in the spine plan) and the AGENTS.md TDD rules apply; this plan names only what L7 adds.

## Goal Capsule

- **Objective:** under `WAIT_ON_ENGINE=rust` / `rust-strict` with the addon loaded, `waitOn` hands the whole wait (every resource, its polling schedule, stabilization, concurrency, timeout, reverse, and `log`/`verbose` output) to Rust in one napi call, and that path loads neither `rxjs` nor `undici`. Callers see the same callback and Promise behavior, the same CLI exit codes and output. Users on the JS engine see no change.
- **Authority:** spine plan key decisions (KD-S1..KD-S9) > AGENTS.md > lane issue #59 > this plan. Spine lane table row L7 (R1, R5, PO16) is the proof this lane owes.
- **Stop conditions:** a needed `.github/workflows/` change (report to the PM, do not edit); any need to add to `test/rust-pending.js`; a merge of `origin/spike-next-rs` that reshapes `waitOnImpl`, `createHTTP$`/`routesHttpToRust` or the addon exports (re-thread, rerun both gates).
- **Execution profile:** `execution: code`, through `/lfg` → `/ce-work` in this worktree, strict test-first per AGENTS.md, one PR into `spike-next-rs` closing #59. The lane worker finishes and ships; the PM merges.

---

## Product Contract

### Summary

Today the Rust engine answers one check at a time inside the JS rxjs pipeline (L2-L6). L7 moves the pipeline itself: `waitOnImpl` validates options and resources in JS as now, then under a loaded addon makes a single napi call carrying the validated options and resources, and settles the caller's callback/Promise from that call's result. The JS module graph for the Rust path must not require `rxjs` or `undici`.

### Requirements

**One call**

- R-L7-1. With the addon loaded, one `waitOn` makes exactly one napi call that runs the wait (helper calls such as `version()` aside), proven by a counting addon fixture at the front door.
- R-L7-2. Callback and Promise forms are unchanged: resolve / `cb(null|undefined)` on success, reject / `cb(err)` on timeout or error, the callback fires exactly once.
- R-L7-3. Under `WAIT_ON_ENGINE=js`, or `rust` with a load failure, behavior is byte-for-byte today's JS engine; `npm test` green.

**Moved into Rust**

- R-L7-4. `delay`, `interval`, `window` (file size stabilization, and `window` raised to `interval` as today), `simultaneous`, `timeout`, `reverse` run in Rust with the JS engine's semantics, for every resource type (`file:`, `http(s)[-get]:`, `tcp:`, `socket:`, `command:`, `http://unix:`).
- R-L7-5. The timeout error message is produced in Rust and matches JS exactly: `Timed out waiting for: <remaining resources, comma-space joined>`.
- R-L7-6. `log` and `verbose` output (the `waiting for N resources: ...` lines, reverse-mode banner, `complete` / `exiting with error` lines, and the per-resource verbose lines) reach stdout with the same text as JS, except deltas already recorded in `docs/guides/architecture.md` (e.g. Rust error text in not-ready reasons). Any new delta is recorded there.

**Kept in JS**

- R-L7-7. Joi schema validation, `validateResources`, config-file loading (CLI), and `validateStatus` evaluation stay JS-side. `validateStatus` keeps crossing as a threadsafe function (L4 mechanism).
- R-L7-8. TLS material, proxy decision (`envProxyFor`, `proxyObjectUri`), header/auth building stay JS-prepared strings per resource (L5 KTD1/KTD2/KTD4), passed in the single call.

**Module graph**

- R-L7-9. The Rust path loads neither `rxjs` nor `undici`, proven by a test (e.g. a subprocess that runs a Rust-engine `waitOn` and asserts `require.cache` / module load hooks never saw either package). The JS path may keep loading both.
- R-L7-10. Cells that L5 left routed to JS (`routesHttpToRust` false: userinfo URL, https target with env-selected proxy, non-`http:` proxy URI) must either move to Rust or have a recorded resolution that keeps R-L7-9 true (see Outstanding Questions).

**Gates and docs**

- R-L7-11. `test/rust-pending.js` stays empty; every api test passes under `rust-strict`.
- R-L7-12. `npm test` and `npm run ci:rs` green on the branch; CLI subprocess tests pass under both engines.
- R-L7-13. `docs/guides/architecture.md` describes the Rust loop (replaces "Status: planned (lane L7)"), the single-call boundary, and any new deltas; `docs/guides/` updated in the same PR.

### Acceptance Examples

- AE-L7-1. **R-L7-1, R-L7-2.** Given `rust-strict` and a counting addon, two ready resources (a temp file and a local http server): `waitOn` resolves, the addon saw one loop call, and the callback form calls back once with no error.
- AE-L7-2. **R-L7-5.** Given `rust-strict`, a missing file and `timeout: 300`: `waitOn` rejects with `Timed out waiting for: <file>`; same text as `js`.
- AE-L7-3. **R-L7-9.** Given `rust-strict`, a subprocess running `waitOn` on a ready resource exits 0 and reports neither `rxjs` nor `undici` loaded; under `js` it reports both (control).
- AE-L7-4. **R-L7-6.** Given `rust-strict` and `--verbose` via the CLI on a resource that becomes ready: stdout has the same `waiting for 1 resources: ...` and `complete` lines as `js`.
- AE-L7-5. **R-L7-4.** Given `rust-strict`, `reverse: true` and a file deleted after 300 ms: `waitOn` resolves; `window` stabilization on a growing file resolves only after size is stable for `window` ms.

### Scope Boundaries

- Not here: parser differential / timing tolerance (L8), prebuild matrix (L9), release readiness (L10). No schema, `index.d.ts`, CLI flag or README option change. No new runtime npm dependency. New Rust crates only when needed and `cargo deny` / `cargo vet` clean. No `.github/workflows/` edits.
- Per-check exports from L2-L6 may stay exported if tests need them; removing them is optional cleanup.

### Outstanding Questions (each names the test that answers it)

- OQ1. How do JS-routed http cells (R-L7-10) run without loading `undici` on the Rust path? Candidates: move the cells to Rust (needs the `requestTls` fix on this branch), or fall back to the whole JS engine for a `waitOn` that contains such a resource (module graph test then scoped to Rust-routed waits). Answered by the module-graph test (AE-L7-3) plus the existing routing tests in `test/engine.mocha.js` / `test/https-proxy.mocha.js` passing on both engines.
- OQ2. Cancellation and process lifetime: when the wait settles (success, timeout), in-flight checks and pooled sockets must not keep the Node process alive. Answered by the existing process-lifetime test in `test/engine.mocha.js` running under the Rust loop.
- OQ3. Output ordering: Rust-side logging must interleave with JS `console.log` deterministically for CLI tests. Answered by AE-L7-4 under both engines.
- OQ4. Fake clock: rxjs-virtualized fake-clock tests (`test/frozen-clock.js`) cannot drive a Rust timer. Which api tests depend on it and how do they pass under `rust-strict` without a pending entry? Answered by running `test/api.mocha.js` under `rust-strict` with an empty pending list.

## Resume notes

