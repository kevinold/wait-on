---
title: "[L8] test: parser differential, timing tolerance and startup benchmark - Plan"
type: test
date: 2026-09-30
topic: rust-port
lane: L8
kind: preview
branch: rs-60-parity-bench
closes: kevinold/wait-on#60
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L8] test: parser differential, timing tolerance and startup benchmark - Plan

Implementation-ready lane plan for sub-issue #60 (lane L8 of spine #35). Product Contract preservation: requirements R-L8-1..R-L8-12 keep the IDs and meaning of the requirements-only plan; the open areas (Rust parser technique, napi shapes, where the differential lives and how it must run under `ci:rs`, the tolerance values, the benchmark harness and its gate) are decided in KTD1..KTD8, and the parser x input-class x engine and engine x vector-class matrices live under the Verification Contract. The spine plan is controlling and is never edited by this lane.

---

## Goal Capsule

**Objective.** After this lane merges into `spike-next-rs`: a Rust parser that disagrees with the JS parser on any input the #245 suite can generate fails `npm run ci:rs` naming the seed and input; a CLI build that finishes a conformance vector earlier or later than one documented tolerance fails the same suite on either engine; and a Rust addon whose startup overhead over the JS engine grows past a documented threshold fails `ci:rs` on every CI OS. Proves spine lane row L8 (R17, R18, PO13, PO22). The JS engine, `npm test`, coverage thresholds, public API, CLI, `WAIT_ON_SCHEMA` and `index.d.ts` are unchanged.

**Means.** Hand-written parsers in `crates/wait-on-core/src/parse.rs` that replicate the JS regex semantics (KTD1), four sync napi exports with JS-comparable shapes (KTD2), a differential `describe` appended to `test/parser-properties.mocha.js` that feeds every existing generator and golden list to both engines (KTD3, KTD4), one named tolerance and one assertion helper in `test/helpers/cli-conformance.js` (KTD5), and `scripts/bench-startup.js` gated on in-run relative overhead with `benchmarks/startup-baseline.json` as the record (KTD6, KTD7), wired as the last `ci:rs` step.

**Authority.** `AGENTS.md` > spine plan KD-S1..KD-S9 > this plan.

**Stop conditions.** A `.github/workflows/` change is needed -> stop and report to the PM (operator request on #35). A public API / CLI / `WAIT_ON_SCHEMA` / `index.d.ts` change would be needed -> stop. A new runtime npm dependency would be needed -> stop. A parser difference between engines whose fix is a JS behavior change -> stop and report (JS behavior is unchanged in this lane; the Rust side changes). A new crate (`regex` or any other) seems needed -> stop and record why before adding one (KTD1 does it in std).

**Execution profile.** `/ce-work` via `/lfg` from this plan; strict TDD per `AGENTS.md`; the conformance and benchmark work stays on the real clock (CLI subprocesses); one PR, base `spike-next-rs` on `kevinold/wait-on`, body `Closes #60`; never merged by the lane; merge `origin/spike-next-rs` (no rebase, no force-push) before marking ready.

---

## Product Contract

### Summary

Expose wait-on's four pure parsers from Rust so the #245 property suite can run as a differential oracle against the JS parsers, replace the ad-hoc elapsed-time bounds in the CLI conformance suites with one named tolerance that also has an upper bound, and add a startup/overhead benchmark with a committed baseline and a regression threshold that `npm run ci:rs` runs on ubuntu, macos and windows.

### Problem Frame

`test/parser-properties.mocha.js` (#245) already holds seeded generators, golden vectors and verbatim copies of `PREFIX_RE`, `HOST_PORT_RE`, `HTTP_UNIX_RE` and `HTTP_UNIX_LEGACY_RE`, plus the real `parseInterval` from `bin/wait-on`, shaped as "the Node oracle a future Rust implementation must match". No Rust parser exists yet (`wait-on-core` has `file_size`, `run_command`, `tcp`, `socket`). The conformance suites assert only lower bounds, each written by hand (`APPEAR - 150`, `T * 0.5`), so a slower engine cannot fail them, and `ci:rs` has no startup measurement at all even though the source plan makes a startup benchmark part of the acceptance gate (PO22).

### Requirements

**Parser differential (R18, PO13)**

- R-L8-1 Rust parsers. `wait-on-core` implements the four pure parsers the #245 suite covers: resource prefix (the `PREFIX_RE` split), `host:port` incl. bracketed IPv6 and bare-port default, the `ms/s/m/h` interval (`parseInterval`, including its current quirks such as uppercase units returning undefined), and the `http://unix:<sock>:<path>` split. They never panic on arbitrary input.
- R-L8-2 Addon surface. `wait-on-napi` exposes each parser to JS synchronously, returning a value shape directly comparable with the JS parser's result.
- R-L8-3 Differential oracle. `test/parser-properties.mocha.js` (or a sibling test) feeds the same seeded generated inputs to the JS parser and the Rust parser and asserts deep-equal results, reporting seed and input on mismatch. It runs when the real addon is loadable and skips cleanly otherwise (so `npm test` without a Rust toolchain stays green); under `ci:rs` (`rust-strict`) it must run, not skip.
- R-L8-4 Golden vectors. The existing golden/example vectors of the #245 suite also run differentially, so edge cases (IPv6, empty port, unknown units, missing unix path) are covered deterministically, not only by chance.

**Timing tolerance (R17, PO13)**

- R-L8-5 One tolerance. The timing slack the conformance suites apply (today ad-hoc: `APPEAR - 150`, `T * 0.5`, `below(1000/1500/2000)`) is replaced by one named, shared tolerance (lower and upper bound around expected elapsed time) used by both `cli-conformance*.mocha.js` files and enforced identically for both engines.
- R-L8-6 Upper bound. Conformance timeouts assert an upper bound as well as a lower bound, so a slower engine (e.g. FFI overhead) that exceeds the tolerance fails.
- R-L8-7 Documented. The tolerance value and its rationale are recorded in `docs/guides/testing.md` as the R5/R17 promote criterion.

**Startup benchmark (PO22)**

- R-L8-8 Benchmark script. A script under `scripts/` or `benchmarks/` measures CLI startup/overhead (e.g. median of N spawns of a trivially satisfied wait-on invocation) for `js` and `rust` engines with no new dependency.
- R-L8-9 Baseline + threshold. A recorded baseline file lives in the repo and the script fails when the measured value regresses past a stated threshold (relative, with an absolute floor so noisy CI runners do not flake).
- R-L8-10 Wired into `ci:rs`. `scripts/ci-rs.js` runs the benchmark as a step; no workflow edit.
- R-L8-11 Guides. `docs/guides/testing.md` (and `ci.md` / `development.md` as applicable) describes the differential suite, the tolerance, the benchmark, how to refresh the baseline, and how to reproduce a failing seed.

**Invariants**

- R-L8-12 JS engine unchanged. `npm test` outcome, coverage thresholds, public API, CLI, `WAIT_ON_SCHEMA`, `index.d.ts` unchanged. `test/rust-pending.js` stays empty.

### Scope Boundaries

- Allowed paths per lane contract (#60): `crates/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `lib/`, `bin/`, `test/`, `index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`, `.gitignore`, `.nycrc.json`, `eslint.config.mjs`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `AGENTS.md`, `README.md`, `benchmarks/`, `scripts/`. No `.github/workflows/` edits. No new npm dependency and no new crate (KTD1, KTD6).
- Not in scope: routing the live JS engine's parsing through Rust (the parsers are exposed for differential testing; the L7 loop owns using them at runtime), the standalone binary, performance improvement work, `test/cli.mocha.js` timing assertions (not a conformance suite), `test/cli-conformance-helper.mocha.js`'s `elapsedMs > 0` harness self-check (not a tolerance).
- Shared-file edits stay small and additive (`crates/wait-on-core/src/lib.rs` one `pub mod parse;`, `crates/wait-on-napi/src/lib.rs` appended exports, `scripts/ci-rs.js` one step, `package.json` one script, `test/scripts.mocha.js` one `describe` plus one step in the pinned list, guides new sections and rows) so the L4 and L9 merges stay mechanical.
- Non-goals (considered, not built, one line each):
  - A `regex` crate: a supply-chain delta under `cargo deny`/`cargo vet` for four fixed patterns whose backtracking is trivial (KTD1); add only if a fifth parser with real regex complexity appears.
  - Cross-runner absolute benchmark comparison (measured median vs a recorded median): CI runners differ by 2-3x, so it would flake or need a threshold too loose to mean anything; the gate is the in-run JS-vs-Rust overhead (KTD6) and the recorded medians are the record, not the gate.
  - A JS-side adapter that normalizes the addon's return values before comparison: it would hide exactly the shape differences the differential exists to catch; shapes are settled in KTD2.
  - Per-vector custom tolerances: one pair of numbers for every vector is the requirement (R-L8-5); a vector that cannot fit them is a bug in the vector, not a reason for a third number.
  - A mocha test that runs the full benchmark: the `ci:rs` step is that run; mocha covers the pure functions and the fail-loud path (T10-T12).
  - Rust-side random fuzzing of the parsers: the JS junk generator already drives the Rust code through the addon on every run (T6); cargo tests keep a fixed nasty-input list (T2).

### Sources

- Issue kevinold/wait-on#60 (no comments at authoring time); spine plan lane row L8, KD-S7, KD-S9; source plan R5, R17, R18, PO13, PO22.
- `test/parser-properties.mocha.js` (mulberry32 PRNG, `WAITON_TEST_SEED`/`WAITON_TEST_RUNS`, `forAll`, `randStr`/`pick`/`randInt`, alphabets, `nodeParsers`, golden and reject arrays, `genUnixCase`), `bin/wait-on` (`parseInterval`, `module.exports`), `lib/wait-on.js` (`PREFIX_RE`, `HOST_PORT_RE`, `HTTP_UNIX_RE`, `HTTP_UNIX_LEGACY_RE`, `extractPrefix`/`extractPath`, `createHTTP$` `replace('-get:', ':')`).
- `test/helpers/cli-conformance.js` (`runCli` hrtime `elapsedMs`, `T`, `I`, `W`, `APPEAR`, `FAST_OPTS`, the "under mocha's 2000 ms default" comment), `test/cli-conformance.mocha.js` (bounds at lines 60, 67, 97, 120), `test/cli-conformance-http.mocha.js` (bounds at lines 49, 78, 85, 109), `test/cli-conformance-helper.mocha.js`, `.mocharc.json` (no timeout override: 2000 ms default).
- `lib/engine.js` (`resolveEngine`, `addonPath`, `rust-strict` throws on load failure), `test/engine-checks.mocha.js` (real-addon `before` skip pattern), `test/rust-pending.mocha.js` (mocha-in-a-subprocess pattern), `test/helpers/engine-env.js`, `test/rust-pending.js` (empty).
- `scripts/ci-rs.js` (`steps({ repoRoot })`, no shell), `scripts/build-napi.js`, `test/scripts.mocha.js` (pinned step list), `package.json` (`files` whitelist, so `.npmignore` needs no change for `benchmarks/` or `scripts/`; `lint` already covers `scripts/**/*.js`).
- `crates/wait-on-core/src/lib.rs` (`pub mod` list, `#[cfg(test)]` style), `crates/wait-on-napi/src/lib.rs` (`#[napi(object)]`, `use_nullable`), `crates/wait-on-napi/Cargo.toml` (`napi = "3"`, `default-features = false`, `features = ["napi4", "async"]`), `Cargo.toml` (edition 2024, MSRV 1.98.1), `deny.toml`.
- `docs/guides/testing.md`, `ci.md`, `development.md`, `architecture.md`, `contributing-dual-engine.md` (docs-as-done checklist). Sibling plans `docs/plans/2026-09-30-spike-rs-l3-file-plan.md`, `docs/plans/2026-09-30-spike-rs-l6-command-plan.md` (real-addon skip pattern, inversion checks for green-on-first-run tests).
- ECMAScript regex semantics the Rust code must replicate (single-pass trace of the four patterns): `.` excludes `\n`, `\r`, U+2028, U+2029; `[^:]` and `[^\]]` include them; `\d` is ASCII `0-9` without the `u` flag; `$` without `m` is strict end of input (no trailing-newline allowance); the `i` flag without `u` folds ASCII case only (a non-ASCII char never canonicalizes to an ASCII one); `String.prototype.replace` with a string pattern replaces the first occurrence only; `parseFloat` takes the longest decimal-literal prefix (`'1.2.3'` -> 1.2, `'5.'` -> 5, `'.5'` -> 0.5, `'.'` and `'..5'` -> NaN) and V8 rounds it correctly; `Math.floor(NaN)` is NaN; `assert.deepStrictEqual` treats NaN as equal to NaN and ignores key order.

---

## Planning Contract

### Key Technical Decisions

- KTD1. Hand-written parsers in `crates/wait-on-core/src/parse.rs`, std only, one function per parser, each a direct transcription of the one way its JS regex can match. Prefix: the input starts with one of the eight fixed scheme prefixes (`https-get:`, `http-get:`, `https:`, `http:`, `tcp:`, `socket:`, `file:`, `command:`, longest first) and the rest is non-empty with no line terminator; otherwise prefix `""`, rest = input, type `file`; type mapping as `PREFIX_TYPE`. Host:port: bracket form (`[` + one or more non-`]` + `]:`) first, else split at the first colon (host may be empty), else no host; the remainder must be one or more ASCII digits to the true end; empty host reads `localhost`; port stays a string. Interval: split at the first char outside `[0-9.]`; the numeric run must be non-empty and the remainder must equal `""`, `ms`, `s`, `m` or `h` ignoring ASCII case; a miss returns the input unchanged; the value is the `parseFloat` longest-prefix of the run (NaN for a dotted run with no digit before or after the first dot); the multiplier switch keys on the unit as written, so any uppercase letter yields undefined; arithmetic is `f64` in JS order (`value * 1000.0 * 60.0 * 60.0`, then `floor`). Http-unix: replace the first `-get:` with `:`, then, after a literal `http://unix:`, take the smallest colon position at or after one char such that the socket part has no line terminator and the request part starts with `https://`, `http://` or `/` and has no line terminator (the lazy `(.+?)`); else the legacy form: socket part is everything before the first colon (non-empty, terminators allowed), request part is the non-empty remainder with no line terminator; else none. All functions take `&str` and index only at `char_indices` boundaries, so they cannot panic. Chosen over a `regex` crate: a supply-chain delta for patterns whose only backtracking is a fixed alternation, and Rust's `regex` differs from JS on `.`, `$` and case folding anyway, so the semantics would still need hand-checking.
- KTD2. Four sync napi exports appended to `crates/wait-on-napi/src/lib.rs`, shapes deep-equal to `nodeParsers`: `parsePrefix(resource): { prefix, rest, type }` (a `#[napi(object)]` struct whose third field reaches JS as the key `type`); `parseHostPort(str): { host, port } | null` (`Option<HostPort>`, None crossing as `null`); `parseInterval(arg): number | string | undefined` (an `Either3` of `f64`, `String` and napi's `Undefined`, so the uppercase-unit case crosses as `undefined`, not `null`); `parseHttpUnix(resource): { socketPath, requestPath } | null` (field names camel-cased by napi). Sync because each call is microseconds of pure string work and an async shape would only make the differential await 1200 promises per run. No `#[napi(object)]` key may be absent when the JS side has it (`use_nullable` is irrelevant: no optional fields). Chosen over returning JSON strings: the differential would then compare JS-side parses, not the addon's values.
- KTD3. The differential lives at the bottom of `test/parser-properties.mocha.js` as `describe('Rust parsers match the Node oracle (real addon)')`. Its `before` resolves `{ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: process.env.WAIT_ON_NATIVE_LIBRARY_PATH }` through `resolveEngine`: when `process.env.WAIT_ON_ENGINE` is not `rust-strict` and `addonPath(vars)` does not exist it calls `this.skip()`; otherwise it calls `resolveEngine(vars)`, which throws under `rust-strict` when the addon cannot load, so under `ci:rs` a missing or stale addon fails the run instead of skipping (R-L8-3). Every input is fed to all four addon parsers and all four `nodeParsers` and compared with `assert.deepStrictEqual` inside `forAll` (which already prefixes seed, run index and input) or, for fixed lists, inside a loop that prefixes the input on failure. Chosen over a sibling file: the generators and golden lists are here, and a sibling would either duplicate them or import test internals.
- KTD4. Generator and golden hoist. The inline generator lambdas become module-level named generators (`genScheme`, `genUnprefixed`, `genAny`, `genHostPort`, `genBarePort`, `genIpv6`, `genInterval`, `genNonInterval`, plus the existing `genUnixCase`) that return their parts plus an `input` field holding the composed string; the existing properties keep their assertions and read `input` where they composed the string inline. The `golden` and `rejects` arrays move to module scope under parser-specific names. A new `genJunk` builds 0-8 tokens from a list that mixes scheme prefixes, `://unix:`, `-get:`, `:`, `/`, `[`, `]`, `\\?\pipe\`, digits, `.`, units in both cases and the four line terminators. A `DIFFERENTIAL_EXTRAS` list holds hand-picked inputs with concrete expected values where a naive port differs: `'1.2.3s'` -> 1200, `'.s'` -> NaN, `'5S'` -> undefined, `'2ſ'` -> `'2ſ'` (non-ASCII never case-folds), `'3000\n'` -> null, `'[]:80'` -> `{ host: '[]', port: '80' }`, `'tcp:a\nb'` -> type `file`, `'http-get://unix:/a-get:/b:/c'` -> `{ socketPath: '/a-get', requestPath: '/b:/c' }` (only the scheme's `-get:` is replaced), `'http://unix:/a\nb:/c'` -> `{ socketPath: '/a\nb', requestPath: '/c' }`. Chosen over leaving the existing tests untouched and re-listing inputs in the differential: two owners of the same vectors drift.
- KTD5. One tolerance, one helper. `test/helpers/cli-conformance.js` exports `TOLERANCE_MS = Object.freeze({ early: 100, late: 1000 })` and `expectElapsed(result, expectedMs)`, which asserts `result.elapsedMs` is within `[expectedMs - early, expectedMs + late]` with a message naming elapsed, expected and both bounds. `early` covers timer clamping only: `elapsedMs` starts before the child's own timers, so a child can never legitimately finish before `expectedMs`. `late` covers node startup plus module load on the slowest CI row (windows, measured by the benchmark) with headroom, and is sized so `T + late = 1800` stays under mocha's 2000 ms default; raising `late` therefore also means raising a per-suite `this.timeout` in both suites, together, and recording both in `testing.md`. Every timing assertion in `test/cli-conformance.mocha.js` and `test/cli-conformance-http.mocha.js` becomes `expectElapsed(r, APPEAR)` (resource appears later) or `expectElapsed(r, T)` (times out); no other numeric bound on `elapsedMs` remains in either file. The same call runs under `npm test` (js) and `npm run ci:rs` (rust-strict, inherited env), so both engines are held to the same numbers (R-L8-5, R-L8-6). Chosen over a percentage tolerance: the noise is additive (startup), not proportional to the wait.
- KTD6. `scripts/bench-startup.js` exports pure `median(values)`, `verdict({ jsMs, rustMs, threshold })` and `withRecording(baseline, key, sample)`, plus `main`. `main` reads `benchmarks/startup-baseline.json`, starts one `net` listener on `127.0.0.1:0` in-process, then spawns `node bin/wait-on tcp:127.0.0.1:<port> -t 10000` with `spawnSync` (no shell, `process.execPath`, `timeout` bounded) once untimed per engine as warm-up, then `runs` times (default 20, `--runs N`) alternating `WAIT_ON_ENGINE=js` and `WAIT_ON_ENGINE=rust-strict` so drift affects both equally, timing each spawn with `process.hrtime.bigint()`. Any non-zero exit aborts with that engine's stderr (under `rust-strict` a missing or stale addon fails here, which is the path proof: the Rust numbers can only come from a loaded addon). `overheadMs = median(rust) - median(js)`; `allowedMs = max(threshold.relative * median(js), threshold.floorMs)`; the verdict fails when `overheadMs > allowedMs` and prints both medians, the overhead and the allowance either way. A tcp resource is the trivially satisfied invocation because it has no stability window (a `file:` resource would add `window` ms of fixed wait to both medians) and under Rust it exercises addon load plus one `tcpCheck` FFI call, which is the overhead PO22 asks about. Chosen over comparing the measured median with a recorded one: runner-dependent (non-goal above).
- KTD7. `benchmarks/startup-baseline.json` holds `{ threshold: { relative, floorMs }, runs, recorded: { "<platform>-<arch>": { jsMs, rustMs, overheadMs, date } } }`; the gate reads `threshold` and `runs`, and `recorded` is the record R-L8-9 asks for. `--record` runs the benchmark and rewrites `recorded[<platform>-<arch>]` for the host (via `withRecording`, other keys kept), which is how the baseline is refreshed; a platform with no entry does not affect the gate. Initial values `relative: 0.25`, `floorMs: 50`, `runs: 20` (Assumptions). `package.json` gains `"bench:startup": "node scripts/bench-startup.js"` for local runs; `scripts/ci-rs.js` appends `{ cmd: process.execPath, args: [<repoRoot>/scripts/bench-startup.js] }` as the last step, after mocha, so a benchmark failure never masks a test failure. Chosen over a threshold in the script: the file is what a reviewer diffs when the threshold moves.
- KTD8. Lane contract. (session-settled: user-directed — no `.github/workflows/` edits; chosen over workflow edits in the lane: spine KD-S7, operator request on #35.) (session-settled: user-directed — stay inside the allowed paths listed under Scope Boundaries; chosen over other paths: parallel sibling lanes.) (session-settled: user-directed — PR base `spike-next-rs` on kevinold/wait-on, `Closes #60`, never merged by the lane; chosen over merging or targeting jeffbski/wait-on: PM owns merge, fork-only.) (session-settled: user-directed — JS engine behavior unchanged and JS stays the default; the Rust side changes on any parser mismatch; chosen over changing JS behavior or the default: side-by-side spike, KD-S1.) (session-settled: user-directed — shared-file edits small and additive; chosen over restructuring shared files: sibling lanes merge in parallel.)

### Assumptions

- napi-rs 3 under `default-features = false, features = ["napi4", "async"]` supports `Either3<f64, String, Undefined>` as a sync return and `Option<T>` returns crossing as `null`; verified by `cargo clippy` and T3 in U2. If `Undefined` cannot be an `Either` arm, the fallback is a dedicated sync export per outcome is not acceptable; instead return `Either<f64, String>` wrapped so that the uppercase case reaches JS as `undefined` (napi's `Env::get_undefined` on a manual `JsUnknown` return), still one export.
- A `#[napi(object)]` struct field can be named `type` on the JS side (raw identifier `r#type` or a field `js_name`); T3 asserts the key.
- V8's `parseFloat` and Rust's `f64::from_str` both round correctly, so a numeric run of any length parses to the same double; the generators cap runs well under 20 digits anyway.
- Tolerance values `early: 100`, `late: 1000` are planning inferences from the suite's existing headroom (`T * 0.5`, `APPEAR - 150`, no upper bound) and mocha's 2000 ms default; the first CI run on the windows `rust` row is the evidence for keeping or moving them (Risks). A change moves both numbers and the `testing.md` rationale together.
- Threshold values `relative: 0.25`, `floorMs: 50`, `runs: 20` are planning inferences: addon load plus tokio start should cost single-digit to low-double-digit ms against a 60-250 ms node startup. Rule for the executor: after `--record` on the host, and reading the `ci:rs` logs for the other two OSes, `allowedMs` on the noisiest OS must be at least twice its recorded `overheadMs`; if not, raise `floorMs` and record why in `testing.md`.
- Only the author's host gets a `recorded` entry in this PR; ubuntu and windows numbers appear in the `ci:rs` job logs and are added by whoever next runs `--record` there. The gate does not depend on entries.
- Sibling L9 may touch `scripts/`, `package.json` and `test/scripts.mocha.js`, and L4 `crates/*/src/lib.rs`; if `origin/spike-next-rs` changes the pinned `ci:rs` step list, re-append the benchmark step last and adjust T11 (mechanical).
- Pipeline run: the scoping synthesis confirmation was skipped (no synchronous user); the decisions above not labeled session-settled are planning inferences the PR reviewer may redirect.

### Risks

| Risk | Answered by |
|---|---|
| Rust `.` matches a line terminator, or `$` allows a trailing newline, so `'tcp:a\nb'` or `'3000\n'` parse under Rust but not JS | T2 (cargo line-terminator cases), T3 extras, T6 junk differential (tokens include all four terminators) |
| Interval arithmetic order or `parseFloat` prefix differs (`'1.2.3s'`, `'.s'`, `'5.'`, large values) | T1 interval golden, T3 extras, T5 (`genInterval` with fractional parts) |
| Case folding wider than ASCII (`'2ſ'`, `'5K'`) makes Rust accept a unit JS rejects | T3 extras, T2 cargo case |
| `-get:` replaced everywhere instead of first occurrence | T3 extra `'http-get://unix:/a-get:/b:/c'`, T1 http-unix golden |
| Lazy `(.+?)` split point wrong for colon-bearing socket paths (Windows pipes) | T1 golden pipe case, T4, T5 (`genUnixCase` pipe branch) |
| `undefined` crosses as `null` (deepStrictEqual fails) or `NaN` crosses as something else | T3 (`'5S'` -> undefined, `'.s'` -> NaN) |
| A Rust panic on odd input kills the mocha process instead of failing one test | T2 nasty list (`''`, `':'`, `'[', ']'`, only terminators, 10 kB input, non-ASCII, `'http://unix:'`), T6 |
| Differential silently skips under `ci:rs` (stale or missing prebuild) | T7 (mocha subprocess under `rust-strict` with a missing addon path exits non-zero naming the addon) |
| Hoisting generators changes the existing property vectors | Existing #245 tests keep their assertions and stay green; the PRNG draw order per generator is unchanged (parts drawn in the same order, `input` composed after) |
| Timing upper bound flakes on the windows `rust` row (startup + addon load) | T9 under `ci:rs` on windows; the Assumptions rule moves `late` and a per-suite `this.timeout` together, never one alone |
| Lower bound too tight for the "appears later" vectors (`APPEAR` 250 with `early` 100) | T9 on all rows; the child cannot finish before the parent's `later()` timer fires plus the window (`file`) or next poll (`http`) |
| Benchmark reports Rust numbers from a JS fallback | KTD6 spawns `rust-strict`; T12 (a junk addon path makes the bench exit non-zero and name the addon) |
| Benchmark flakes on a noisy runner | KTD6 interleaving, warm-up, median of 20; `floorMs` absolute floor (KTD7); T10 verdict cases at the floor |
| Benchmark failure hides a mocha failure in `ci:rs` | KTD7 step order (last); T11 pins the position |
| Bench hangs when the CLI never exits | `-t 10000` on the CLI and `spawnSync` `timeout`; T12 completes in bounded time |
| `benchmarks/` or `scripts/` leak into the npm tarball | `package.json` `files` whitelist already excludes them (Sources); `npm pack --dry-run` in the Verification Contract |
| L4/L9 parallel edits to `crates/*/src/lib.rs`, `scripts/`, `package.json`, `test/scripts.mocha.js`, guides | All L8 edits additive; merge `origin/spike-next-rs` before ready and rerun the Verification Contract |
| T4, T5, T6 are green on first run (U1 is already correct) | Expected by sequencing. After U2 GREEN, temporarily make the Rust prefix parser accept a `\n` in `rest` and confirm T6 goes red naming seed and input; temporarily swap the interval multiplier order to `value * 60 * 1000` and confirm T5 goes red on a fractional minute; restore both |
| T9 is green on first run (refactor of existing assertions) | After U3 GREEN, temporarily set `late` to 0 and confirm every "times out" and "appears later" vector goes red with the helper's message, then restore |

---

## Implementation Units

Order: U1 -> U2 -> U3 -> U4 -> U5. Test and code land in the same Conventional Commit. U1 before U2 so `ci:rs` is green at every commit (an export before its core function would fail clippy; a differential before its exports would fail `rust-strict`). U3 and U4 are independent of U1/U2 and of each other, but land in this order to keep the PR's commit story linear.

### U1. Rust parsers in core

**Goal.** `wait_on_core::parse` answers the four parsers with the exact JS results for the #245 golden vectors and the regex-semantics edge cases, and never panics.

**Requirements.** R-L8-1.

**Dependencies.** None.

**Files.** `crates/wait-on-core/src/parse.rs` (new), `crates/wait-on-core/src/lib.rs` (`pub mod parse;`).

**Approach.** KTD1. One public function per parser returning a small owned struct or `Option`; a private `is_line_terminator(char)` helper; the interval result is an enum with `Number(f64)`, `Unchanged(String)` and `Undefined` arms. Tests in a `#[cfg(test)]` module in `parse.rs`, written per parser against a `todo!()` body first.

**Patterns to follow.** `file_size` and its tests in `crates/wait-on-core/src/lib.rs` (plain `#[test]`, `assert_eq!` on concrete values, no crates).

**Test scenarios.**

- T1 (`cargo test`, prefix): the nine `PREFIX_GOLDEN` vectors from `test/parser-properties.mocha.js` parse to the same prefix, rest and type; `'tcp:'` and `''` give prefix `""`, type `file`.
- T1 (host:port): the five golden forms parse (`'3000'` -> localhost, `'[::1]:8080'`, `'[2001:db8::1]:443'`, `':3000'` -> localhost) and the nine rejects (`''`, `'abc'`, `'3000x'`, `'host:'`, `'host:port'`, `'a:b:80'`, `':'`, `'[::1]'`, `'80:'`) give None; `'[]:80'` gives host `[]`.
- T1 (interval): the eight golden values (`'250'`, `'250ms'`, `'2s'`, `'1m'`, `'1h'`, `'2.5s'`, `'.5s'`, `'0'`); `'1.2.3s'` -> 1200; `'5.'` -> 5; `'.s'` -> NaN; `'2S'`, `'2M'`, `'2H'`, `'5MS'`, `'5mS'` -> Undefined; `'abc'` and `'12sec'` -> Unchanged; `'1.5m'` -> 90000 and `'0.001h'` -> 3600 (float order).
- T1 (http-unix): the five golden splits incl. the `\\?\pipe\C:\app\sock` case and the legacy `'http://unix:/sock:foo'`; the four rejects give None; `'http-get://unix:/a-get:/b:/c'` -> `/a-get`, `/b:/c`.
- T2 (regex semantics): `'tcp:a\nb'`, `'tcp:a\u{2028}b'` -> type `file`; `'3000\n'`, `'3000\r'` -> None; `'[a\nb]:80'` -> host `a\nb`; `'http://unix:/a\nb:/c'` -> legacy split; `'http://unix:/a:/b\n'` -> None; `'2ſ'` -> Unchanged.
- T2 (never panics): a fixed list (`''`, `':'`, `'['`, `']'`, `'\n'`, `'http://unix:'`, `'http://unix::'`, `'-get:'`, a 10 kB string of `:` and `/`, `'ünïcödé:80'`, `'tcp:ünïcödé'`) runs through all four parsers without panicking.

**Verification.** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green; `cargo deny check` unchanged (no new crate); `npm test` unchanged.

### U2. napi parser exports and the differential oracle

**Goal.** The built addon answers `parsePrefix`, `parseHostPort`, `parseInterval`, `parseHttpUnix` with JS-comparable shapes, and `test/parser-properties.mocha.js` proves the Rust parsers equal the Node oracle on every golden, generated and junk input, failing (not skipping) under `rust-strict` when the addon is missing.

**Requirements.** R-L8-2, R-L8-3, R-L8-4.

**Dependencies.** U1.

**Files.** `crates/wait-on-napi/src/lib.rs` (appended structs and exports), `test/parser-properties.mocha.js` (generator and golden hoist, new `describe`, `DIFFERENTIAL_EXTRAS`, `genJunk`), `test/parser-properties-strict.mocha.js` only if T7 cannot reuse the subprocess helper pattern in `test/rust-pending.mocha.js` (prefer placing T7 in `test/parser-properties.mocha.js` itself, in its own `describe` outside the differential block, whose `before` would skip it without an addon).

**Approach.** KTD2, KTD3, KTD4.

1. T3 first against the current addon (red: `addon.parsePrefix is not a function`), then the four exports; rebuild with `npm run build:napi`.
2. Hoist generators and golden lists (existing tests green throughout).
3. Add the differential `describe` with T4, T5, T6 using one loop over a `[nodeName, addonName]` table so every input reaches all four parsers on both sides.
4. T7 as a mocha subprocess (pattern from `test/rust-pending.mocha.js`): run mocha on `test/parser-properties.mocha.js` with `--grep "Rust parsers match the Node oracle"` (so the child runs only the differential block and never re-runs T7 or the 300-run property suite) and `WAIT_ON_ENGINE=rust-strict` and `WAIT_ON_NATIVE_LIBRARY_PATH` set to a path that does not exist.

**Patterns to follow.** `describe('addon checks (real addon)')` in `test/engine-checks.mocha.js` (`before` skip, `resolveEngine` with explicit vars); `forAll` failure prefixing in `test/parser-properties.mocha.js`; the fixture-spec subprocess in `test/rust-pending.mocha.js`.

**Test scenarios** (`test/parser-properties.mocha.js`, describe "Rust parsers match the Node oracle (real addon)", unless noted).

- T3 "should export the four parsers and answer the hand-picked edge cases with the JS values": each of the four is a function; every `DIFFERENTIAL_EXTRAS` entry (KTD4) deep-strict-equals its concrete literal from the addon; `Object.keys(addon.parsePrefix('tcp:x'))` sorted is `['prefix', 'rest', 'type']`; `addon.parseInterval('5S')` is `undefined` (not `null`); `Number.isNaN(addon.parseInterval('.s'))`.
- T4 "should agree with the Node oracle on every golden and reject vector": every input from the hoisted golden and reject lists of all four parsers, through all four parsers on both sides, deep-strict-equal; failure message names the input.
- T5 "should agree with the Node oracle on every generated input": `forAll` over each hoisted generator (`genScheme`, `genUnprefixed`, `genAny`, `genHostPort`, `genBarePort`, `genIpv6`, `genInterval`, `genNonInterval`, `genUnixCase`), input through all four parsers on both sides.
- T6 "should agree with the Node oracle on junk including line terminators": `forAll(genJunk, ...)`, all four parsers both sides; runs count from `WAITON_TEST_RUNS`.
- T7 "should fail, not skip, under rust-strict when the addon cannot load": the mocha subprocess exits non-zero and its output includes `failed to load the native addon`; under `js` with the same missing path the file's differential block reports pending instead.

**Verification.** After `npm run build:napi`, T3-T6 run (not skipped) and pass; T7 passes without an addon; `npm test` green with T3-T6 skipped when no prebuild exists; `npm run ci:rs` green with 0 pending in the parser file's differential block.

### U3. One timing tolerance for both conformance suites

**Goal.** Every elapsed-time assertion in both conformance suites goes through `expectElapsed` with `TOLERANCE_MS`, adding an upper bound, and the helper's bounds are pinned by self-tests.

**Requirements.** R-L8-5, R-L8-6.

**Dependencies.** None.

**Files.** `test/helpers/cli-conformance.js` (`TOLERANCE_MS`, `expectElapsed`, comment update), `test/cli-conformance-helper.mocha.js` (T8), `test/cli-conformance.mocha.js` and `test/cli-conformance-http.mocha.js` (T9: replace the eight bounds).

**Approach.** KTD5. T8 first (red: `expectElapsed` is not a function), helper, then replace each `expect(r.elapsedMs).to.be.at.least(...)` with the helper call (`APPEAR` for the two "later" vectors, `T` for the six "times out" vectors). The helper file's "sized so a vector's worst case stays under mocha's 2000 ms default" comment gains the `T + late < 2000` relation. Plain `it` on the real clock (AGENTS.md clock rule).

**Patterns to follow.** `describe('runCli')` in `test/cli-conformance-helper.mocha.js` (fake results are enough for bound tests, no spawn needed).

**Test scenarios.**

- T8 (`test/cli-conformance-helper.mocha.js`, describe "expectElapsed"): `TOLERANCE_MS` deep-equals `{ early: 100, late: 1000 }` and is frozen; `expectElapsed({ elapsedMs: 700 }, 800)` and `({ elapsedMs: 1800 }, 800)` pass (inclusive bounds); `({ elapsedMs: 699 }, 800)` throws with a message containing `699`, `800` and `700`; `({ elapsedMs: 1801 }, 800)` throws with a message containing `1801` and `1800`.
- T9 (both conformance suites): the two "becomes available later" vectors call `expectElapsed(r, APPEAR)` and the six "times out" vectors call `expectElapsed(r, T)`; no `at.least` or `below` on `elapsedMs` remains in either file; the suites pass under `npm test` and `npm run ci:rs` on ubuntu, macos and windows. Seen red by the `late = 0` inversion (Risks).

**Verification.** `npm test` green; `npm run ci:rs` green locally; PR `rust` rows green on all three OSes; grep for `elapsedMs` in the two suites shows only `expectElapsed` calls.

### U4. Startup benchmark, baseline and the `ci:rs` step

**Goal.** `npm run bench:startup` prints both medians and the overhead and exits non-zero when the Rust addon's overhead over JS exceeds the committed threshold or when the Rust engine cannot load; `npm run ci:rs` runs it last.

**Requirements.** R-L8-8, R-L8-9, R-L8-10.

**Dependencies.** None (needs a host prebuild to run green locally; `ci:rs` builds one).

**Files.** `scripts/bench-startup.js` (new), `benchmarks/startup-baseline.json` (new), `scripts/ci-rs.js` (one step), `package.json` (`bench:startup`), `test/scripts.mocha.js` (T10, T11, T12).

**Approach.** KTD6, KTD7.

1. T10 against the missing module (red), then `median`, `verdict`, `withRecording`.
2. T11 (red: step list mismatch), then the appended step.
3. T12 (red: script missing or exits 0), then `main` with the listener, warm-up, interleaved timed spawns, verdict and `--record`.
4. Commit `benchmarks/startup-baseline.json` with the threshold and an empty `recorded` first; U5 records the host.

**Patterns to follow.** `scripts/ci-rs.js` (`spawnSync`, `process.execPath`, no shell, exit-code propagation); `scripts/build-napi.js` (`parseArgs`, `require.main === module`, exported pure functions); `describe('ci:rs')` in `test/scripts.mocha.js`.

**Test scenarios** (`test/scripts.mocha.js`).

- T10 "median of an odd, an even and an unsorted list" (`[3, 1, 2]` -> 2, `[4, 1, 3, 2]` -> 2.5, `[7]` -> 7).
- T10 "verdict passes when overhead is under the relative allowance" (`jsMs: 200, rustMs: 240`, `relative: 0.25, floorMs: 50` -> ok, `allowedMs` 50, `overheadMs` 40).
- T10 "verdict passes under the absolute floor when the relative allowance is smaller" (`jsMs: 100, rustMs: 145` -> ok, `allowedMs` 50).
- T10 "verdict fails past both with a message naming the numbers" (`jsMs: 100, rustMs: 160` -> not ok; message contains `60`, `50`, `100`, `160`).
- T10 "withRecording replaces only the host key and keeps others" (baseline with `linux-x64` recorded, add `darwin-arm64` -> both present, threshold untouched).
- T11 "runs the benchmark after mocha as the last step": the pinned step list in `describe('ci:rs')` gains `[process.execPath, path.join(repoRoot, 'scripts', 'bench-startup.js')]` at the end; `process.execPath` check still holds for every non-cargo step.
- T12 "exits non-zero naming the addon when the Rust engine cannot load": spawn `node scripts/bench-startup.js --runs 1` with `WAIT_ON_NATIVE_LIBRARY_PATH` set to a missing file; exit code non-zero; output includes `rust-strict` and the missing path; completes within the test's 10 s timeout.

**Verification.** `npm test` green (T10-T12 need no prebuild); after `npm run build:napi`, `npm run bench:startup` exits 0 and prints both medians, overhead and allowance; `npm run bench:startup -- --record` rewrites the host entry; `npm run ci:rs` green with the benchmark as its final output block.

### U5. Guides and the recorded baseline

**Goal.** The guides describe the differential suite, the tolerance and its rationale, the benchmark, refreshing the baseline and reproducing a failing seed; the baseline carries the author's host entry.

**Requirements.** R-L8-7, R-L8-11, R-L8-12.

**Dependencies.** U2, U3, U4.

**Files.** `docs/guides/testing.md` (suite table rows for `parser-properties` and `scripts`; new sections "Timing tolerance" with `TOLERANCE_MS`, the rationale from KTD5, that it is the R5/R17 promote criterion and the rule for moving it; "Parser differential" with the skip/must-run rule and `WAITON_TEST_SEED`/`WAITON_TEST_RUNS` reproduction; "Startup benchmark" with what is spawned, the gate formula, `npm run bench:startup`, `--record`, `--runs`), `docs/guides/ci.md` (`ci:rs` row: benchmark step and that it fails the job), `docs/guides/development.md` (commands table: `bench:startup`; `ci:rs` row mentions the benchmark), `docs/guides/architecture.md` (one bullet: parsers (L8) exist in `wait_on_core::parse` and the addon for differential testing only; runtime parsing stays JS until L7), `benchmarks/startup-baseline.json` (host entry from `--record`).

**Approach.** Docs-only edits per KD-S9 and the docs-as-done checklist in `docs/guides/contributing-dual-engine.md`; then run `--record` on the host and commit the entry. `test/rust-pending.js` untouched.

**Patterns to follow.** The L3 and L6 bullets in `architecture.md`; the "Running under each engine" section in `testing.md`.

**Test scenarios.**

- Test expectation: none for the guide text (docs-only). Docs check: every page names only what is true on merge; no guide says the tolerance is per-vector or that the benchmark compares against another runner.
- Baseline check: `benchmarks/startup-baseline.json` parses, `recorded` has the host key with numeric `jsMs`, `rustMs`, `overheadMs`, and `allowedMs` for that entry is at least twice `overheadMs` (Assumptions rule); T12 continues to pass.

**Verification.** `npm test`, `npm run ci:rs` green; PR `rust` rows green; the docs-as-done checklist ticked in the PR body.

---

## Verification Contract

Run from the repo root; all must pass before the PR opens, and again after merging `origin/spike-next-rs`.

- `npm test`: lint (covers `scripts/**/*.js`), types, mocha under JS; outcome unchanged apart from the new tests (T7, T8, T10-T12 always; T3-T6 skip without a host prebuild and run when one exists).
- `npm run test:coverage`: `.nycrc.json` thresholds unchanged (no `lib/` or `bin/` change).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (T1, T2), `cargo deny check` (no new crate).
- `npm run build:napi`, then T3-T6 run (not skipped) and `npm run bench:startup` exits 0.
- `npm run ci:rs`: green; mocha under `rust-strict` shows the differential block executed with 0 pending; the benchmark block is the last output and reports both medians.
- `npm pack --dry-run`: no `benchmarks/` or `scripts/` entry.
- CI on the PR: every check green, including the `rust` rows on ubuntu, macos and windows (T9 and the benchmark on each), commitlint and PR title.

Parser matrix (parser x input class); every cell runs on both engines because each differential test feeds both sides, and the OS axis is covered by `ci:rs` on three runners:

| Input class | prefix | host:port | interval | http-unix |
|---|---|---|---|---|
| golden and rejects (fixed) | T1, T4 | T1, T4 | T1, T4 | T1, T4 |
| generated (own generator) | T5 `genScheme`, `genUnprefixed`, `genAny` | T5 `genHostPort`, `genBarePort`, `genIpv6` | T5 `genInterval`, `genNonInterval` | T5 `genUnixCase` |
| generated (other parsers' inputs) | T5 cross-feed | T5 cross-feed | T5 cross-feed | T5 cross-feed |
| junk incl. line terminators | T6, T2 | T6, T2 | T6, T2 | T6, T2 |
| hand-picked semantics extras | T3 | T3 | T3 | T3 |
| addon missing under `rust-strict` | T7 | T7 | T7 | T7 |

Timing matrix (engine x vector class); OS via CI rows:

| Engine | appears later (`APPEAR`) | times out (`T`) | helper bounds |
|---|---|---|---|
| `js` (`npm test`) | T9 file, http | T9 file, tcp, socket, http 404, http never, http-get never | T8 |
| `rust-strict` (`ci:rs`) | T9 same vectors | T9 same vectors | T8 |

Benchmark matrix: `verdict` cells (under relative, under floor, over both) T10; step position T11; Rust load failure T12; the real run is the `ci:rs` step on each OS.

---

## Definition of Done

- R-L8-1..R-L8-12 met; T1..T12 present, named for behavior, each seen red for the right reason before green (T4-T6 and T9 by the inversions noted in Risks).
- Verification Contract fully green locally and on the PR.
- `test/rust-pending.js` still `[]`; no `.only`/`.skip` in the diff (conditional `this.skip()` in the differential `before` only).
- `lib/`, `bin/`, `index.d.ts`, `WAIT_ON_SCHEMA` untouched; no `.github/workflows/` change; no new npm dependency; no new crate in `Cargo.toml`/`Cargo.lock`.
- No numeric bound on `elapsedMs` outside `test/helpers/cli-conformance.js`; `TOLERANCE_MS` and its rationale in `docs/guides/testing.md` match the code.
- `benchmarks/startup-baseline.json` committed with the threshold and the host entry; `ci:rs` runs the benchmark last.
- Guides updated per U5; `docs/plans/**` never deleted; this plan's Resume notes appended; spine and sibling plans untouched.
- Cleanup: no abandoned-attempt code (no `regex` crate, no JSON-string exports, no second differential file, no per-vector tolerances, no `file:`-based bench variant); no generated files outside gitignored paths; no listener or child process left open by T12 or the bench.
- `origin/spike-next-rs` merged (no rebase, no force-push) before ready; PR opened against `spike-next-rs` with `Closes #60`; not merged by the lane.

---

## Resume notes
