---
title: Port the upstream redirect-cap and body-destroy fixes - Plan
type: fix
date: 2026-10-17
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
issue: 111
upstream: jeffbski/wait-on#262
---

# Port the upstream redirect-cap and body-destroy fixes - Plan

kevinold/wait-on#111. Spike PR #110 ported the first version of the upstream bad-port fix (jeffbski/wait-on#262) into `lib/engine-js.js`. Review of #262 found two bugs in that version and fixed them upstream test-first (a7fd55f, 45a1d58), plus a guard test (89fe79e) and a docs learning (e98d4c7). This lane ports all four by hand to the spike, and replaces the mocha bad-port tests' 6000-or-skip setup with the contract's first-free-bad-port pattern.

## Goal Capsule

- Objective: a wait-on consumer on the JS engine (the default) gets fetch's behaviour back on two boundaries: a 2xx reached after exactly 20 redirects resolves instead of failing with `max redirects`, and a non-2xx GET whose body never ends does not hold the poll slot until the overall timeout. Both engines pass the same tests. The mocha bad-port tests always run (no silent skip when XQuartz holds 6000 on macOS).
- Means: the upstream diffs applied to `lib/engine-js.js` (KTD1), the three upstream tests placed in the bad-port describe so they run through `outcome()` under both engines (KTD2), no Rust production change plus one Rust guard test for the exact-20 boundary (KTD3), a shared bad-port listener in `test/helpers/` that `features/support/servers.js` and both mocha suites import (KTD4), no new contract scenario (KTD5), the e98d4c7 learning ported by `ce-compound` in the shipping tail (KTD6).
- Authority: kevinold/wait-on#111 (user-directed decisions below) > upstream commits a7fd55f, 45a1d58, 89fe79e, e98d4c7 > `AGENTS.md` > this plan.
- Stop conditions: a public API, CLI, schema or `index.d.ts` change; a `.github/workflows/` edit; a new runtime dependency; a Rust production-code change outside `#[cfg(test)]` (would mean the reqwest facts in KTD3 are wrong: stop and report rather than patch); a test that only passes with `skip()`.
- Execution profile: `ce-work` via `lfg`, branch `fix/rs-111-upstream-262-fixes` off `spike-next-rs`, PR into `spike-next-rs` on kevinold/wait-on (never upstream), Conventional Commit messages per `AGENTS.md`, `ce-compound mode:non-interactive` in the shipping tail.

---

## Product Contract

### Summary

Port two upstream JS-engine fixes and one guard test from jeffbski/wait-on#262 into `lib/engine-js.js` and `test/https-proxy.mocha.js`, test-first and passing under `js` and `rust`. Replace the hardcoded-6000-with-skip bad-port setup in `test/https-proxy.mocha.js` and `test/cli.mocha.js` with the first-free Fetch bad-list port pattern already in `features/support/servers.js`, moved to a shared helper so the list exists once.

### Problem Frame

- `lib/engine-js.js` composes `interceptors.redirect({ maxRedirections: 20, throwOnMaxRedirect: true })`. undici counts the final response against `maxRedirections`, so a 2xx after exactly 20 redirects throws `max redirects` and the probe reports not ready; fetch followed 20 and failed only on a 21st.
- `httpCallSucceeds` ends with `if (!consumed) await body.dump().catch(() => {})`. `dump()` waits for the body to end, so a non-2xx response whose body never ends holds the poll (with `simultaneous: 1` and no `httpTimeout`, every later poll) until the wait's own timeout. fetch's `body.cancel()` dropped it at once.
- Fetch stripped `Authorization` when a redirect left the origin; the probe now relies on undici's redirect interceptor for that and nothing pins it.
- The mocha bad-port tests listen on 6000 and `skip()` on `EADDRINUSE`/`EACCES`. XQuartz holds 6000 on macOS, so locally those seven tests plus the CLI one skip silently. The contract already solves this with a first-free port over `FETCH_BAD_PORTS` and fails only when every port is taken.
- The Rust engine (`crates/wait-on-core/src/http.rs`, reqwest 0.13) already behaves: `Policy::limited(20)` errors only when `previous.len() > 20`, so exactly 20 redirects follow; a non-2xx response is dropped without reading the body; reqwest strips `Authorization` when host, port or scheme changes.

### Requirements

Redirect cap
- R1. `waitOn` on the JS engine resolves for an `http:` resource whose server answers 302 twenty times and then 200; it still rejects (`Timed out`) on an endless chain (existing test `should fail an endless redirect chain even when validateStatus accepts 3xx` stays green).

Rejected body
- R2. `waitOn` on the JS engine with `simultaneous: 1` and no `httpTimeout` resolves within the wait's timeout when an `http-get:` server first answers 503 with a body it never ends and later answers 200; the 503 body is destroyed, not awaited, and the `RequestAbortedError` destroy emits is swallowed.

Auth on redirect
- R3. With `opts.auth`, the first server sees the Basic header and a different-origin server reached by a 302 sees no `Authorization` header, on both engines.

Both engines
- R4. R1 to R3 pass under `WAIT_ON_ENGINE=js` (default) and `WAIT_ON_ENGINE=rust`/`rust-strict` with the host prebuild, with the routing proof `outcome()` gives.

Bad-port tests always run
- R5. One list of unprivileged Fetch bad-list ports and one listen-on-first-free helper, in `test/helpers/`, used by `features/support/servers.js` (`badPortHttpServer`), the `Fetch bad-list ports (#104)` describe in `test/https-proxy.mocha.js`, and the CLI bad-port test in `test/cli.mocha.js`. The helper accepts a prebuilt http or https server and a host, retries the next port on `EADDRINUSE`/`EACCES`, rethrows anything else, and throws when every port is taken.
- R6. No port `skip()` remains in the bad-port tests. The openssl skip for the https bad-port case stays (tool-missing skip, allowed by `AGENTS.md`).
- R7. The helper has a self-check in the existing `test helpers` describe: with the first free bad-list port held, a second call lands on a later port from the list.

Docs
- R8. The e98d4c7 learning lands in the spike's `docs/solutions/` and `CONCEPTS.md`, in the same PR.

### Key Decisions

- KD1. Port upstream commits a7fd55f, 45a1d58, 89fe79e (modulo the `lib/wait-on.js` to `lib/engine-js.js` move and the describe placement) (session-settled: user-directed — chosen over rewriting the fixes locally: upstream already reviewed and verified them test-first). Governs R1, R2, R3.
- KD2. Mocha bad-port tests use the first-free Fetch bad-list port pattern from `features/support/servers.js`, reused through one shared helper, with no skip (session-settled: user-directed — chosen over hardcoded 6000 plus `skip()`: XQuartz holds 6000 on macOS so the tests skipped silently). Governs R5, R6, R7.
- KD3. The new tests must pass under both engines; any Rust fix would be test-first in `crates/wait-on-core` (session-settled: user-directed — chosen over JS-only: both engines must pass the same mocha suites). Governs R4.
- KD4. Gates are `npm test`, `npm run ci:rs`, `npm run contract` (js and rust-strict), `cargo test -p wait-on-features --test features`, with no skipped scenario (session-settled: user-directed — chosen over `npm test` only: the Rust engine and the consumer contract are part of the spike's definition of green). Governs the Verification Contract.

### Scope Boundaries

- Non-goals considered and not built: a Rust-side auth-strip or body unit test (KTD3 carve-outs); a new contract scenario for either boundary (KTD5); CLI, schema, `README.md` or `bin/usage.txt` changes; `.github/workflows/`; `httpTimeout` semantics.
- The `https/tls and proxy parity` describe is left alone: it skips wholesale without openssl and does not use `outcome()`, so the ported tests go in the bad-port describe instead (KTD2).

### Assumptions

- A1. Re-calling `server.listen()` on the same Node server after an `EADDRINUSE`/`EACCES` error works. The helper self-check (R7) proves it; if not, the helper takes a server factory instead.
- A2. Under `WAIT_ON_ENGINE=rust*`, `outcome()` loads the counting addon which delegates to the host prebuild, so a local Rust run needs `npm run build:napi` first.
- A3. Two `localhost` servers differing by port are cross-origin for both undici and reqwest.

---

## Planning Contract

### Key Technical Decisions

- KTD1. Same diff shape as upstream in `lib/engine-js.js`: `maxRedirections: 21` with a one-line comment that undici counts the final response; `if (!consumed) body.on('error', () => {}).destroy();` in the `finally`, with upstream's comment on why not `dump()`.
- KTD2. The three ported tests live in the `Fetch bad-list ports (#104)` describe of `test/https-proxy.mocha.js`, written through `outcome()` and `httpTarget()` so each run proves which engine answered and the suite does not skip without openssl. `FAST` has no `httpTimeout`, which the body test relies on.
- KTD3. No Rust code change. Per `AGENTS.md` (engine behaviour gets a Rust test first), add one Rust guard test in `crates/wait-on-core/src/http.rs` pinning the lower redirect boundary: a 200 after exactly 20 redirects is ready, next to `redirect_chain_over_20_hops_is_not_ready`. Carve-outs: the non-2xx body (Rust never reads it; nothing to pin) and the cross-origin auth strip (reqwest's own behaviour, pinned at the front door by the ported mocha test under `rust-strict`). A Rust guard that fails is a stop condition.
- KTD4. The shared helper lives in `test/helpers/cli-conformance.js` next to `getFreePort` and `listening`, which `features/support/servers.js` already imports. `FETCH_BAD_PORTS` moves there and `servers.js` drops its copy. Shape: `listenOnBadPort(server, host = '127.0.0.1')` resolves the listening server, looping `listening(server, port, host)` over the list.
- KTD5. No new `features/*.feature` scenario. Both fixes restore behaviour inside already-contracted promises (`a redirect is followed by default`, `a server on a Fetch bad-list port is reached`); these undici boundaries are pinned by mocha under both engines. The body-stall case would need step vocabulary the contract lacks.
- KTD6. The e98d4c7 learning is written by `ce-compound mode:non-interactive` in the `lfg` shipping tail: a `docs/solutions/` doc (the upstream path does not exist on the spike) carrying the redirect-cap and body-destroy gotchas and the first-free-bad-port prevention note, plus a `CONCEPTS.md` update of `HTTP probe` and `Fetch bad-port list`.

---

## Implementation Units

### U1. Shared first-free bad-port listener, used by contract and mocha

- **Goal:** one list, one helper, no port skip anywhere.
- **Requirements:** R5, R6, R7, KD2.
- **Dependencies:** none.
- **Files:** `test/helpers/cli-conformance.js`, `features/support/servers.js`, `test/https-proxy.mocha.js`, `test/cli.mocha.js`.
- **Approach:**
  1. Add `FETCH_BAD_PORTS` and `listenOnBadPort` to `cli-conformance.js` (KTD4) and export both.
  2. `badPortHttpServer` uses it; delete the list from `servers.js`.
  3. In `https-proxy.mocha.js` replace `BAD_PORT`, `listenBadPort(ctx, ...)` and the fixed `badUrl` with the helper and a port read from the listening server.
  4. In `cli.mocha.js` replace the inline listen-or-skip with the helper; build the URL from the listening port.
- **Execution note:** test-first. Write the self-check against a stub `listenOnBadPort` that always listens on the first list port; RED is the assertion that the second server's port differs (or an unretried `EADDRINUSE`), not a missing-export `TypeError`. Then implement the loop.
- **Patterns to follow:** `badPortHttpServer` loop in `features/support/servers.js`; `listening()` in `cli-conformance.js`; the `test helpers: tls fixture and stub proxy` describe.
- **Test scenarios:**
  - should listen on a later bad-list port when the first free one is held: helper for server A, then B; B's port is in `FETCH_BAD_PORTS` and differs from A's.
  - should rethrow a listen error that is neither EADDRINUSE nor EACCES (e.g. an unassignable host such as `192.0.2.1` gives `EADDRNOTAVAIL`).
  - existing bad-port tests and the CLI exit-0 test keep passing with a port from the helper.
  - Carve-out: the every-port-taken throw is not tested (holding all 14 ports is slow and collides with parallel lanes); it is a single `throw` after the loop, already covered by the contract helper it replaces.
- **Verification:** no pending tests in the bad-port describes; `npm run contract` green under `js` and `rust-strict`.

### U2. Exactly 20 redirects resolve (`maxRedirections: 21`)

- **Goal:** R1.
- **Requirements:** R1, R4, KD1.
- **Dependencies:** none.
- **Files:** `crates/wait-on-core/src/http.rs`, `test/https-proxy.mocha.js`, `lib/engine-js.js`.
- **Approach:** first the Rust guard test (KTD3): 20 scripted 302 replies then a 200, `checker(..., "GET", true, ...)` is ok. Then port 45a1d58. An `httpTarget` reading the hop from `req.url`, 200 at hop 20, 302 to `/${hop + 1}` otherwise; `outcome` resolves. Fix per KTD1.
- **Execution note:** RED under `js` with `Timed out waiting for`. Under `rust` passes first (expected engine difference). After GREEN, revert the fix and watch it fail again.
- **Patterns to follow:** `should fail an endless redirect chain even when validateStatus accepts 3xx`.
- **Test scenarios:**
  - should succeed when the final 2xx comes after exactly 20 redirects.
  - existing endless-chain test stays green.
- **Verification:** green under `js` and `WAIT_ON_ENGINE=rust`.

### U3. A never-ending non-2xx body does not stall the poll (`body.destroy()`)

- **Goal:** R2.
- **Requirements:** R2, R4, KD1.
- **Dependencies:** none.
- **Files:** `test/https-proxy.mocha.js`, `lib/engine-js.js`.
- **Approach:** port a7fd55f. Handler answers 503 and writes one chunk without ending until 500 ms after start (real clock: the resource changes on real time), then `res.end('ok')`; `outcome({ resources: [http-get url], simultaneous: 1 })` resolves. Fix per KTD1.
- **Execution note:** RED under `js` with `Timed out waiting for`. After GREEN, revert and confirm the timeout returns; no unhandled `RequestAbortedError`.
- **Patterns to follow:** upstream a7fd55f test body.
- **Test scenarios:**
  - should succeed when an earlier non-2xx GET response never ends its body.
- **Verification:** green under both engines, well under 2000 ms.

### U4. Guard: `opts.auth` is not sent to a different origin after a redirect

- **Goal:** R3.
- **Requirements:** R3, R4, KD1.
- **Dependencies:** none.
- **Files:** `test/https-proxy.mocha.js`.
- **Approach:** port 89fe79e into the bad-port describe using `httpTarget` and `outcome`.
- **Execution note:** a guard, expected to pass on first run. Show RED under `js` by temporarily disabling undici's redirect-handler header strip in `node_modules` (upstream verified red the same way), then restore it (never commit `node_modules`).
- **Test scenarios:**
  - should not send opts.auth to a different origin after a redirect.
- **Verification:** green under both engines.

---

## Verification Contract

| Check | Command | Expected |
|---|---|---|
| Lint, types, mocha under js | `npm test` | green |
| Mocha under the Rust engine | `npm run build:napi` then `WAIT_ON_ENGINE=rust npm run test:mocha` | green |
| Rust CI | `npm run ci:rs` | green |
| Consumer contract | `npm run contract` | green under `js` and `rust-strict`, zero skipped scenarios |
| cucumber-rs | `cargo test -p wait-on-features --test features` | green |
| No port skip | `grep -n "skip(" test/https-proxy.mocha.js test/cli.mocha.js` | only openssl and Windows-env skips |
| RED proofs | per unit Execution notes | U2, U3 red before fix and on revert; U4 red with the strip disabled |

## Definition of Done

- U1 to U4 landed test-first with Conventional Commit headers (lowercase subjects, no issue number or lane id at the start).
- Every Verification Contract row green locally; CI green on the PR.
- `ce-compound` doc and `CONCEPTS.md` update committed in the same PR (KTD6, R8).
- PR into kevinold/wait-on base `spike-next-rs`, not merged by the worker.
- No `.only`, `.skip` or disabled test in the diff; no `node_modules` change; abandoned-attempt code removed; this plan committed.
