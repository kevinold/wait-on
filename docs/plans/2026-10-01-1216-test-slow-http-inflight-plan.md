---
title: Slow HTTP In-Flight Request Characterization - Plan
type: test
date: 2026-10-01
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# Slow HTTP In-Flight Request Characterization - Plan

## Goal Capsule

- Objective: a wait-on user (API or CLI) whose http(s) target always answers slower than `interval` can trust that wait-on still succeeds, because a committed test suite proves the earlier in-flight request survives later polling ticks on `next` (undici), the same tests confirm the v9.5.1 (axios) baseline behaves identically, and jeffbski/wait-on#259 carries a cell-by-cell map of that evidence.
- Means: one self-contained characterization test file covering the issue's nine matrix cells with a first-request-only-succeeds server (KTD1, KTD3), each test shown able to go red by a local uncommitted mutation (KTD5), a one-off run of the same file inside a scratch v9.5.1 export (KTD6), and a results table posted as an issue comment (KTD8).
- Delivery: branch `test/slow-http-inflight` on kevinold/wait-on (remote `origin`), based on `next`; a comment on jeffbski/wait-on#259 linking the commit permalink of the test file. No PR unless a defect is found (KD2).
- Authority: AGENTS.md (TDD, conventions, clock rules) over this plan; this plan over the issue text where they differ (KTD3 refines the reverse cell).
- Execution profile: `/ce-work`, Standard depth, test-only change set (`test/slow-response.mocha.js` plus this plan under `docs/plans/`).
- Stop conditions: a cell fails on `next` for a reason the mutation check does not explain (switch to the defect flow in KD1: failing repro, fix in `lib/`, PR into `next`); a cell cannot be turned red by any mutation in KTD5 (report it as unprovable instead of committing a vacuous test); `gh` cannot comment on the upstream issue (post the table in the handback and stop).

---

## Product Contract

### Summary

Add `test/slow-response.mocha.js`: nine behavior-named mocha tests, one per cell of the #259 matrix, each against a local server whose every response is delayed by `D` (400 ms) with `interval` 100 ms, and which answers success only to the first request it receives. Each test asserts wait-on's front-door outcome (resolve, reject, or CLI exit code) plus server-side proof that request #1 was answered rather than aborted and, where overlap is expected, that later ticks ran while it was in flight. Prove each test can fail with local mutations of `createHTTP$` / `httpCallSucceeds`, run the identical file against the v9.5.1 axios tag in a scratch export, and post the cell → test → next → v9.5.1 → mutation table on jeffbski/wait-on#259.

### Problem Frame

`createHTTP$` uses `mergeMap(..., simultaneous)` on `timer(delay, interval)` (`lib/wait-on.js` createHTTP$), so a new tick should not cancel an earlier request; the only abort sources are `httpTimeout` and the resource teardown signal in `httpCallSucceeds`. The existing suite only exercises slow servers in the failing direction (`test/api.mocha.js` 'should timeout when an http resource does not respond before httpTimeout', `test/cli.mocha.js` same name, the frozen unix-socket too-slow test). No test has latency greater than `interval`, expects success, and proves the success came from the request that was in flight across later ticks. The axios → undici migration (#238) rewrote this layer, and a Rust port is being weighed, so the guarantee needs implementation-independent tests.

### Requirements

**Coverage of the matrix**

- R1. Every cell 1–9 of issue #259 has one named test in `test/slow-response.mocha.js` (names listed per unit).
- R2. Every success cell asserts all three: wait-on succeeds (callback `err` falsy, or CLI exit code 0); the server's record for request #1 shows it was answered, not client-closed before the response was written; and the overlap evidence for its `simultaneous` setting (max concurrent requests ≥ 2 for the default, exactly 1 for cell 4).
- R3. The server answers success only to request #1 (reverse cell: a failing status only to request #1, 2xx afterwards), so a dropped or cancelled first result cannot be rescued by a later request; this is what makes each success assertion prove the in-flight path.
- R4. The control cell (`httpTimeout` < D) asserts wait-on rejects and that the server observed request #1 client-closed before its response was written.

**Portability of the tests**

- R5. The file requires only Node built-ins, `mocha`, `chai`, `'../'` (the package main) and `../bin/wait-on` by path; it does not require `test/frozen-clock.js`, `test/helpers/*`, or any shared fixture, so it runs unmodified when copied into a v9.5.1 checkout and can be transcribed for a Rust port.
- R6. Tests run on the real clock with generous headroom, bind ephemeral ports (`listen(0)`), use temp directories for sockets (Windows named pipe via the `socketPathIn` pattern), and skip (not fail) the https cell when `openssl` is unavailable; they pass on the CI matrix (ubuntu + windows, Node 22/24/26, `.github/workflows/node.js.yml`).

**Evidence that the tests are live**

- R7. Each test is shown able to fail: a local, uncommitted mutation of `lib/wait-on.js` from KTD5 turns it red for the reason the test names, then the mutation is reverted and the test is green again. The outcome per cell is recorded in the issue comment.
- R8. The same test file is run once against tag v9.5.1 in a scratch export of the tag with its own `npm ci`; per-cell results are recorded; nothing from that run is committed anywhere.
- R9. A comment on jeffbski/wait-on#259 contains a table with one row per cell: concern, test name, result on `next` (undici), result on v9.5.1 (axios), mutation-check result, plus a commit permalink to the test file on kevinold/wait-on. A PR is opened only if a cell exposes a defect.

**Process**

- R10. Commits follow Conventional Commits (`test(http): ...`, body `Refs jeffbski/wait-on#259`); the test file and this plan land on the branch; `lib/` and `bin/` are unchanged unless the defect flow of KD1 triggers.

### Key Decisions

- KD1. **Characterization tests only; no library change is expected.** (session-settled: user-directed — chosen over refactoring or hardening the polling pipeline: the concern is a missing proof, not a known defect; a defect found by a test is fixed test-first in the same branch.) Governs R1–R4, R10.
- KD2. **The deliverable is the issue comment; a PR is opened only if a defect needs fixing.** (session-settled: user-directed — chosen over opening a PR to land the tests by default: the cell map on #259 is the confirmation artifact the user asked for.) Governs R9.
- KD3. **v9.5.1 parity is a one-off local run, not a suite fixture.** (session-settled: user-approved — chosen over a dual-version CI job: no two-version harness exists and the parity result is a point-in-time fact for the issue.) Governs R8.
- KD4. **Real clock with headroom, not `itFrozen`.** (session-settled: user-approved — chosen over the frozen clock: the resource's state changes on real `setTimeout`s and the CLI cell is a subprocess, the AGENTS.md carve-out.) Governs R6.

### Scope Boundaries

- Deferred: `tcp:` and `socket:` slow paths. Those checks are connect-only (no response-latency dimension beyond `tcpTimeout`) and share the same `mergeMap(..., simultaneous)` shape; revisit if the Rust port plan wants a transport-wide matrix.
- Deferred: extracting the openssl cert generation from `test/https-proxy.mocha.js` into a shared helper (KTD7 duplicates it on purpose).
- Outside: the Rust port, any rxjs pipeline refactor, changes to `index.d.ts`, README, or `bin/usage.txt` (no option changes).

### Sources

- jeffbski/wait-on#259 (matrix and acceptance).
- `lib/wait-on.js` createHTTP$ and httpCallSucceeds on `next`; `git show v9.5.1:lib/wait-on.js` for the axios shape (same `mergeMap(..., simultaneous)`, `await axios(httpOptions)`, `timeout: httpTimeout`).
- `test/https-proxy.mocha.js` (self-contained file precedent: openssl EC P-256 cert in `before`, `listen(0, 'localhost')`, servers closed in `afterEach`).
- `test/api.mocha.js` `socketPathIn` and the existing slow-server failure tests; `test/cli.mocha.js` spawn pattern.
- `.mocharc.json` (`require: ./test/frozen-clock.js` on `next` only; v9.5.1 has no `.mocharc.json`).

---

## Planning Contract

### Key Technical Decisions

- KTD1. **New self-contained file `test/slow-response.mocha.js` (recorded carve-out from the AGENTS.md API → `api.mocha.js`, CLI → `cli.mocha.js` mapping).** R5 cannot be met inside the existing files: v9.5.1 has no `test/frozen-clock.js`, `test/helpers/`, or `.mocharc.json`, and its `api.mocha.js` has diverged, so cells copied there would need hand-merging. One file with its own `describe('slow http responses (issue #259)')`, own server factory, and ephemeral ports also avoids the fixed-port collisions of `api.mocha.js` (3000/8125). Precedent: `test/https-proxy.mocha.js`. Lint and `npm test` pick the file up through the `test/**/*.mocha.js` glob.
- KTD2. **One server factory with counters.** `slowServer({ delay, first, later, headersFirst, createServer })` returns `{ server, requests, maxConcurrent(), listen(target, cb) }`. On each request it pushes `{ aborted: false }`, increments in-flight and tracks the max, picks status `first` for the first record and `later` otherwise, and ends the response after `delay`. With `headersFirst` it writes head and one body chunk immediately and ends after `delay` (GET cell). `res.on('close')` clears the pending timer, decrements in-flight, and sets `aborted = true` when `!res.writableFinished`. `createServer` defaults to `http.createServer` and takes an https variant for the https cell.
- KTD3. **First-request-only success is the proof mechanism.** Success cells: `first: 200, later: 503`. Reverse cell: `first: 503, later: 200`, so reverse mode can only succeed through the first, slow, failing response (refines the issue's "slow server that eventually goes away"). A result dropped by a hypothetical switchMap-style cancellation can never be recovered by a later request, so "resolves" alone already proves in-flight survival; the server counters add the direct evidence R2 asks for.
- KTD4. **Timing constants.** `D = 400`, `interval: 100`, `timeout: 3000`, `window: 100`, mocha `this.timeout(10000)`. Overlap assertion is `maxConcurrent >= 2`, not an exact count, for Windows headroom. Control cell: `httpTimeout: 150`, `timeout: 1000`. CLI cell: `-i 100 -t 3000`.
- KTD5. **Mutation check stands in for RED.** Characterization tests pass on first run; AGENTS.md requires showing they can fail. Each mutation is a local edit to `lib/wait-on.js`, run with `npx mocha --exit test/slow-response.mocha.js`, then reverted and the file re-run green; none is committed.
  - M-A and M-C must drop the `, simultaneous` second argument: rxjs 7 reads a second argument to `switchMap` / `exhaustMap` as a deprecated `resultSelector`, so keeping it throws a TypeError on the first response and turns cells red for the wrong reason.
  - M-A: in createHTTP$ replace `mergeMap(() => {...}, simultaneous)` with `switchMap(() => {...})` (add `switchMap` to the rxjs import). Expected red: cells 1, 2, 3, 4, 5, 7, 8, 9 reject or exit 1 on `timeout` (with `D` > `interval`, every inner request is unsubscribed before it can emit). Unsubscribing `from(promise)` does not abort the fetch, so the server's abort flag stays false under M-A; the red comes from R3.
  - M-B: in httpCallSucceeds set `const signal = teardownSignal` (drop `AbortSignal.timeout`). Expected red: cell 6 resolves instead of rejecting.
  - M-C: replace `mergeMap(() => {...}, simultaneous)` with `exhaustMap(() => {...})`. Expected red: cells 1 and 9 still resolve / exit 0 and fail only on `maxConcurrent >= 2`, proving the overlap assertion is live.
  - M-D: change `}, simultaneous)` in createHTTP$ to `}, Infinity)`. Expected red: cell 4 fails on `maxConcurrent === 1`.
  - M-E (abort-previous-but-report, the shape a hand-rolled poll loop in a Rust port could take): keep `mergeMap`, but on each tick abort the previous request's `AbortController` (combined into its fetch signal) while its result is still delivered. Expected red: success cells reject on `timeout` (every request is aborted by the next tick) with `requests[0].aborted === true`; reverse cell 8 resolves (the aborted request errors, which reverse mode counts as success) and fails only on `requests[0].aborted === false`. This is the mutation that proves the abort assertion is live on a success cell; without it, a cancel-by-abort regression in reverse mode would pass on "resolves" alone, so KTD3's "resolves alone proves survival" holds for the forward cells but not for cell 8.
- KTD6. **Parity run in a scratch export of v9.5.1.** Export the tag's tree into `<scratchpad>/wait-on-v9.5.1` with `git archive v9.5.1` piped to `tar -x` (no second worktree: the worktree-isolated session refuses `git worktree add` targeting another path, and an export needs no cleanup through git), `npm ci` there, copy `test/slow-response.mocha.js` into its `test/`, run `npx mocha --exit test/slow-response.mocha.js` (no `.mocharc.json` at v9.5.1, so nothing else loads), capture per-cell pass/fail/skip, then delete the scratch directory. Run multi-step shell sequences from a script file in the scratchpad, since the session guard rejects compound git commands. The CLI cell spawns that checkout's `bin/wait-on` automatically because the file resolves it relative to `__dirname`.
- KTD7. **https cert generated inline with openssl (EC P-256), copied from `test/https-proxy.mocha.js` `before`.** Ten duplicated lines beat a new shared helper that v9.5.1 lacks (R5). `before` sets `this.timeout(30000)` and `this.skip()`s when `openssl version` fails; `strictSSL: false` on the resource.
- KTD8. **Report shape and delivery.** `gh issue comment 259 --repo jeffbski/wait-on --body-file <scratchpad>/259-comment.md`. Table columns: Cell, Concern, Test, next (undici), v9.5.1 (axios), Mutation check. Header links `https://github.com/kevinold/wait-on/blob/<sha>/test/slow-response.mocha.js` and names the Node version and OS the local runs used. A skipped https cell is reported as "skipped: no openssl" rather than omitted.

### Test-harness design

- File skeleton: `'use strict'`; requires `child_process`, `fs`, `http`, `https`, `os`, `path`, `mocha`, `chai`, `waitOn = require('../')`; `CLI_PATH = path.resolve(__dirname, '../bin/wait-on')`; local `socketPathIn(dir)` (same three lines as `test/api.mocha.js`); `D`, `FAST = { interval: 100, timeout: 3000, window: 100 }`.
- Lifecycle: `servers` array; `afterEach` calls `closeAllConnections()` then `close()` on each and waits, so undici keep-alive sockets or the CLI subprocess's leftovers never bleed into the next test. Temp dir via `fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-slow-'))`, removed in `after`.
- Assertions: `err` falsy, `requests[0].aborted === false`, `maxConcurrent() >= 2`; control: `err` truthy, `requests[0].aborted === true`.

### Assumptions

- `openssl` is on PATH locally (otherwise cell 3 is skipped locally and reported as such; CI runners have it, as `test/https-proxy.mocha.js` relies on).
- `gh` is authenticated with permission to comment on jeffbski/wait-on (already used to file #259).
- Local Node satisfies both `>=22.19.0` (next) and `>=20` (v9.5.1).
- The v9.5.1 `bin/wait-on` accepts `-i` and `-t`.

### Sequencing

U1 (harness + cell 1) first, since every other cell reuses the factory and its mutation check validates the proof mechanism. U2, U3, U4 follow in any order within the same file. U5 (parity) runs after the file is final. U6 (commit, push, comment) last.

### Risks

- CLI cell teardown: `process.exit(0)` in `bin/wait-on` drops the subprocess's open sockets, so the server sees client-closed records for requests ≥ 2; only request #1 is asserted (R2).
- Windows named pipes with delayed responses: the pattern already runs in `test/api.mocha.js`; keep `D`, `timeout`, and mocha timeouts as in KTD4 and widen rather than touch `lib/` if a flake appears (AGENTS.md rule).
- Running `npx mocha` without `--exit` can hang on leftover handles; every command in this plan passes `--exit`.

---

## Implementation Units

### U1. Harness and cell 1 (HEAD, default simultaneous)

- **Goal:** the server factory, lifecycle hooks, and the first proof test exist and are shown to go red.
- **Requirements:** R1, R2, R3, R5, R6, R7.
- **Files:** `test/slow-response.mocha.js` (new).
- **Approach:** write the skeleton and `slowServer` per KTD2 and the harness design; add the cell 1 test with `http://localhost:<port>/`, options `FAST`; run it green; apply M-A then M-C from KTD5, observe red on the expected assertion, revert, rerun green.
- **Test Scenarios:**
  - 'should succeed when an http HEAD response arrives after several polling intervals': server `{ delay: D, first: 200, later: 503 }`; expect `err` falsy, `requests[0].aborted === false`, `maxConcurrent() >= 2`.
- **Verification:** `npx mocha --exit test/slow-response.mocha.js`; `npm run lint`; M-A red (timeout rejection), M-C red (overlap assertion), both reverted and `lib/` diff clean.

### U2. HTTP option cells: GET body, simultaneous 1, httpTimeout above and below D, reverse

- **Goal:** cells 2, 4, 5, 6, 8 prove the in-flight request survives under each option that touches the request lifecycle.
- **Requirements:** R1, R2, R3, R4, R7.
- **Files:** `test/slow-response.mocha.js`.
- **Approach:** five tests reusing the factory; cell 2 uses `headersFirst: true` and resource prefix `http-get:`; cell 6 is the only rejection test and leaves the existing `api.mocha.js` httpTimeout test untouched; after green, run M-A (cells 2, 4, 5, 8 red), M-B (cell 6 red), M-D (cell 4 red), M-E (cell 8 red on the abort assertion), revert each.
- **Test Scenarios:**
  - 'should succeed when an http GET body completes after several polling intervals': `headersFirst: true`, `first: 200`, `later: 503`; expect resolve, request #1 not aborted, `maxConcurrent() >= 2`.
  - 'should succeed with simultaneous 1 without overlapping or cancelling the slow request': `simultaneous: 1`; expect resolve, request #1 not aborted, `maxConcurrent() === 1`.
  - 'should succeed when httpTimeout exceeds the slow response delay': `httpTimeout: 1500`; expect resolve, request #1 not aborted, `maxConcurrent() >= 2`.
  - 'should fail and abort the request when httpTimeout is shorter than the response delay': `httpTimeout: 150`, `timeout: 1000`; expect `err` truthy and `requests[0].aborted === true`.
  - 'should succeed in reverse mode when the first slow response is a failing status': `reverse: true`, `first: 503`, `later: 200`; expect resolve, request #1 not aborted, `maxConcurrent() >= 2`.
- **Verification:** `npx mocha --exit test/slow-response.mocha.js` green; mutation results per KTD5 recorded for the comment; `npm run lint`.

### U3. Transport cells: https and unix socket

- **Goal:** cells 3 and 7 prove the same guarantee over TLS and over a unix-domain socket / Windows named pipe.
- **Requirements:** R1, R2, R3, R5, R6, R7.
- **Files:** `test/slow-response.mocha.js`.
- **Approach:** nested `describe('https')` with the inline openssl `before` from KTD7 and a `createServer` override into the factory; unix cell listens on `socketPathIn(tmpDir)` and uses resource `'http://unix:' + sock + ':/'`; after green, run M-A and confirm both red, revert.
- **Test Scenarios:**
  - 'should succeed when an https response arrives after several polling intervals': https server with generated cert, `strictSSL: false`; expect resolve, request #1 not aborted, `maxConcurrent() >= 2`; skipped when openssl is missing.
  - 'should succeed when a unix-socket http response arrives after several polling intervals': http server on the socket path; same three assertions.
- **Verification:** `npx mocha --exit test/slow-response.mocha.js` green locally; CI on windows runs the named-pipe variant; M-A red for both cells.

### U4. CLI cell

- **Goal:** the CLI front door exits 0 against the slow server with the same server-side proof.
- **Requirements:** R1, R2, R3, R5, R7.
- **Files:** `test/slow-response.mocha.js`.
- **Approach:** spawn `process.execPath` with `[CLI_PATH, url, '-i', '100', '-t', '3000']`, listen for `exit`; assert after exit; M-A (exit 1) and M-C (overlap assertion) must turn it red, then revert.
- **Test Scenarios:**
  - 'should exit 0 when the CLI polls a server slower than its interval': server `{ delay: D, first: 200, later: 503 }`; expect exit code 0, `requests[0].aborted === false`, `maxConcurrent() >= 2`.
- **Verification:** `npx mocha --exit test/slow-response.mocha.js`; `npm test` full suite green.

### U5. Axios parity run against v9.5.1

- **Goal:** per-cell results for the identical file on the last axios release, with nothing committed from the run.
- **Requirements:** R8.
- **Files:** none in the repo (scratch export under the session scratchpad).
- **Approach:** follow KTD6; save the mocha output to `<scratchpad>/parity-v9.5.1.txt`; delete the scratch directory afterwards.
- **Test Scenarios:** the nine tests above, unmodified, on v9.5.1. Expected: all pass (cell 3 skipped if no openssl). Any failure is examined for a harness cause before being reported as an axios delta; it does not block U6.
- **Verification:** results captured; `git status` in the branch worktree shows no unintended changes.

### U6. Commit, push, and report on #259

- **Goal:** the evidence is durable on the fork branch and visible on the upstream issue.
- **Requirements:** R9, R10.
- **Files:** `test/slow-response.mocha.js`, this plan under `docs/plans/`.
- **Approach:** commit `test(http): characterize in-flight requests slower than interval` with body `Refs jeffbski/wait-on#259` (plan file staged in the same commit); `git push origin test/slow-http-inflight`; build the table per KTD8 from U1–U5 results; post with `gh issue comment`. Open a PR only if a cell exposed a defect that was fixed (KD1, KD2).
- **Test expectation:** none -- delivery step, no executable change.
- **Verification:** the comment renders with nine rows and a working permalink; `git log origin/test/slow-http-inflight` contains the commit; no PR exists unless the defect flow ran.

---

## Verification Contract

| Check | Command | Applies to |
|---|---|---|
| New file alone (fast loop) | `npx mocha --exit test/slow-response.mocha.js` | U1–U4, every mutation run |
| Lint | `npm run lint` | U1–U4 |
| Full gate | `npm test` (lint + `test:types` + mocha) | U4, before U6 |
| Coverage thresholds unaffected | `npm run test:coverage` | optional, U4 |
| Mutation evidence | KTD5 M-A..M-E, each reverted and followed by a green rerun | U1–U4 |
| Parity | KTD6 run in `<scratchpad>/wait-on-v9.5.1` | U5 |
| CI | push triggers `.github/workflows/node.js.yml` (ubuntu + windows, Node 22/24/26); commitlint requires the Conventional Commit subject | U6 |

---

## Definition of Done

- All nine tests exist in `test/slow-response.mocha.js`, named as in U1–U4, and pass with `npm test` locally and on the fork's CI for both OSes.
- Every test was observed red under at least one KTD5 mutation and green after revert; `lib/wait-on.js` and `bin/wait-on` are unchanged in the final diff (unless the KD1 defect flow ran, in which case the fix has a failing repro committed first and a PR into `next`).
- Parity results for v9.5.1 are recorded per cell; the scratch export is deleted; nothing from it is committed.
- The comment on jeffbski/wait-on#259 is posted with the nine-row table and a commit permalink to the test file on kevinold/wait-on.
- The commit includes this plan file under `docs/plans/`; no `.only`, `.skip` (other than the openssl `this.skip()`), or disabled tests remain; no experimental code from mutations or parity runs is left in the tree.
- `/ce-compound` is run only if the work surfaced a non-obvious learning (for example an axios/undici parity delta or a Windows timing fact).
