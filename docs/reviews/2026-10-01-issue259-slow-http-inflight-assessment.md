# Issue #259 — slow HTTP(S) responses vs. polling interval: assessment

**Issue:** jeffbski/wait-on#259 — does a later polling tick abandon a slow in-flight request?
**Reviewed release:** `next` @ 10.0.0-rc.1 (undici `fetch`), compared with v9.5.1 (axios ^1.20)
**Branch:** `test/slow-http-inflight` (CI-trigger-only fork draft PR kevinold/wait-on#95)
**Date:** 2026-10-01
**Method:** 9 characterization tests (`test/slow-response.mocha.js`), each checked to make sure it can fail by applying local mutations to `lib/wait-on.js`, then the same file run against v9.5.1 and on the fork's CI matrix. Plan: `docs/plans/2026-10-01-1216-test-slow-http-inflight-plan.md`. Results map posted on the issue.

## Verdict

**No defect. The concern does not reproduce on either implementation.** When a request is still in flight, the next polling tick does not cancel it, so wait-on succeeds against a server that is always slower than `interval`. This holds on `next` (undici) and v9.5.1 (axios), on ubuntu and windows, on Node 22, 24, and 26. No `lib/` change was needed, so no fix PR was opened.

Why it holds: `createHTTP$` schedules each check with `mergeMap(…, simultaneous)` (`lib/wait-on.js`). With `mergeMap`, a new tick starts a new request but does not cancel the ones already in flight. An in-flight request is aborted in only two cases:

- its own `httpTimeout` expires (`AbortSignal.timeout`), or
- the resource's `teardown` AbortController fires once polling stops.

v9.5.1 has the same `mergeMap` shape, and its axios call is never cancelled by a later tick.

## How the tests prove it

- The server delays every response by **400 ms**, while wait-on polls every **100 ms**.
- It returns success **only to request #1** and 503 to every later request. A pass can therefore only come from request #1 surviving the 3+ ticks that fire while it is in flight.
- In reverse mode the statuses flip: 503 to request #1, then 200 to every later request.
- The server records whether each request was **aborted by the client** before it responded, and the **peak number of concurrent requests**.
- Success tests assert three things: wait-on succeeded, request #1 was not aborted, and the requests overlapped (exactly 1 in flight for `simultaneous: 1`).

## Results matrix

| # | Concern | `next` (undici) | v9.5.1 (axios) | Proven able to fail by |
|---|---|---|---|---|
| 1 | `http:` HEAD, default `simultaneous` | pass | pass | M-A, M-C, M-E |
| 2 | `http-get:` GET, slow body | pass | pass | M-A, M-C, M-E |
| 3 | `https:` | pass | pass | M-A, M-C, M-E |
| 4 | `simultaneous: 1` | pass | pass | M-A, M-D |
| 5 | `httpTimeout` > delay | pass | pass | M-A, M-C, M-E |
| 6 | `httpTimeout` < delay (control: rejects, request #1 aborted) | pass | pass | M-B |
| 7 | `http://unix:` socket / Windows named pipe | pass | pass | M-A, M-C, M-E |
| 8 | `reverse: true` | pass | pass | M-A, M-C, M-E (abort assertion) |
| 9 | CLI `wait-on -i 100 -t 3000` (exit 0) | pass | pass | M-A, M-C, M-E |

| Environment | Result |
|---|---|
| macOS, Node 26.3.1, `next` | 9/9; full `npm test` 283 passing |
| macOS, Node 26.3.1, v9.5.1, identical file | 9/9 |
| CI ubuntu-latest, Node 22 / 24 / 26 | pass (kevinold/wait-on#95) |
| CI windows-latest, Node 22 / 24 / 26 | pass; all 9 ran, including the named pipe and https tests |

## Mutation checks (local only, never committed)

| ID | Mutation in `lib/wait-on.js` | What it turns red |
|---|---|---|
| M-A | `mergeMap` → `switchMap` (new tick cancels in-flight request) | Every success test, which times out; the CLI exits 1 |
| M-B | drop `AbortSignal.timeout(httpTimeout)` | Test 6 resolves instead of rejecting |
| M-C | `mergeMap` → `exhaustMap` (ticks dropped while one request is in flight) | Overlap assertion (`maxConcurrent >= 2`) |
| M-D | ignore `simultaneous` | Test 4 (`maxConcurrent === 1`; observed 4) |
| M-E | abort the previous request each tick, still report its result | Forward tests time out; reverse test 8 *resolves* but fails `requests[0].aborted === false` |

## Findings

- **No product defect.** Every concern raised in #259 has a passing test that has been shown it can fail.
- **Reverse mode needs the abort assertion.** If a future implementation aborts the previous request but still reports its result, reverse mode would treat that abort as success, so "resolves" alone cannot catch it. Only the request-#1-not-aborted assertion does. This matters for a future Rust port. How to check this kind of test can fail is in `docs/solutions/`.
- **CI does not run on feature-branch pushes.** `.github/workflows/node.js.yml` triggers only on pushes to `master`, `next`, and `*.x`, or on a `pull_request`. Windows coverage for this work came from a fork-only draft PR.

## Not covered / residual

- `tcp:` / `socket:` resources: they only check that a connection opens, so they have no slow-response dimension. They use the same `mergeMap(…, simultaneous)` shape.
- **Proxy environment:** with `HTTP_PROXY`/`HTTPS_PROXY` set and no `NO_PROXY`, the env proxy dispatcher sends localhost through the proxy, and these tests time out. Every existing localhost test in `test/api.mocha.js` has the same exposure.

## Reviews

- **Plan review:** coherence, feasibility, scope-guardian, and adversarial reviewers, plus a codex cross-model pass. 3 fixes were applied to the plan before implementation.
- **Code review (`ce-code-review`, focused):** verdict "Ready with fixes". Two fixes were applied: openssl paths are now quoted, and the CLI test fails if the process is killed by a signal.
