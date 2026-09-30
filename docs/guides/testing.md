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
| `test/cli-conformance-helper.mocha.js` | self-check for the spawn harness `test/helpers/cli-conformance.js` |
| `test/parser-properties.mocha.js` | property tests for the four pure parsers (seeded PRNG, override with `WAITON_TEST_SEED`) |
| `test/validation.mocha.js` | schema and resource syntax |
| `test/https-proxy.mocha.js` | TLS client options, proxy, unix socket + proxy |
| `test/coverage.mocha.js` | branches the main suites miss (validation errors, dispatcher options, verbose, CLI help) |
| `test/native-helpers.mocha.js` | pure helpers exposed via `_internal` |
| `test/engine.mocha.js` | `WAIT_ON_ENGINE` selection, fallback and errors (API and CLI), prebuild path resolution, `file:` probe routing and `command:` check routing (stub addon), the real addon's `fileSize` and `runCommand`, and that slow commands never starve a `file:` probe, http routing to the addon (counting fixture), real-addon `HttpChecker`, process lifetime with a hung http server |
| `test/engine-checks.mocha.js` | tcp/socket dispatch to the addon: fixture addon (always runs) and real addon (skips without a host prebuild) |
| `test/benchmarks.mocha.js` | `benchmarks/http-ffi.js` helpers and smoke runs |
| `test/rust-pending.mocha.js` | the Rust pending list hooks (fixture specs in a mocha subprocess) |
| `test/scripts.mocha.js` | planning functions behind `build:napi` and `ci:rs` |
| `test/rust-scaffold.mocha.js` | toolchain pin equals the workspace MSRV; `Cargo.lock` committed |
| `crates/wait-on-core` `#[test]`s | Rust unit tests (`cargo test --workspace`); `run_command` cases use per-OS shell builtins (`#[cfg(unix)]`/`#[cfg(windows)]`) so no `node` is needed |
| `test/types.test-d.ts` | `index.d.ts` type tests (`npm run test:types`) |

Shared fixtures: `test/config-http-resources.js`, `test/config-headers.js`, `test/config-status-codes.js`. Engine fixtures live under `test/fixtures/` (outside the `*.mocha.js` glob): `fake-addon.js` stands in for the native addon via `WAIT_ON_NATIVE_LIBRARY_PATH`. Its `fileSize` records each probed path in `calls` and answers a constant (`WAIT_ON_FAKE_FILE_SIZE`, default `1`) that JS cannot produce for a missing file, so a success proves the Rust route ran. Its `runCommand` records `{ command, timeoutMs }` in the same `calls` and answers `ok: true` without spawning (`ok: false` when `WAIT_ON_FAKE_COMMAND_OK=0`), so a failing command succeeding, or a passing one satisfying reverse mode, proves the same. `fake-addon-checks.js` extends it with `tcpCheck`/`socketCheck`, answers per `WAIT_ON_FAKE_ADDON_ANSWER` (`ready`, `refused`, `timeout`), records calls in `module.exports.calls` and appends them to the file in `WAIT_ON_FAKE_ADDON_LOG` (CLI subprocess proof). `counting-addon.js` records every `HttpChecker` construction and `check` call and delegates to the real prebuild when one exists (a canned answer otherwise), so routing tests prove which engine served a request. `hung-http-api.js` is the API script the process-lifetime test spawns. `test/helpers/engine-env.js` has `withEnv` / `runCLI`.

## Conformance and property tests

The CLI conformance suites are language-agnostic spawn tests and `parser-properties` is shaped as the Node oracle; both are the contract the Rust engine must match.

## Coverage gate

`npm run test:coverage` runs nyc over `lib/**/*.js` and `bin/wait-on` and fails below the thresholds in [`.nycrc.json`](../../.nycrc.json) (branches 95, lines 98, functions 94, statements 97).

## Fake clock

[`test/frozen-clock.js`](../../test/frozen-clock.js) exports `FROZEN_NOW`, `itFrozen`, and `mochaHooks`.

- `itFrozen` virtualizes rxjs scheduling via `intervalProvider.delegate` and freezes global `Date` at `FROZEN_NOW` (2026-06-17T12:00:00Z); global timers stay real so undici and net teardown run normally.
- Opt-in per test. Use it for fixed-state tests (timeouts, file stability window).
- Stay on the real clock (plain `it`) for `command:` resources, `tcpTimeout`/`httpTimeout`, socket teardown, resources changed by a real `setTimeout`, and CLI subprocess tests.
- Don't use node:test `mock.timers`: it breaks rxjs on Node 22.19.
- Use `FROZEN_NOW`, never bare `new Date()`, in fixtures.

## Windows notes

- No unix domain sockets: tests use named pipes (`\\?\pipe\...`) on `win32`.
- Delete-pending files can keep `fs.stat` succeeding after unlink; reverse-file tests use headroom and retry, not `lib/` changes.
- Cert generation shells out to `openssl` and is slow on Windows runners; skip (don't fail) when a tool is missing.
- No POSIX-only shell or tooling in tests.

## Running under each engine

The same suites run once per engine: `npm test` (CI `build` job) runs them under JS, and `npm run ci:rs` (CI `rust` job, ubuntu/macos/windows) builds the host addon and runs them again with `WAIT_ON_ENGINE=rust-strict`. The env var reaches CLI subprocess tests through the inherited environment. Tests that pin a particular engine (`test/engine.mocha.js`) set and restore `WAIT_ON_ENGINE` themselves, so they pass under either run. The real-addon case skips when no host prebuild exists and always runs under `ci:rs`. Tests that must prove a Rust check ran use the real clock (`it`, not `itFrozen`): the frozen pump reaches a virtual timeout before a cross-thread check settles, so a frozen test can pass without the Rust path having answered.

**Proving the Rust path ran.** A passing tcp/socket test alone could be the JS fallback. The fixture suite asserts the fixture saw the call; the real-addon suite spies on the export of the addon object `resolveEngine` returns, and the CLI case checks for `(os error` in `--verbose` output (Rust's reason text; Node prints `ECONNREFUSED`). The #82 `TCPSocketWrap` leak test passes trivially under Rust (no Node handle is opened); the #82 close tests, where the server sees the close, are the real guard.

## Rust pending list

[`test/rust-pending.js`](../../test/rust-pending.js) lists tests that cannot pass on Rust yet, by mocha full title (describe titles and test title, space-joined). It is empty today (L2 moved `tcp:`/`socket:` to Rust with no entries). Its root hooks are composed into `test/frozen-clock.js`, the root-hook plugin `.mocharc.json` already loads. Under `WAIT_ON_ENGINE=rust` or `rust-strict`, a listed test reports pending, and a listed title that no suite registers fails the run so stale entries cannot pile up. Under JS the list has no effect. Each lane shrinks it; it must be empty before the spike PR leaves draft.
