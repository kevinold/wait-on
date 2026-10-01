---
title: Proving wait-on HTTP polling characterization tests can fail via local lib mutations
date: 2026-10-01
category: best-practices
module: lib/wait-on.js createHTTP$ / test/slow-response.mocha.js
problem_type: best_practice
component: testing_framework
severity: medium
applies_when:
  - Writing characterization tests for the http(s) polling pipeline (createHTTP$)
  - A test must show that an in-flight request survives later polling ticks
  - Mutating mergeMap to switchMap or exhaustMap locally to prove a test can go red
  - Asserting server-side client aborts or covering reverse-mode cells
tags: [mutation-testing, characterization-tests, rxjs, mergemap, reverse-mode, abort, slow-http]
related_components: [development_workflow]
---

# Proving wait-on HTTP polling characterization tests can fail via local lib mutations

## Context

Issue jeffbski/wait-on#259 asked whether an http(s) resource that answers slower than `interval` still succeeds. For that to happen, a later polling tick must not cancel the request already in flight.

- **Tests:** `test/slow-response.mocha.js`, on branch `test/slow-http-inflight`. Fork draft PR kevinold/wait-on#95 exists only to trigger CI and is unmerged as of this writing.
- **Results:** all 9 tests pass on `next` (undici) and on v9.5.1 (axios).

A test that passes on both implementations is only worth something if it goes red when the behavior breaks. So each test was run against local, uncommitted mutations of `lib/wait-on.js`, and every test went red under at least one of them. The full mutation table is in the #259 results comment and in `docs/reviews/2026-10-01-issue259-slow-http-inflight-assessment.md`.

How the code under test behaves:

- `createHTTP$` polls with `timer(delay, interval).pipe(mergeMap(..., simultaneous))`.
- `httpCallSucceeds` combines the teardown signal with `AbortSignal.timeout(httpTimeout)` when `httpTimeout` is set, and returns `false` on any error.
- Reverse mode wraps `httpCallSucceeds` in `negateAsync`, so a `false` becomes success.

## Guidance

**Mechanics.**
1. Back up `lib/wait-on.js`.
2. Apply the mutation with sed or perl.
3. Run `npx mocha --exit test/slow-response.mocha.js`.
4. Restore the file byte for byte and confirm `git diff --exit-code lib/` is clean.

Never commit a mutation. Read each failure before counting it as red.

**Design the server so "resolved" proves survival.**

- `slowServer` delays every response by 400 ms (polling interval 100 ms).
- It answers `first` to request #1 and `later` to every other request. The default is 200 then 503; the reverse test uses 503 then 200.
- In forward tests only request #1 can succeed, so resolving at all means it survived later ticks.
- The server also records a per-request `aborted` flag (`res.on('close')` plus `!res.writableFinished`) and the peak number of concurrent requests.

**Map each assertion to the mutation that proves it can fail:**

| Mutation | Change | Proves live |
|---|---|---|
| M-A | `mergeMap` -> `switchMap` (no 2nd arg) | "resolves" in every forward test (they time out) |
| M-B | drop `AbortSignal.timeout(httpTimeout)` | the httpTimeout control test (it resolves instead of rejecting) |
| M-C | `mergeMap` -> `exhaustMap` (no 2nd arg) | the overlap assertion `maxConcurrent() >= 2` |
| M-D | force `simultaneous` to `Infinity` | the `simultaneous: 1` test's `maxConcurrent() === 1` |
| M-E | keep `mergeMap`, abort the previous request each tick, still deliver its result | `requests[0].aborted === false`, the only assertion that catches this regression in reverse mode |

### Traps

1. **rxjs 7 `switchMap` and `exhaustMap` take a deprecated second argument, `resultSelector`.**
   - What happens: a naive swap from `mergeMap(fn, simultaneous)` to `switchMap(fn, simultaneous)` passes `simultaneous` (`Infinity`, which is truthy) as `resultSelector`. On the first inner emit, rxjs calls it and throws a `TypeError`. waitOn rejects, so the tests go red for the wrong reason.
   - Fix: drop the second argument in the mutation.
2. **Server-side abort detection races the client callback.**
   - What happens: a client abort reaches the server's `close` handler a few event-loop turns after waitOn settles. In the first M-E run, the reverse test failed on the overlap assertion instead of the abort assertion, because `aborted` still read `false`.
   - Fix: assert after a short settle (`afterServerSettles`, 100 ms).
3. **Reverse mode hides abort-the-previous-request regressions.**
   - What happens: under M-E the aborted request throws, so `httpCallSucceeds` returns `false`. `negateAsync` turns that into `true`, so reverse mode resolves. Only `requests[0].aborted === false` catches it.
   - `switchMap` plus an abort does **not** reproduce this, because `switchMap` unsubscribes the inner observable before it can emit. Model M-E as `mergeMap` plus a manual abort of the previous request's AbortController.

## Why This Matters

A characterization test proves nothing until you have seen it fail on the regression it names (AGENTS.md: "Right reason", "Prove the path ran"). Without the mutation runs, trap 1 would have produced red tests for the wrong reason. Trap 3 would have let a real regression pass every assertion but one. That regression is plausible in a hand-rolled polling loop, such as a future Rust port.

## When to Apply

- Adding or changing tests for `createHTTP$` / `httpCallSucceeds` concurrency, timeout, or reverse behavior.
- Any test where reverse mode can turn an error into success: assert on what the server saw, not only on the waitOn outcome.
- Any rxjs 7 operator-swap mutation: check the replacement operator's second parameter first.

## Examples

Mutation loop (M-C shown; edit only the `createHTTP$` block):

```sh
cp lib/wait-on.js "$TMPDIR/wait-on.js.bak"
# M-C: mergeMap(() => {...}, simultaneous)  ->  exhaustMap(() => {...})   (drop the 2nd arg)
npx mocha --exit test/slow-response.mocha.js   # expect overlap (maxConcurrent >= 2) assertions red
cp "$TMPDIR/wait-on.js.bak" lib/wait-on.js && git diff --exit-code lib/
```

Red for the wrong reason: `switchMap(fn, simultaneous)` rejects waitOn with a `TypeError` from calling `Infinity` as `resultSelector`, not with a chai assertion failure.

## Related

- `docs/reviews/2026-10-01-issue259-slow-http-inflight-assessment.md`: issue #259 assessment and the full results/mutation matrix.
- `docs/reviews/2026-09-29-pr238-axios-fetch-parity.md`: the axios -> fetch/undici parity audit this work extends.
- `docs/plans/2026-10-01-1216-test-slow-http-inflight-plan.md`: the plan (KTD5 lists the mutations).
