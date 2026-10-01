# Testing

## Rule

Strict TDD, test-first, no exceptions beyond docs. The rule, cycle, and anti-patterns live in [AGENTS.md](../../AGENTS.md#test-driven-development-mandatory); don't restate them here.

## Suites

mocha + chai, `test/**/*.mocha.js`, run with `npm run test:mocha` (`--exit` is required: test servers leave open handles). [`.mocharc.json`](../../.mocharc.json) requires `test/frozen-clock.js`.

| File | Covers |
|---|---|
| `test/api.mocha.js` | `waitOn` API: every resource type, reverse, options |
| `test/cli.mocha.js` | `bin/wait-on` as a subprocess |
| `test/cli-conformance.mocha.js` | black-box CLI vectors: file, tcp, socket, args, config-file precedence |
| `test/cli-conformance-http.mocha.js` | black-box CLI vectors: http(s), http(s)-get, http-over-unix |
| `test/cli-conformance-helper.mocha.js` | self-check for the spawn harness `test/helpers/cli-conformance.js`, including the `expectElapsed` tolerance bounds |
| `test/parser-properties.mocha.js` | property tests for the four pure parsers (seeded PRNG, override with `WAITON_TEST_SEED`), and the Rust-vs-JS differential over the same vectors (real addon) |
| `test/validation.mocha.js` | schema and resource syntax |
| `test/https-proxy.mocha.js` | TLS client options (roots, mTLS, encrypted keys, http→https redirects), proxy object and env-proxy routing through a counting stub proxy, unix socket + proxy; runs on both engines and, under `rust*`, asserts through the counting addon that the wait made one `wait` call (or, for JS carve-outs, `routed: false`, none) |
| `test/coverage.mocha.js` | branches the main suites miss (validation errors, dispatcher options, verbose, CLI help) |
| `test/native-helpers.mocha.js` | pure helpers exposed via `_internal` |
| `test/engine.mocha.js` | `WAIT_ON_ENGINE` selection, fallback and errors (API and CLI), the `wait`-less addon as a load failure, prebuild path resolution, the real addon's `fileSize`, `runCommand` and `HttpChecker`, the real addon's `wait` (lines, timeout message, absent optional fields, unknown kind), http dispatch and the whole-wait carve-outs (counting fixture), the Rust shim's spec and timeout delivery (fake addon, API and CLI), module graph per engine, process lifetime with a hung http server, `NODE_EXTRA_CA_CERTS` |
| `test/engine-checks.mocha.js` | tcp under the real addon (skips without a host prebuild): Rust's `--verbose` reason text (CLI), a spec napi rejects (port above 65535) reaching the callback, the overall timeout firing while a connect is pending |
| `test/benchmarks.mocha.js` | `benchmarks/http-ffi.js` helpers and smoke runs |
| `test/rust-pending.mocha.js` | the Rust pending list hooks (fixture specs in a mocha subprocess) |
| `test/scripts.mocha.js` | planning functions behind `build:napi` and `ci:rs`, and the `bench:startup` median/verdict/record functions and its fail-loud path |
| `test/rust-scaffold.mocha.js` | toolchain pin equals the workspace MSRV; `Cargo.lock` committed |
| `crates/wait-on-core` `#[test]`s | Rust unit tests (`cargo test --workspace`), including `src/waiter/tests.rs` (loop schedule, stabilization, concurrency, timeout and line text under a paused clock); `run_command` cases use per-OS shell builtins (`#[cfg(unix)]`/`#[cfg(windows)]`) so no `node` is needed |
| `crates/wait-on-core/tests/` | integration tests: `loop.rs` runs `waiter::wait` end to end on real files, tcp listeners, unix sockets / named pipes, shell commands and scripted http servers under a paused clock (`#[tokio::test(start_paused = true)]` with `advance`) |
| `test/types.test-d.ts` | `index.d.ts` type tests (`npm run test:types`) |

Shared fixtures: `test/config-http-resources.js`, `test/config-headers.js`, `test/config-status-codes.js`. Engine fixtures live under `test/fixtures/` (outside the `*.mocha.js` glob): `fake-addon.js` stands in for the native addon via `WAIT_ON_NATIVE_LIBRARY_PATH`: its `wait(spec)` records the spec in `calls` and answers ready, or with `WAIT_ON_FAKE_ADDON_ANSWER=timeout` the Rust timeout result naming every resource; with `WAIT_ON_FAKE_ADDON_LOG` set it appends each spec as one JSON line (CLI subprocess proof). It runs no check, so it exercises the shim and dispatch, not the loop. `counting-addon.js` records every call as `{ type: 'wait', spec, validateStatus }` and delegates to the real prebuild when one exists (otherwise a canned answer: ready unless `validateStatus(200)` is false for an http resource); setting its `constructError` makes the next calls reject. Routing tests use it to prove which engine ran the wait. `hung-http-api.js` is the API script the process-lifetime test spawns; `extra-ca-api.js` is the `NODE_EXTRA_CA_CERTS` subprocess script. `test/helpers/engine-env.js` has `withEnv` / `runCLI`. `test/helpers/tls-fixture.js` generates the shared TLS material with `openssl` (a SAN, `CA:FALSE` self-signed leaf both engines can verify, an unrelated second leaf, a PKCS#8-encrypted key and its passphrase; `null` when `openssl` is missing, so callers skip). `test/helpers/stub-proxy.js` is a counting proxy: it forwards absolute-form requests and tunnels `CONNECT`, recording each request line and `proxy-authorization`, so proxy tests prove the path taken and direct tests assert zero counts.

## Conformance and property tests

The CLI conformance suites are language-agnostic spawn tests and `parser-properties` is shaped as the Node oracle; both are the contract the Rust engine must match.

### Timing tolerance

Every elapsed-time assertion in `test/cli-conformance.mocha.js` and `test/cli-conformance-http.mocha.js` goes through `expectElapsed(result, expectedMs)` from `test/helpers/cli-conformance.js`, which checks `[expectedMs - early, expectedMs + late]` with `TOLERANCE_MS = { early: 100, late: 1000 }`. Vectors where a resource appears later expect `APPEAR` (250 ms); timeout vectors expect `T` (800 ms). No other numeric bound on `elapsedMs` exists in those suites.

- `early` covers timer clamping only: `elapsedMs` starts before the child's own timers, so a child cannot legitimately finish before `expectedMs`.
- `late` covers node startup plus module load (and addon load under `rust-strict`) on the slowest CI row. It is additive, not a percentage, because the noise is startup, not proportional to the wait. `T + late = 1800` stays under mocha's 2000 ms default; raising `late` means raising a per-suite `this.timeout` in both suites together, and updating this section.
- The same numbers apply to both engines (`npm test` and `npm run ci:rs`). This tolerance is the R5/R17 timing-parity promote criterion: a Rust build that cannot meet it on every CI OS is not promotable.

### Parser differential

The last `describe` in `test/parser-properties.mocha.js` feeds every golden, reject and generated input (plus a junk generator with line terminators and a hand-picked list of JS regex edge cases) through the four addon parsers (`parsePrefix`, `parseHostPort`, `parseInterval`, `parseHttpUnix`) and the Node oracle, and asserts deep-strict-equal results.

- It skips when no host prebuild exists and `WAIT_ON_ENGINE` is not `rust-strict`, so `npm test` without a Rust toolchain stays green. Under `rust-strict` (`ci:rs`) it never skips: a missing or stale addon fails the run (a test spawns mocha with a missing addon path to pin this).
- A failure names the seed, run index and input. Reproduce with `WAITON_TEST_SEED=<seed> npx mocha --exit test/parser-properties.mocha.js`; `WAITON_TEST_RUNS=<n>` raises the run count (default 300).
- On a mismatch the Rust side changes; the JS parsers are the oracle.

## Startup benchmark

`npm run bench:startup` (`scripts/bench-startup.js`, needs a host prebuild) starts a local TCP listener and spawns `node bin/wait-on tcp:127.0.0.1:<port>` under `WAIT_ON_ENGINE=js` and `rust-strict`, one untimed warm-up each, then `runs` timed spawns per engine, interleaved. It fails when `median(rust) - median(js)` exceeds `max(relative * median(js), floorMs)`, or when the Rust engine cannot load. A tcp resource is used because it has no stability window, so the numbers are startup plus one check.

- Threshold and run count live in [`benchmarks/startup-baseline.json`](../../benchmarks/startup-baseline.json) (`relative: 0.25`, `floorMs: 50`, `runs: 20`). The gate compares the two engines in the same run, so it does not depend on the runner's speed.
- `recorded` in that file holds reference medians per `<platform>-<arch>`; they are the record, not the gate. Refresh this host's entry with `npm run bench:startup -- --record` and commit it. `--runs N` overrides the run count.
- `npm run ci:rs` runs it as its last step, after mocha, on ubuntu, macos and windows.

## Coverage gate

`npm run test:coverage` runs nyc over `lib/**/*.js` and `bin/wait-on` and fails below the thresholds in [`.nycrc.json`](../../.nycrc.json) (branches 95, lines 98, functions 94, statements 97).

## Fake clock

[`test/frozen-clock.js`](../../test/frozen-clock.js) exports `FROZEN_NOW`, `itFrozen`, and `mochaHooks`.

- `itFrozen` virtualizes rxjs scheduling via `intervalProvider.delegate` and freezes global `Date` at `FROZEN_NOW` (2026-06-17T12:00:00Z); global timers stay real so undici and net teardown run normally.
- Opt-in per test. Use it for fixed-state tests (timeouts, file stability window).
- Stay on the real clock (plain `it`) for `command:` resources, `tcpTimeout`/`httpTimeout`, socket teardown, resources changed by a real `setTimeout`, and CLI subprocess tests.
- Don't use node:test `mock.timers`: it breaks rxjs on Node 22.19.
- Use `FROZEN_NOW`, never bare `new Date()`, in fixtures.
- The pump cannot advance a Rust timer: under `rust*` an `itFrozen` test runs the wait in real time within its budget (raise a `describe` budget, never add a pending entry). The one test of the mechanism itself (`test/frozen-clock.mocha.js`, 5000 ms virtual in under 2000 ms real) pins `WAIT_ON_ENGINE: 'js'` with `withEnv`.
- Rust loop tests use tokio's paused clock instead (`start_paused = true`, `advance`).

## Windows notes

- No unix domain sockets: tests use named pipes (`\\?\pipe\...`) on `win32`.
- Delete-pending files can keep `fs.stat` succeeding after unlink; reverse-file tests use headroom and retry, not `lib/` changes.
- Cert generation shells out to `openssl` and is slow on Windows runners; skip (don't fail) when a tool is missing.
- No POSIX-only shell or tooling in tests.

## Running under each engine

The same suites run once per engine: `npm test` (CI `build` job) runs them under JS, and `npm run ci:rs` (CI `rust` job, ubuntu/macos/windows) builds the host addon and runs them again with `WAIT_ON_ENGINE=rust-strict`. The env var reaches CLI subprocess tests through the inherited environment. Tests that pin a particular engine (`test/engine.mocha.js`) set and restore `WAIT_ON_ENGINE` themselves, so they pass under either run. The real-addon case skips when no host prebuild exists and always runs under `ci:rs`. Tests that must prove the Rust wait ran use the real clock (`it`, not `itFrozen`).

**Proving the Rust path ran.** A passing test alone could be the JS fallback. A Rust wait is exactly one `wait` call, so routing tests load `counting-addon.js` (or `fake-addon.js`) and assert one recorded call, or zero for a whole-wait carve-out; CLI subprocess tests read `WAIT_ON_FAKE_ADDON_LOG`. Under the real addon, `--verbose` output carries Rust's text (e.g. `(os error` where Node prints `ECONNREFUSED`). The #82 `TCPSocketWrap` leak test passes trivially under Rust (no Node handle is opened); the #82 close tests, where the server sees the close, are the real guard.

## Rust coverage

Loop code in `crates/wait-on-core` (`waiter.rs` and every file it touches) is held to 100% line and region coverage by its own cargo tests; an uncovered region is a missing test or a branch to delete, never a follow-up. The napi crate stays a thin mapping checked by the real-addon mocha tests ([architecture.md](architecture.md#polling-loop-l7)). Measure locally (prerequisites in [development.md](development.md#prerequisites)), excluding the test bodies:

```bash
cargo llvm-cov -p wait-on-core --summary-only --ignore-filename-regex '(waiter/tests\.rs|/tests/)'
```

CI does not enforce the number yet; that lands in lane L13.

## Rust pending list

[`test/rust-pending.js`](../../test/rust-pending.js) lists tests that cannot pass on Rust yet, by mocha full title (describe titles and test title, space-joined). It is empty today (L2 moved `tcp:`/`socket:` to Rust with no entries). Its root hooks are composed into `test/frozen-clock.js`, the root-hook plugin `.mocharc.json` already loads. Under `WAIT_ON_ENGINE=rust` or `rust-strict`, a listed test reports pending, and a listed title that no suite registers fails the run so stale entries cannot pile up. Under JS the list has no effect. Each lane shrinks it; it must be empty before the spike PR leaves draft.
