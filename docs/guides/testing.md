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
| `test/engine.mocha.js` | `WAIT_ON_ENGINE` selection, fallback and errors (API and CLI), the `wait`-less addon as a load failure, the real prebuild's export list and `version()`, http dispatch, the whole-wait carve-outs and `routesHttpToRust` (counting addon), the real-addon loop at the front door (timeout message identical to JS, a spec napi throws on reaching the callback, verbose tcp and socket lines), the Rust shim's spec shape and timer clamps (counting addon), module graph per engine, process lifetime with a hung http server, `NODE_EXTRA_CA_CERTS`, prebuild path resolution |
| `test/benchmarks.mocha.js` | `benchmarks/http-ffi.js` helpers and smoke runs |
| `xtask` `#[test]`s | `cargo test -p xtask`: unit tests in `xtask/src/*.rs` (`ci` step order, `build-napi` planning, `package` guards, `bench-startup` median/verdict/record); binary tests in `xtask/tests/cli.rs` (`--help`, dispatch, fail-loud paths, `hooks` in a temp repo); `xtask/tests/commit_msg_hook.rs` (good/bad message table run through `git hook run` against `.githooks/commit-msg`; skips without git 2.36+ locally, fails on CI); `xtask/tests/commitlint_config.rs` (`commitlint.config.js` ignores evaluated by node). Needs `node` on `PATH`, the `npm pack` test skips unless run through npm |
| `test/prebuild-probe.mocha.js` | `xtask/assets/prebuild-probe.js`, the probe `ci:rs:package` runs inside installed packages, against the real host prebuild (skips without one) |
| `test/rust-scaffold.mocha.js` | toolchain pin equals the workspace MSRV and carries `llvm-tools-preview`; the `ci:rs` string, coverage gate included; `Cargo.lock` committed |
| `crates/wait-on-core` `#[test]`s | Rust unit tests (`cargo test --workspace`): inline `#[cfg(test)]` modules in `src/{lib,tcp,socket,http,parse}.rs` and `src/waiter/tests.rs` (loop schedule, stabilization, concurrency, timeout and line text under a paused clock, `Resource::from_parts`); `run_command` cases use per-OS shell builtins (`#[cfg(unix)]`/`#[cfg(windows)]`) so no `node` is needed |
| `crates/wait-on-core/tests/loop.rs` | `waiter::wait` end to end across kinds: http result lines, `validateStatus` false and erroring, settle aborting in-flight checks, `command:` under `commandTimeout`, the mixed-kind waiting lines |
| `crates/wait-on-core/tests/file.rs` | `file:` forward and reverse through `wait`: present, `window` stabilization, missing, removed |
| `crates/wait-on-core/tests/tcp.rs` | `tcp:` forward and reverse: listening, `[::1]` literal, overall timeout while a connect is pending, refused |
| `crates/wait-on-core/tests/socket.rs` | `socket:` forward and reverse: unix socket, or named pipe under `cfg(windows)` |
| `crates/wait-on-core/tests/command.rs` | `command:` forward and reverse: exit 0, non-zero exit |
| `crates/wait-on-core/tests/http.rs` | plain http HEAD and GET: status handling, headers verbatim, redirects on and off, reverse |
| `crates/wait-on-core/tests/https.rs` | https as Rust receives it: `roots` (right, wrong, absent, garbage), mTLS `cert` + `key`, http→https redirect, reverse |
| `crates/wait-on-core/tests/proxy.rs` | explicit `proxy` URI: `CONNECT` tunnel for an https target (counted), userinfo as Basic auth, proxy down under reverse |
| `crates/wait-on-core/tests/unix_http.rs` | http over a unix socket (`cfg(unix)`) or named pipe (`cfg(windows)`), the proxy bypassed, missing socket forward and reverse |
| `test/types.test-d.ts` | `index.d.ts` type tests (`npm run test:types`) |

Shared fixtures: `test/config-http-resources.js`, `test/config-headers.js`, `test/config-status-codes.js`. Engine fixtures live under `test/fixtures/` (outside the `*.mocha.js` glob). `counting-addon.js` is the only addon stand-in, loaded via `WAIT_ON_NATIVE_LIBRARY_PATH` as the routing spy: it records every call as `{ type: 'wait', spec, validateStatus }` and delegates to the real prebuild when one exists (otherwise a canned answer: ready unless `validateStatus(200)` is false for an http resource, which keeps `lib/engine-rust.js` under nyc without a Rust toolchain); setting its `constructError` makes the next calls reject. `hung-http-api.js` is the API script the process-lifetime test spawns; `extra-ca-api.js` is the `NODE_EXTRA_CA_CERTS` subprocess script. `test/helpers/engine-env.js` has `withEnv` / `runCLI`. `test/helpers/tls-fixture.js` generates the shared TLS material with `openssl` (a SAN, `CA:FALSE` self-signed leaf both engines can verify, an unrelated second leaf, a PKCS#8-encrypted key and its passphrase; `null` when `openssl` is missing, so callers skip). `test/helpers/stub-proxy.js` is a counting proxy: it forwards absolute-form requests and tunnels `CONNECT`, recording each request line and `proxy-authorization`, so proxy tests prove the path taken and direct tests assert zero counts.

Rust fixtures: `crates/wait-on-core/tests/common/mod.rs` holds the shared helpers (`recorder`, `spec`, `temp`, `settle`, `breathe`, `closed_port`, scripted http `serve`, `tls` server config on `rustls`, the counting `proxy`, `listening_socket`, `socket_server`, `first_check`, `ready`/`times_out`); `crates/wait-on-core/tests/fixtures/*.pem` is the committed TLS material ([regeneration](#rust-coverage)).

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

`npm run bench:startup` (`cargo xtask bench-startup`, needs a host prebuild) starts a local TCP listener and spawns `node bin/wait-on tcp:127.0.0.1:<port>` under `WAIT_ON_ENGINE=js` and `rust-strict`, one untimed warm-up each, then `runs` timed spawns per engine, interleaved. It fails when `median(rust) - median(js)` exceeds `max(relative * median(js), floorMs)`, or when the Rust engine cannot load. A tcp resource is used because it has no stability window, so the numbers are startup plus one check.

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
- The pump cannot advance a Rust timer: under `rust*` an `itFrozen` test runs the wait in real time within its budget (raise a `describe` budget). The one test of the mechanism itself (`test/frozen-clock.mocha.js`, 5000 ms virtual in under 2000 ms real) pins `WAIT_ON_ENGINE: 'js'` with `withEnv`.
- Rust loop tests use tokio's paused clock instead (`start_paused = true`, `advance`).

## Windows notes

- No unix domain sockets: tests use named pipes (`\\?\pipe\...`) on `win32`.
- Delete-pending files can keep `fs.stat` succeeding after unlink; reverse-file tests use headroom and retry, not `lib/` changes.
- Cert generation shells out to `openssl` and is slow on Windows runners; skip (don't fail) when a tool is missing.
- No POSIX-only shell or tooling in tests.

## Running under each engine

The same suites run once per engine: `npm test` (CI `build` job) runs them under JS, and `npm run ci:rs` (CI `rust` job: ubuntu and windows on PRs, plus macOS on push) builds the host addon and runs them again with `WAIT_ON_ENGINE=rust-strict`. The env var reaches CLI subprocess tests through the inherited environment. Tests that pin a particular engine (`test/engine.mocha.js`) set and restore `WAIT_ON_ENGINE` themselves, so they pass under either run. The real-addon case skips when no host prebuild exists and always runs under `ci:rs`. Tests that must prove the Rust wait ran use the real clock (`it`, not `itFrozen`).

**Proving the Rust path ran.** A passing test alone could be the JS fallback (`rust-strict` still routes a carve-out wait to JS). A Rust wait is exactly one `wait` call, so routing tests load `counting-addon.js` and assert one recorded call, or zero for a whole-wait carve-out; subprocess tests that need the count run an API script under the spy (`extra-ca-api.js`). Under the real addon, `--verbose` output carries Rust's text (e.g. `(os error` where Node prints `ECONNREFUSED`). The #82 `TCPSocketWrap` leak test passes trivially under Rust (no Node handle is opened); the #82 close tests, where the server sees the close, are the real guard.

## JS vs Rust inventory

Rust is the primary implementation (L13): engine behaviour is specified by cargo tests in `crates/wait-on-core`, and the JS suites prove only what users see at `waitOn` and the CLI under both engines.

**Rule for new work.** Engine behaviour (a check, the loop, a line's text, a timing) gets a Rust test first: in `src/` for a unit, or in `tests/<kind>.rs` through `waiter::wait`, named `<kind>_<forward|reverse>_<behaviour>`. A JS test is added only at the API or CLI front door, or for something JS still owns (table B).

### A. Rust-specified

Files are under `crates/wait-on-core/`.

| Behaviour | File | Tests |
|---|---|---|
| `file:` through `wait` | `tests/file.rs` | `file_forward_ready_when_present`, `file_forward_window_waits_for_stable_size`, `file_forward_times_out_when_missing`, `file_reverse_ready_once_removed`, `file_reverse_times_out_while_present` |
| file size | `src/lib.rs` | `file_size_is_the_byte_length_of_an_existing_file`, `file_size_is_minus_one_for_a_missing_path`, `file_size_is_minus_one_for_a_path_under_a_regular_file`, `file_size_reports_a_directory`, `file_size_is_minus_one_for_a_dangling_symlink`, `file_size_is_minus_one_when_stat_is_denied` |
| `tcp:` through `wait` | `tests/tcp.rs` | `tcp_forward_ready_when_listening`, `tcp_forward_ipv6_literal_ready`, `tcp_forward_times_out_while_connect_pending`, `tcp_reverse_ready_when_nothing_listens`, `tcp_reverse_times_out_while_listening` |
| tcp connect | `src/tcp.rs` | `ready_when_listening`, `times_out_within_bound`, `timeout_zero_means_no_timeout`, `refused_when_nothing_listens`, `localhost_finds_ipv4_only_listener`, `ipv6_literal`, `resolve_error_is_not_ready`, `closes_after_success` |
| `socket:` through `wait` | `tests/socket.rs` | `socket_forward_ready_when_listening`, `socket_forward_times_out_when_missing`, `socket_reverse_ready_when_missing`, `socket_reverse_times_out_while_listening` |
| socket / pipe connect | `src/socket.rs` | `ready_when_listening`, `not_ready_when_missing` (unix); `ready_when_pipe_exists`, `not_ready_when_pipe_missing` (windows) |
| `command:` through `wait` | `tests/command.rs` | `command_forward_ready_on_exit_zero`, `command_forward_times_out_on_nonzero_exit`, `command_reverse_ready_when_command_fails`, `command_reverse_times_out_while_command_succeeds` |
| command runner | `src/lib.rs` | `run_command_is_ready_with_stdout_when_the_command_exits_0`, `run_command_is_not_ready_naming_the_command_on_a_non_zero_exit`, `run_command_puts_stderr_in_the_error_on_failure`, `run_command_runs_quoted_and_chained_commands_through_the_shell`, `run_command_inherits_the_parent_environment`, `run_command_is_not_ready_when_the_shell_cannot_find_the_command`, `run_is_not_ready_when_the_shell_itself_cannot_spawn`, `run_command_is_not_ready_on_signal_death`, `run_command_kills_an_attempt_at_the_timeout`, `run_command_waits_for_a_slow_command_without_a_timeout`, `run_command_returns_at_the_timeout_when_a_grandchild_holds_stdout`, `run_command_caps_captured_stdout_at_1_mib` |
| http through `wait` | `tests/http.rs` | `http_forward_head_ready_on_200`, `http_forward_get_ready_on_204`, `http_forward_get_times_out_on_404`, `http_forward_headers_reach_server_verbatim`, `http_forward_follows_redirect_when_enabled`, `http_forward_redirect_not_followed_times_out`, `http_reverse_ready_on_500`, `http_reverse_times_out_on_200` |
| https through `wait` | `tests/https.rs` | `https_forward_verified_roots_ready`, `https_forward_wrong_roots_times_out`, `https_forward_no_roots_accepts_any_cert`, `https_forward_mtls_identity_ready`, `https_forward_missing_identity_times_out`, `https_forward_garbage_roots_fail_tls_hop_only`, `https_forward_http_redirect_to_https_follows`, `https_reverse_ready_when_roots_wrong` |
| explicit proxy through `wait` | `tests/proxy.rs` | `proxy_forward_https_target_tunnels_connect`, `proxy_forward_userinfo_becomes_basic_auth`, `proxy_reverse_ready_when_proxy_down` |
| http over a unix socket / named pipe through `wait` | `tests/unix_http.rs` | `unix_http_forward_ready_over_socket` (unix), `unix_http_forward_ready_over_named_pipe` (windows), `unix_http_forward_times_out_when_socket_missing`, `unix_http_reverse_ready_when_socket_missing` |
| http checker | `src/http.rs` | `head_200_with_content_length_and_no_body_is_ready`, `get_204_is_ready_and_404_is_not`, `follow_redirect_true_follows_302_to_200`, `follow_redirect_false_reports_the_302`, `redirect_chain_over_20_hops_is_not_ready`, `validate_callback_decides_readiness_from_the_exact_status`, `validate_fn_pointer_overrides_the_2xx_rule`, `timeout_when_server_never_writes`, `get_timeout_covers_a_stalled_body`, `connection_refused_is_not_ready`, `headers_reach_the_server_verbatim`, `invalid_method_fails_construction`, `proxy_receives_absolute_form_requests_for_the_target`, `proxy_userinfo_is_percent_decoded_into_basic_auth`, `without_proxy_the_target_sees_origin_form`, `unparsable_proxy_uri_fails_construction`, `unparsable_roots_fail_only_tls_hops`, `garbage_identity_fails_only_tls_hops`, `non_pem_roots_are_an_empty_trust_set`, `valid_client_identity_builds_and_plain_http_is_ready`, `socket_path_carries_the_request_and_ignores_the_proxy`, `socket_path_carries_the_request_over_a_named_pipe`, `missing_socket_path_is_not_ready`, `cancel_settles_in_flight_and_later_checks`, `cancel_closes_the_pooled_keep_alive_connection`, `http_reuses_one_connection_across_checks` |
| loop end to end | `tests/loop.rs` | `http_head_and_get_print_result_lines_and_a_refused_url_times_out`, `validate_false_keeps_waiting_until_it_answers_true`, `validate_error_is_not_ready`, `settle_aborts_in_flight_checks_and_stops_ticking`, `command_runs_once_across_five_ticks_and_is_killed_at_command_timeout`, `mixed_wait_logs_one_waiting_line_per_flip_except_the_last` |
| loop schedule, lines, settle | `src/waiter/tests.rs` | `first_tick_is_at_delay_then_every_interval`, `interval_zero_ticks_every_millisecond`, `timeout_at_or_before_the_first_tick_times_out_before_any_check`, `no_timeout_waits_until_ready`, `missing_file_times_out_naming_it_with_the_stat_lines`, `missing_file_without_verbose_logs_only_the_waiting_line`, `two_ready_resources_log_one_line_per_flip_except_the_last`, `without_a_log_function_the_outcome_is_unchanged`, `growing_file_stabilizes_one_window_after_its_last_change`, `reverse_file_is_ready_once_removed_with_no_window`, `reverse_tcp_times_out_while_listening_and_is_ready_when_closed`, `tcp_verbose_lines_for_success_and_connect_timeout`, `socket_lines_for_connected_and_missing`, `command_lines_for_success_and_failure`, `command_drops_ticks_while_an_attempt_runs`, `simultaneous_one_queues_ticks_and_starts_one_per_completion`, `unlimited_simultaneous_starts_a_check_on_every_tick`, `first_ready_check_latches_cancels_the_checker_and_stops_ticking`, `reverse_http_is_ready_when_refused_with_the_error_line`, `checker_construction_error_returns_before_any_line`, `http_over_a_unix_socket_names_the_socket_path` |
| resource kind mapping (the napi `kind` string) | `src/waiter/tests.rs` | `from_parts_maps_file_to_its_path`, `from_parts_maps_http_to_its_options_or_the_default`, `from_parts_maps_tcp_with_host_and_port_or_their_defaults`, `from_parts_maps_socket_and_command`, `from_parts_rejects_an_unknown_kind` |
| parsers (JS is the oracle) | `src/parse.rs` | `prefix_matches_js_golden_vectors`, `host_port_matches_js_golden_vectors`, `interval_matches_js_numbers`, `http_unix_matches_js_golden_vectors`, and the reject, edge and no-panic cases beside them |

### B. JS parity

| File | Why it stays JS |
|---|---|
| `test/api.mocha.js`, `test/cli.mocha.js`, `test/coverage.mocha.js` | front doors: what `waitOn` resolves or rejects with, and the CLI's exit code and output, under both engines |
| `test/cli-conformance.mocha.js`, `test/cli-conformance-http.mocha.js`, `test/cli-conformance-helper.mocha.js` | black-box CLI vectors both engines must match, and their harness |
| `test/validation.mocha.js` | `WAIT_ON_SCHEMA` and `validateResource` run in JS before either engine |
| `test/https-proxy.mocha.js` | options JS resolves before the spec crosses: `passphrase` (`pkcs8Pem`), `strictSSL` into `roots`, `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` (`envProxyFor`), the `routesHttpToRust` carve-outs; routing proof through the counting addon and stub proxy |
| `test/engine.mocha.js` | engine selection (`WAIT_ON_ENGINE` values, missing and `wait`-less addon), JS spec-building and timer clamps (`buildSpec`), routing proof (counting addon), module graph, process lifetime, `NODE_EXTRA_CA_CERTS`, prebuild resolution |
| `test/parser-properties.mocha.js` | parser property tests and the Rust-vs-JS differential: JS is the oracle |
| `test/native-helpers.mocha.js`, `test/frozen-clock.mocha.js` | JS-only helpers (`_internal`, proxy decision helpers) and the JS fake clock |
| `test/prebuild-probe.mocha.js`, `test/rust-scaffold.mocha.js`, `test/benchmarks.mocha.js` | packaging probe, toolchain and `ci:rs` pins, benchmark helpers |

Spike-created JS tests are deleted only when every assertion is already covered by a Rust test. The L14 audit (#91, [plan](../plans/2026-10-01-spike-rs-l14-js-test-cleanup-plan.md#audit-base-832c588)) found none that are: each file above, and every spike fixture and helper, still asserts something no Rust test does.

## Rust coverage

`npm run ci:rs` ends with the coverage gate:

```bash
cargo xtask cov --exclude xtask --fail-under-lines 100 --fail-under-regions 100
```

`cargo xtask cov` is `cargo llvm-cov --workspace` with the arguments appended directly (no `--`, which would reach the test binaries). `--exclude xtask` drops the tooling crate from test and report; `build.rs` is not instrumented. The napi cdylib builds no test binary (`test = false`), so its files are absent from the report: every mocha suite under `rust-strict` in `ci:rs` exercises it, but the gate does not measure it. Prerequisites: [development.md](development.md#prerequisites). An uncovered region is a missing test or a branch to delete, never a follow-up.

- **Per host.** Each `rust` CI row gates what that OS compiles; `cfg`-gated code counts only on its own OS, so platform code keeps one `cfg`-gated test per side. PRs run ubuntu and windows; macOS runs only on push, so a local `npm run ci:rs` on a darwin host is the pre-merge macOS proof.
- **What counts.** Inline `#[cfg(test)]` modules in `src/*.rs` are measured. `tests/` directories and files named `tests.rs` (`crates/wait-on-core/tests/**`, `src/waiter/tests.rs`) are left out of the report by cargo-llvm-cov's built-in default, but the `src/` code they run still counts as covered. So deleting one unit test turns the gate red only when no other test (inline or integration) reaches its arm: the red-proof used `invalid_method_fails_construction` (`src/http.rs`), whose removal drops regions to 99.88%. There is no `--ignore-filename-regex` and no coverage attribute: unreachable code is restructured (`expect` carrying the invariant) or removed.
- **Branch coverage** is unavailable: `--branch` is nightly-only on the pinned stable toolchain. Lines and regions are the gate.
- **Assertions in inline tests.** A passing `assert!` or `assert_eq!` costs no region, but an expression in its message, or a `matches!` in an async test, leaves a region at count 0. Bind first (`let e = start.elapsed(); assert!(e < X, "{e:?}")`, not `assert!(.., "{:?}", start.elapsed())`); inline captures such as `"{out:?}"` cost nothing. In async tests use `is_err()`, a `let Err(..) = .. else` with no dead arm, or `assert_eq!` on a value taken from the result instead of `assert!(matches!(..))`. Host-dependent outcomes get one `cfg`-gated test per host, never a `match` with arms this host cannot reach.
- **Generics score per instantiation.** The per-file region summary scores a generic function by its best single instantiation, not the union, so `--show-missing-lines` can show no `^0` while the summary still reports misses. Find them in `cargo llvm-cov report --json`; fix by making one instantiation (e.g. `NoValidate` in `src/http.rs`) reach every arm.
- **Reading a report.** The instrumented build lands in `target/xtask-inner`; after a gate run, `CARGO_TARGET_DIR=target/xtask-inner cargo llvm-cov report --show-missing-lines` lists the misses. `cargo xtask cov --exclude xtask --summary-only` prints the table without failing.

**PEM fixtures.** `crates/wait-on-core/tests/fixtures/` holds `ca.pem`, `server.pem` + `server-key.pem` (SAN `DNS:localhost, IP:127.0.0.1, IP:::1`, `CA:FALSE`), `client.pem` + `client-key.pem` (signed by `ca.pem`) and the unrelated `other-ca.pem`; keys are PKCS#8, validity ~100 years. Tests parse them and never byte-compare, so a CRLF checkout is harmless. The CA key was discarded, so regenerating replaces all six files together. The suite never runs this; it needs OpenSSL 3 and a POSIX shell:

```sh
cd crates/wait-on-core/tests/fixtures
T=$(mktemp -d)
ca() { openssl req -x509 -newkey rsa:2048 -nodes -keyout "$2" -out "$1" -days 36500 -subj "/CN=$3" \
  -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign; }
leaf() { # out key cn extfile
  openssl req -newkey rsa:2048 -nodes -keyout "$2" -out "$T/req.csr" -subj "/CN=$3"
  openssl x509 -req -in "$T/req.csr" -CA ca.pem -CAkey "$T/ca-key.pem" -CAcreateserial \
    -CAserial "$T/ca.srl" -out "$1" -days 36500 -extfile "$4"
}
ca ca.pem "$T/ca-key.pem" "wait-on test CA"
ca other-ca.pem "$T/other-ca-key.pem" "wait-on other CA"
printf 'basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1\nextendedKeyUsage=serverAuth\n' > "$T/server.ext"
printf 'basicConstraints=critical,CA:FALSE\nextendedKeyUsage=clientAuth\n' > "$T/client.ext"
leaf server.pem server-key.pem localhost "$T/server.ext"
leaf client.pem client-key.pem "wait-on test client" "$T/client.ext"
rm -rf "$T"
```
