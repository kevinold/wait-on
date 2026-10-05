---
title: Port the Fetch bad-port fix to the JS engine and pin it in the contract - Plan
type: fix
date: 2026-10-05
artifact_contract: ce-unified-plan/v1
product_contract_source: legacy-requirements
execution: code
lane: L21
issue: 104
spine: 35
upstream: jeffbski/wait-on#260
---

# Port the Fetch bad-port fix to the JS engine and pin it in the contract - Plan

Lane L21 of spine #35 (kevinold/wait-on#104). The fix lands on `next` first
(`docs/plans/2026-10-05-fix-260-fetch-bad-ports-plan.md` on `fix/260-fetch-bad-ports`, PR to
jeffbski/wait-on base `next`); this lane ports it by hand to `lib/engine-js.js`, where the
spike split the JS engine out of `lib/wait-on.js`.

## Goal Capsule

- **Objective:** a library or CLI consumer waiting on an http server whose port is on the
  WHATWG Fetch bad-port list (6000, 6665-6669, 10080, ...) succeeds under `WAIT_ON_ENGINE=js`
  (the default) and `rust-strict`, as on every 9.x. start-server-and-test `demo-multiple`
  passes three ways in `npm run dependents`.
- **Means:** the upstream change (undici `dispatcher.request` + `interceptors.redirect`
  instead of `fetch`) applied to `httpCallSucceeds` in `lib/engine-js.js` (KTD1), pinned by
  a consumer-contract scenario (KTD2).
- **Authority:** kevinold/wait-on#104, upstream jeffbski/wait-on#260.
- **Stop conditions:** a public API / CLI / schema / `index.d.ts` change; a `crates/` or
  `.github/workflows/` edit; a new runtime dependency.
- **Allowed paths:** `lib/`, `features/`, `test/`, `docs/guides/`, `docs/plans/`,
  `docs/solutions/`, `CONCEPTS.md`, `AGENTS.md`.

## Product Contract

### Problem Frame

The spike's JS engine still calls undici `fetch`, which refuses bad-list ports before
connecting, so an http wait on :6000 times out under `js`. The Rust engine (reqwest) has no
bad-port list and already passes. The dependents harness found it (`demo-multiple`).

### Requirements

- R1. Contract: `features/api-http-options.feature` gains a `@kind:good` scenario: an HTTP
  server on a Fetch bad-list port is waited for and the wait resolves, under `js` and
  `rust-strict`, with the default route proof (engine-js under `js`, the addon under
  `rust-strict`). Red under `js` before the port, green after.
- R2. JS front door: the upstream mocha cases mirrored in `test/https-proxy.mocha.js` (HEAD,
  GET, https + `ca` + `strictSSL`, redirect onto a bad port, `followRedirect: false` to
  `validateStatus`, explicit proxy and `HTTP_PROXY` counted by the stub proxy, userinfo
  still refused) via its `outcome()` helper, so they run on both engines; CLI exit 0 in
  `test/cli.mocha.js`.
- R3. Everything R6/R7 of the upstream plan preserves stays preserved (TLS, proxies,
  socketPath, auth, headers, HEAD/GET, httpTimeout over the body, validateStatus, body
  drained, userinfo refused).
- R4. `docs/guides/testing.md` recorded run notes the fix.
- R5. Parity guards from the upstream adversarial review, on both engines: default
  `accept: */*` (and `user-agent: undici` on the JS engine), caller headers win, an endless
  redirect chain fails even when `validateStatus` accepts 3xx, a userinfo URL still fails.

### Carve-outs

- Default `user-agent` is asserted on the JS engine only: reqwest sends none, a pre-existing
  difference recorded in `docs/guides/architecture.md` ("Deliberate JS vs Rust differences").
  Changing it is a `crates/` change, outside this lane.
- `README.md` (outside allowed paths) still says the package uses `fetch`; the upstream PR
  updates it and the line arrives with the next `next` → spike merge.

### Scope Boundaries

- No Rust change: reqwest already reaches these ports, and `crates/` is outside the lane.
- The scenario is `@api`, not `@engine`: an `@engine` row would need a matching cucumber-rs
  step in `crates/wait-on-features`, outside allowed paths. The `@api` scenario still runs
  under both engines through `npm run contract`.

## Planning Contract

### Key Technical Decisions

- KTD1. Port, not merge: same diff shape as upstream (`dispatcher.request({ origin, path,
  method, headers, signal })` on the dispatcher `buildDispatcher` returns; `compose(
  interceptors.redirect({ maxRedirections: 20 }))` when `followRedirect`; body
  `arrayBuffer()` on ok else `dump()`; `STATUS_CODES` for the verbose line; userinfo throws as
  fetch did).
- KTD2. Port choice in the contract: the contract allows no skipped scenario (R2 of the
  contract plan), so instead of skipping when 6000 is taken, the step listens on the first
  free port from a list of unprivileged Fetch bad-list ports (6000, 6665-6669, 6697, 10080,
  5060, 5061, ...) and fails only if every one is taken. Mocha tests skip on `EADDRINUSE`
  like upstream.

## Implementation Units

### U1. Contract scenario + step (RED under js)

- **Files:** `features/api-http-options.feature`, `features/support/servers.js`,
  `features/support/steps-options.js`.
- **Verification:** `npm run contract` fails the new scenario under `js` only.

### U2. Mocha mirrors (RED)

- **Files:** `test/https-proxy.mocha.js`, `test/cli.mocha.js`.
- **Verification:** new cases fail under `js` with `fetch failed`; pass under `rust-strict`.

### U3. Port into `lib/engine-js.js` (GREEN)

- **Files:** `lib/engine-js.js`.
- **Verification:** U1 and U2 green.

## Verification Contract

- `npm test`, `npm run contract`, `npm run ci:rs` green (host prebuild built first).
- After GREEN, revert `lib/engine-js.js` and see the contract scenario fail under `js`.

## Definition of Done

- PR into `kevinold/wait-on` base `spike-next-rs` with `Closes #104`, linking the upstream PR;
  CI green; not merged by the worker.
- Rust-port learnings in `docs/solutions/`, `CONCEPTS.md` updated.
