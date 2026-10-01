# Architecture

## Front door and modules

[`lib/wait-on.js`](../../lib/wait-on.js) is the front door for both engines. Runtime deps: `joi`, `rxjs`, `undici`. The CLI ([`bin/wait-on`](../../bin/wait-on)) parses args with `util.parseArgs` and calls the same `waitOn`.

- **Entry:** `waitOn(opts[, cb])` → `waitOnImpl`. Promise form without `cb`, callback form with it.
- **Validation:** options are validated against the joi `WAIT_ON_SCHEMA`; `validateResources` fails fast on malformed http/tcp resources. Then `resolveEngine`, the bound `log`/`output`, the reverse banner and `cleanup` (the `complete` / `exiting with error` line, then the callback) run here for both engines.
- **Dispatch:** `waitOnImpl` lazily requires one engine and calls its `run` ([Polling loop](#polling-loop-l7)).

| Module | Holds | Loads |
|---|---|---|
| `lib/wait-on.js` | schema, `waitOn`/`waitOnImpl`, `validateResource`, `_internal` | `joi`, `engine.js`, `resources.js` |
| `lib/resources.js` | pure helpers: `PREFIX_RE` and friends, `extractPrefix`/`extractPath`, `tcpHostPort`, `httpRequest` (method, url, unix split, headers with auth), `proxyObjectUri`, `envProxyFor`, `routesHttpToRust`, `determineRemainingResources` | nothing |
| `lib/engine-js.js` | the rxjs/undici engine below | `rxjs`, `undici` |
| `lib/engine-rust.js` | the Rust shim: `routable`, `buildSpec`, TLS preparation, `run` | `tls`, `crypto` |
| `lib/engine.js` | `WAIT_ON_ENGINE` selection and the addon loader | `path` |

## JS engine

[`lib/engine-js.js`](../../lib/engine-js.js), addon-free: it never calls the addon, even when one is loaded.

- **Pipeline (rxjs):** one observable per resource from `createResource$`, joined with `combineLatest`, merged with a `timer(timeout)` error. Runs `takeWhile` until every resource is ready.
- **Per resource:** `timer(delay, interval)` → `mergeMap` (concurrency `simultaneous`) → check → `startWith(false)` → `distinctUntilChanged()` → `take(2)`.
- **Dispatch:** `PREFIX_RE` (`https?-get`, `https?`, `tcp`, `socket`, `file`, `command`) selects the `createResource$` case:

| Prefix | Factory | Ready when |
|---|---|---|
| `file:` (default, no prefix) | `createFileResource$` | file exists and size is stable over `window` |
| `http:` / `https:` | `createHTTP$` | HEAD returns 2xx (or `validateStatus`) |
| `http-get:` / `https-get:` | `createHTTP$` | GET returns 2xx (or `validateStatus`) |
| `tcp:` | `createTCP$` | connect succeeds within `tcpTimeout` |
| `socket:` | `createSocket$` | unix socket / Windows named pipe connects |
| `command:` | `createCommand$` | command exits 0 (per-attempt `commandTimeout`) |

- **HTTP:** undici `fetch` with a dispatcher from `buildDispatcher` (TLS `ca`/`cert`/`key`/`passphrase`/`strictSSL`, `proxy`, env proxies via `EnvHttpProxyAgent`, `http://unix:<sock>:<path>` via `socketPath`).
- **Reverse:** `reverse: true` inverts each check (`negateAsync`; `file:` waits for size -1).

## Rust engine layout

Cargo workspace at repo root (`Cargo.toml`, committed `Cargo.lock`), `crates/wait-on-core` (pure Rust engine, no napi), `crates/wait-on-napi` (napi-rs `cdylib` binding). The npm package stays in `lib/` and `bin/`. One version for both crates lives in `[workspace.package]`; `rust-toolchain.toml` pins the toolchain and the workspace `rust-version` (MSRV) equals it. `deny.toml` is the `cargo deny` policy and `supply-chain/` the `cargo vet` store ([Supply chain](#supply-chain)).

## Supply chain

Two gates run on every `npm run ci:rs` (R23). `cargo deny check` (`deny.toml`) gates advisories, licenses, bans and sources. `cargo vet --locked` (`supply-chain/`) runs first and fails on any third-party crate version in `Cargo.lock` that no imported audit, local audit (`audits.toml`) or exemption (`config.toml`) covers, so a new or bumped crate cannot reach the addon unreviewed. `imports.lock` pins the imported audits, and `--locked` never fetches new ones. Workspace members (`wait-on-core`, `wait-on-napi`) are not vetted.

Trust sources (`[imports]` in `supply-chain/config.toml`): Mozilla, Google, Bytecode Alliance and ZcashFoundation are the required baseline. ZcashFoundation is a URL import of zebra's `audits.toml` because it is not in the cargo-vet registry, and it covers none of today's crates (its section in `imports.lock` is empty). ISRG, Embark Studios and Zcash (ECC) are added on top, each kept because it removed at least one exemption.

Baseline (2026-09-30, L11): 148 third-party crates, 18 fully audited by imports, 130 exempted. Of the exemptions, 63 are small bumps past an audited version (`notes` names the audited base), 66 have no audited base, and one (`quote`) is exempt because Google's `quote` audits are excluded from the import (`exclude = ["quote"]`): their notes contain double quotes that cargo-vet 0.10.0 (what CI installs) and 0.10.2 format differently, and either format fails `cargo vet --locked` on the other version. Every exemption carries a `notes` reason. A change that raises the exemption count should say why in its PR; the preferred path is certifying (see [development.md](development.md#vetting-a-new-or-bumped-crate)).

The addon exports `wait()` (the whole loop, [below](#polling-loop-l7)), `version()` (the crate version), `noop()`, and the per-check exports of L2–L8 ([Resource checks in Rust](#resource-checks-in-rust)).

## Polling loop (L7)

Under a loaded addon one `waitOn` is one napi call: `addon.wait(spec, log?, validateStatus?)` runs every resource, the schedule, `window` stabilization, `simultaneous`, `timeout`, `reverse` and the `log`/`verbose` lines in `wait_on_core::waiter` ([`crates/wait-on-core/src/waiter.rs`](../../crates/wait-on-core/src/waiter.rs)). rxjs does not drive the Rust engine.

**Dispatch.** `resolveEngine` treats an addon without a `wait` function (a stale prebuild) as a load failure: `rust` falls back to JS with `loadError` set, `rust-strict` fails with `WAIT_ON_ENGINE=rust-strict: the native addon at <path> has no wait export`. With an addon loaded, `waitOnImpl` runs `lib/engine-rust.js` when `routable` holds (every http resource passes `routesHttpToRust`), else `lib/engine-js.js`. The fallback is whole-wait:

| Condition on validated options, url and env | Wait runs on |
|---|---|
| Addon not loaded (`js`, or `rust` with load failure or no `wait` export) | JS (rxjs, undici) |
| Any http resource with userinfo (`user:pass@`) | JS (undici rejects it; kept for parity) |
| Any https resource whose env-proxy decision selects a proxy | JS: on this branch the JS path's `EnvHttpProxyAgent({ connect })` drops `strictSSL`/`ca`/`cert`/`key` for https targets (undici reads target TLS only from `requestTls`), so Rust would diverge. Moves to Rust once the `requestTls`/`proxyTls` fix from `refactor/axios-to-fetch` reaches this branch |
| Any http resource with a proxy URI in play (the `proxy` object's URI, or a set `http_proxy ?? HTTP_PROXY` / `https_proxy ?? HTTPS_PROXY`) that is not a valid `http:` URL | JS: undici verifies an `https:` proxy hop with Node defaults while reqwest would apply the target's TLS settings to it; `socks*` needs reqwest's `socks` feature (not built); a malformed env value makes undici's constructor throw at once |
| Otherwise (no http resource, or every one is plain http/https, any TLS option, `strictSSL: true`, an `http:` proxy object, `proxy: false`, an env proxy for an http target or a `NO_PROXY`-exempted https target, `http://unix:`) | Rust (one `wait` call) |

One carve-out http resource sends every co-listed `file:`, `tcp:`, `socket:` and `command:` resource of that wait to the JS engine too, with zero addon calls.

**Spec.** `lib/engine-rust.js` `buildSpec` turns validated options into `{ delayMs, intervalMs, windowMs, tcpTimeoutMs, commandTimeoutMs, simultaneous?, timeoutMs?, reverse, verbose, resources }` (napi `u32` fields; `simultaneous` and `timeoutMs` absent for `Infinity`). Each resource is `{ name, kind, path?, host?, port?, command?, http? }`: `kind` is `file`, `tcp` (`host`, `port` from `tcpHostPort`), `socket`, `command`, or `http` with the L5 `HttpCheckerOptions` ([table below](#https-l4-l5)). Resource parsing, http option and TLS preparation, and `validateStatus` stay in JS. Timer-backed fields (`delay`, `interval`, `timeout`, `tcpTimeout`, `commandTimeout`) above 2^31-1 ms become 1 ms, as Node's timers do; `window` and `simultaneous` clamp at 2^32-1. A spec napi cannot convert (a tcp port above 65535) throws synchronously and reaches the callback.

**Result.** `wait` resolves `{ ok, error }` and never rejects for a timeout or a checker construction error: `ok` maps to `cleanup()` (callback `err === undefined`), `{ ok: false, error }` to `cleanup(new Error(error))`. Http checkers are built before the first line, so a bad proxy URI returns at once with no waiting line. `timeout <= max(delay, 1 ms)` returns timed out after the initial waiting line and before any check (on JS the timeout timer is subscribed first and wins the tie).

**Log ordering.** `log` is passed only when `log` or `verbose` is on; it crosses as a weak `ThreadsafeFunction<String>` that Rust awaits with `call_async`, so each line has run in JS before the next step. The `waiting for N resources: …` lines (once at start, then after each flip that leaves a resource unready) all run before the result settles; `cleanup` then prints `complete` or `exiting with error`, so `bin/wait-on`'s `process.exit` inside the callback loses nothing. `spec.verbose` gates the per-resource lines inside Rust; their text is the JS engine's. Rust never writes to fd 1.

**Schedule.** One tokio task per resource: first tick at `max(delay, 1 ms)`, then every `max(interval, 1 ms)` without bursting (rxjs `timer(delay, interval)`). Ticks beyond `simultaneous` queue and start one per completed check (`mergeMap`); `command:` keeps one attempt in flight and drops ticks while it runs (`exhaustMap`). The non-reverse `file:` result runs the JS `scan` state machine on `Instant`; reverse `file:` is ready at size -1; other kinds negate the verdict under `reverse`. `file_size` and `run_command` run under `spawn_blocking`, never on the libuv threadpool.

**Cancellation on settle.** A resource that reads ready aborts its in-flight checks and cancels its http checker. On settle by any path the aggregator drops every resource task (aborting in-flight connects and ticks) and cancels every http checker, so pooled sockets close and a hung server cannot keep an API caller's process alive. Only a `spawn_blocking` stat or command attempt outlives the settle; it holds no napi reference.

**Module graph.** A bare `require('wait-on')` loads neither `rxjs` nor `undici`. A Rust wait loads `wait-on.js`, `engine.js`, `resources.js`, `engine-rust.js`, `joi`, `tls`, `crypto` and the addon; a JS wait (including every whole-wait fallback) loads `engine-js.js` and with it `rxjs` and `undici` (`test/engine.mocha.js`, module graph).

**Thin napi.** All loop logic, line text, spec types and the sink contract live in `wait-on-core` as plain Rust, where cargo tests cover them. [`crates/wait-on-napi/src/wait.rs`](../../crates/wait-on-napi/src/wait.rs) holds only the `#[napi(object)]` structs, a field-by-field conversion into `waiter::WaitSpec`, the closure forwarding one line to the `log` threadsafe function, the `validateStatus` adapter and the `AsyncBlockBuilder` call; the `cdylib` (`test = false`) is checked only by the real-addon mocha tests under `ci:rs`.

## Resource checks in Rust

The checks were ported one lane at a time behind the rxjs pipeline: `tcp:`/`socket:` (L2), `file:` (L3), `http(s)` (L4), TLS/proxy/unix (L5), `command:` (L6). Since L7 the loop calls the core functions (`file_size`, `run_command`, `tcp::ready`, `socket::ready`, `HttpChecker`) directly. The per-check napi exports stay for the real-addon tests in `test/engine.mocha.js` (`fileSize`, `runCommand`, `HttpChecker`) and the parser differential (`parse*`); `tcpCheck`/`socketCheck` have no JS caller left. Removing them is a later cleanup. The known differences listed with each check apply on the Rust engine.

- **`file:` (L3):** `wait_on_core::file_size` stats the path (following symlinks) and returns its size, or `-1` on any error, the same contract as JS `getFileSize` (a missing file, `EPERM`, and Windows delete-pending errors all read as `-1`). The export `fileSize(path): Promise<number>` is an napi `AsyncTask`. Known difference: paths cross into Rust as UTF-8, so a Windows path containing an unpaired UTF-16 surrogate (which `fs.stat` accepts) reads as `-1` under Rust.
- **`command:` (L6):** `wait_on_core::run_command` runs the command through the same shell as Node's `child_process.exec`: `/bin/sh -c <command>` on POSIX, and on Windows `ComSpec` (default `cmd.exe`) with `/d /s /c "<command>"` passed verbatim. The child inherits the environment and working directory. Exit 0 is ready; a non-zero exit, a signal, a spawn failure or a `commandTimeout` kill is not ready. With `commandTimeout > 0` the shell is killed at the bound, and the result is also returned at the bound when the shell exited but a backgrounded grandchild still holds stdout (as Node does). The export `runCommand(command, timeoutMs): Promise<{ ok, stdout, error }>` never rejects and runs each attempt on its own thread. Known differences from JS: stdin is empty (Node gives an open pipe), output beyond 1 MiB per stream is dropped instead of failing the attempt (Node's `maxBuffer`), the timeout error reads `Command failed: <cmd>\nkilled after <ms>ms`, and a `ComSpec` that is not `cmd.exe` still gets the `cmd.exe` arguments.

- **`tcp:` / `socket:` (L2):** `crates/wait-on-core/src/{tcp,socket}.rs`, exported as async `tcpCheck(host, port, timeoutMs)` and `socketCheck(path)`, each resolving to `{ ready, timedOut, reason }` (`reason` is `null` when ready). They run on napi's tokio runtime (napi feature `async`), not the libuv threadpool.
- **Parsers (L8):** `crates/wait-on-core/src/parse.rs` ports the four pure parsers (resource prefix, `host:port`, the `ms/s/m/h` interval, the `http://unix:` split) by hand, replicating the JS regex semantics (`.` excludes line terminators, `\d` is ASCII, `$` is strict end, `/i` folds ASCII only, `parseFloat` prefix rules). The addon exports them as sync `parsePrefix`, `parseHostPort`, `parseInterval`, `parseHttpUnix` for the differential test only; runtime parsing stays in JS (the `wait` spec carries parsed fields).
- **tcp:** resolves the host and races one connect per address (Windows reports a refused loopback connect only after ~2 s, so sequential tries would starve the IPv4 fallback); `tcpTimeout` bounds the whole attempt, `0` means no timeout.
- **socket:** unix domain socket connect; on Windows the path is a named pipe opened as a client.
- **Deliberate difference:** under `--verbose` the not-ready reason is Rust's error text (e.g. `Connection refused (os error 61)`), not Node's `ECONNREFUSED`.

### http(s) (L4, L5)

`http:`/`https:` (HEAD) and `http-get:`/`https-get:` (GET) run in Rust, including TLS options, proxies and `http://unix:` (L5), whenever the wait runs on Rust ([routing](#polling-loop-l7)).

- **Checker.** `crates/wait-on-core/src/http.rs` holds `HttpChecker`: one reqwest `Client` per resource (rustls with the `ring` provider, no OpenSSL, `no_proxy()` so the environment never leaks in), mirroring the one undici dispatcher per resource on the JS path. `followRedirect: true` maps to `redirect::Policy::limited(20)`, `false` to `Policy::none()` so the 3xx is the status checked. `httpTimeout` becomes a whole-request timeout covering the GET body; unset means none. GET reads the body only when the status passed.
- **Binding.** The loop builds `HttpChecker` in core from the spec's `http` options (`HttpCheckerOptions`, mapped by the one function the export shares). The export `crates/wait-on-napi/src/http.rs` exposes `new HttpChecker({ url, method, headers, followRedirect, timeoutMs, roots?, cert?, key?, proxy?, socketPath? })`, `check(validateStatus?)` (a Promise of `{ ok, status?, statusText?, error? }` that never rejects for not-ready) and `cancel()`.
- **`validateStatus` across the boundary.** JS wraps the user function as `s => Boolean(fn(s))` with a throw mapped to `false`, then passes it to `wait` as one napi `ThreadsafeFunction` (`CalleeHandled = false`, weak) that every http resource of the wait shares through an `Arc`. Rust calls it with `call_async_catch`, so a JS throw or non-boolean is an `Err` (not ready), never a fatal exception. The JS thread is free while it awaits the check Promise, so callbacks from concurrent checks under `simultaneous` do not deadlock.
- **Headers and auth.** JS builds the final header map (the `auth` Basic header and its case-insensitive override) for both engines and passes string values (`String(v)`); Rust sets them verbatim.
- **Teardown.** The loop calls `checker.cancel()` when the resource reads ready and for every checker at settle. `cancel()` is a tokio `watch` flag: every in-flight check and any later one resolves `{ ok: false, error: 'cancelled' }`. It also drops the checker's `Client`, so pooled keep-alive sockets close as the JS path's `dispatcher.close()` does; otherwise each finished resource held an idle connection until reqwest's pool timeout, which exhausted sockets (`ENOBUFS`) across a full test run on Windows. Without it a pending Promise to a hung server would keep an API caller's process alive after `waitOn` settles (`test/engine.mocha.js`, process lifetime).
- `reverse` negates the un-negated `ok` inside the loop; verbose lines show the un-negated result, as on JS.

TLS, proxy and unix options cross the boundary as plain strings JS prepares once per resource; Rust only configures reqwest from them:

| Input | JS preparation | Addon field | reqwest |
|---|---|---|---|
| `strictSSL: true` | `ca` as UTF-8 PEM, else `tls.getCACertificates('default')` (Node's roots, including `NODE_EXTRA_CA_CERTS`) | `roots` | `tls_certs_only` over exactly those certificates; never the OS store |
| `strictSSL: false` (default) | nothing | absent | `tls_danger_accept_invalid_certs(true)` |
| `cert` + `key` (+ `passphrase`) | key re-exported as plain PKCS#8 with `crypto.createPrivateKey`, passed as given when it does not parse | `cert`, `key` | `identity(Identity::from_pem(cert + key))` |
| TLS material that does not parse | passed as given | as given | client with an empty root set and no danger flag: every TLS hop fails, plain http still works (Node's per-connection failure) |
| `proxy` object | normalized URI (`proxyObjectUri`, shared with the JS path) | `proxy` | `Proxy::all` after `no_proxy()` |
| `proxy` unset | `envProxyFor`: undici's `EnvHttpProxyAgent` choice (`http_proxy ?? HTTP_PROXY`, `https_proxy ?? HTTPS_PROXY` falling back to the http proxy, undici's `NO_PROXY` grammar) | `proxy` or absent | as above |
| `proxy: false` or `http://unix:` | none | absent | direct |
| `http://unix:<sock>:<path>` | `socketPath`, url `http://localhost/<path>` | `socketPath` | `unix_socket` (POSIX) / `windows_named_pipe` (Windows) |

TLS material is passed for every target scheme because one client follows the whole redirect chain, and an http resource may redirect to https.

**Per-check overhead** (`node benchmarks/http-ffi.js --iterations 200`; darwin arm64, Node v26.3.1). Per-call rows time a full `waitOn` against a ready local server (one new checker and connection each); `steady` rows time the gap between polls on one resource (connection reuse); `ffi noop` is the bare boundary (`addon.noop()`, mean of 1000 calls).

| configuration | median ms | p95 ms |
|---|---|---|
| js | 1.959 | 3.181 |
| js steady | 1.497 | 1.779 |
| rust-strict | 1.494 | 1.697 |
| rust-strict + validateStatus | 1.556 | 1.735 |
| rust-strict steady | 1.272 | 1.382 |
| rust-strict ffi noop | 0.000014 | 0.000020 |

The bare FFI call is about 14 ns, and the `validateStatus` threadsafe round trip adds about 0.06 ms per check. The Rust check is not slower than the JS check on this host. L8 owns the regression threshold. Measured before L7, when rxjs drove one napi call per check; not re-measured since the loop moved (L7).

## Deliberate JS vs Rust differences

On the Rust engine, beyond the per-check differences listed with each check above:

- Default request headers: undici sends `user-agent: undici`, `accept-language`, `sec-fetch-mode`; reqwest sends `accept: */*` only.
- Redirect hop limit: 20 on both (reqwest `Policy::limited(20)`, undici fetch's limit).
- TLS: rustls negotiates TLS 1.2/1.3 with ECDHE AEAD suites only. A legacy https target offering only RSA key exchange or DHE suites handshakes under Node/OpenSSL (with `strictSSL: false`) but not under Rust, and polls until `timeout` (under `reverse` it reads as gone on the first poll).
- `httpTimeout` above 2^31-1 ms: Node's `AbortSignal.timeout` overflows (every check fails); Rust clamps to 2^32-1 ms.
- Certificate shape under `strictSSL: true`: webpki requires the server name in `subjectAltName` and rejects an end-entity certificate with `basicConstraints CA:TRUE`. Node accepts a CN-only or CA-flagged self-signed certificate supplied as its own `ca`; Rust times out on it. The test fixture (`test/helpers/tls-fixture.js`) generates a SAN, `CA:FALSE` leaf so both engines verify it.
- Env proxy selection is computed once per resource from the resource URL: `NO_PROXY` changes during the wait are not re-read (undici re-reads per dispatch), and a redirect to another scheme or host keeps the first hop's proxy decision (undici decides per hop).
- Under `--verbose`, every http error line carries reqwest's error text, not Node's.
- Under `--verbose`, an http result with no canonical reason phrase prints `statusText: undefined` (JS prints `statusText: ''`).
- `interval: 0` is a 1 ms period on both engines (Node clamps `setInterval` to 1 ms; the loop uses `max(interval, 1 ms)`).
- Timer-backed durations (`delay`, `interval`, `timeout`, `tcpTimeout`, `commandTimeout`) above 2^31-1 ms become 1 ms on both engines: the shim mirrors Node's timer overflow, without Node's `TimeoutOverflowWarning`. `window` is compared against elapsed time, not a timer, and clamps at 2^32-1 ms under Rust.
- A `command:` attempt may outlive the process exit (since L6): the loop cannot abort a `spawn_blocking` attempt at settle.
- A verbose line from a check still in flight at settle is dropped under Rust (the check is aborted); JS may still print it after the completion line.
- An out-of-range tcp port (e.g. `tcp:localhost:99999`) errors on both engines with different text: JS `Port should be >= 0 and < 65536…`, Rust `Failed to convert u32 to u16 on ResourceSpec.port …` (napi's spec conversion).

## Prebuilds and loader

Prebuilt addons live in `prebuilds/<platform>-<arch>[-musl]/wait-on.node` (gitignored; shipped via `files` in `package.json`), loaded by the hand-written loader in [`lib/engine.js`](../../lib/engine.js) with a plain `require()` (no new runtime dependency). The directory is `process.platform`-`process.arch`, plus `-musl` on linux when the process report shows no glibc runtime. `npm run build:napi` builds the host addon (see [development.md](development.md#building-the-addon)).

All eight targets ship in one tarball; `ci:rs:package` proves install and load (see [ci.md](ci.md#cirspackage)). Linux glibc addons need glibc 2.39+ ([releasing.md](releasing.md#rust-test-prereleases-fork)).

## Engine selection and fallback

`lib/engine.js` reads `WAIT_ON_ENGINE` on every `waitOn` call, after option and resource validation:

| Value | Engine | Addon load failure |
|---|---|---|
| unset, empty, `js` | JS; the addon is never loaded | n/a |
| `rust` | Rust addon loaded | silent fallback to JS, no output change |
| `rust-strict` | Rust addon loaded | `waitOn` rejects / calls back with `WAIT_ON_ENGINE=rust-strict: failed to load the native addon at <path>: <cause>`; the CLI prints it and exits 1 |
| anything else | none | `WAIT_ON_ENGINE="<v>" is not one of js, rust, rust-strict` (same delivery) |

An addon that loads but has no `wait` export counts as a load failure ([Polling loop](#polling-loop-l7)).

`WAIT_ON_NATIVE_LIBRARY_PATH` (absolute path) overrides the addon location. It is a dev/test hook, not a public option; wait-on does not reuse napi-rs's global `NAPI_RS_NATIVE_LIBRARY_PATH` so it cannot pick up another package's addon. Public API, CLI, schema, and `index.d.ts` do not change.
