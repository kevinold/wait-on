# Architecture

## JS engine (today)

All engine code is one file, [`lib/wait-on.js`](../../lib/wait-on.js). Runtime deps: `joi`, `rxjs`, `undici`. The CLI ([`bin/wait-on`](../../bin/wait-on)) parses args with `util.parseArgs` and calls the same `waitOn`.

- **Entry:** `waitOn(opts[, cb])` → `waitOnImpl`. Promise form without `cb`, callback form with it.
- **Validation:** options are validated against the joi `WAIT_ON_SCHEMA`; `validateResources` fails fast on malformed http/tcp resources.
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

Cargo workspace at repo root (`Cargo.toml`, committed `Cargo.lock`), `crates/wait-on-core` (pure Rust engine, no napi), `crates/wait-on-napi` (napi-rs `cdylib` binding). The npm package stays in `lib/` and `bin/`. One version for both crates lives in `[workspace.package]`; `rust-toolchain.toml` pins the toolchain and the workspace `rust-version` (MSRV) equals it. `deny.toml` is the `cargo deny` policy.

The addon exposes `version()` (the crate version), `noop()`, and the resource checks ported so far (below). Resources whose check is not ported yet keep using the JS checks under a loaded Rust engine.

## Resource checks in Rust

Checks move to Rust one at a time behind the existing rxjs pipeline: `tcp:`/`socket:` (L2), `file:` (L3), `http(s)` (L4), TLS/proxy/unix (L5), `command:` (L6); then the polling loop itself (L7). `waitOnImpl` passes the loaded addon (`null` under `js` or a `rust` fallback) into the resource factories, and each ported factory asks the addon instead of its JS check when one is present.

- **`file:` (L3):** `wait_on_core::file_size` stats the path (following symlinks) and returns its size, or `-1` on any error, the same contract as JS `getFileSize` (a missing file, `EPERM`, and Windows delete-pending errors all read as `-1`). The addon exports it as `fileSize(path): Promise<number>`, an napi `AsyncTask` on the libuv threadpool, so a slow stat never blocks the event loop. `createFileResource$` calls it in place of `getFileSize`; the `window` stabilization `scan`, the reverse check, and the verbose lines stay in JS and are unchanged. Known difference: paths cross into Rust as UTF-8, so a Windows path containing an unpaired UTF-16 surrogate (which `fs.stat` accepts) reads as `-1` under Rust.

- **`tcp:` / `socket:` (L2):** `crates/wait-on-core/src/{tcp,socket}.rs`, exported as async `tcpCheck(host, port, timeoutMs)` and `socketCheck(path)`, each resolving to `{ ready, timedOut, reason }` (`reason` is `null` when ready). They run on napi's tokio runtime (napi feature `async`), not the libuv threadpool, so a pending connect never blocks Node's fs/dns work. `createTCP$` / `createSocket$` call the export when the loaded addon has it, else the JS check (per export, under `rust` and `rust-strict`). JS still parses `host:port`, applies `negateAsync` for reverse and prints the verbose lines.
- **tcp:** resolves the host and races one connect per address (Windows reports a refused loopback connect only after ~2 s, so sequential tries would starve the IPv4 fallback); `tcpTimeout` bounds the whole attempt, `0` means no timeout.
- **socket:** unix domain socket connect; on Windows the path is a named pipe opened as a client.
- **Deliberate difference:** under `--verbose` the not-ready reason is Rust's error text (e.g. `Connection refused (os error 61)`), not Node's `ECONNREFUSED`.

Status: planned (lane L7)

### http(s) (L4)

`http:`/`https:` (HEAD) and `http-get:`/`https-get:` (GET) run in Rust when the Rust engine is loaded and no L5 condition holds. `createHTTP$` decides once per resource with `routesHttpToRust` ([`lib/wait-on.js`](../../lib/wait-on.js)):

| Condition on validated options and env | Check |
|---|---|
| Addon not loaded (`js`, or `rust` with load failure) | JS (undici) |
| `http://unix:` socket path | JS (L5) |
| Any of `ca`, `cert`, `key`, `passphrase` | JS (L5) |
| `strictSSL: true` | JS (L5) |
| `proxy` set (object or `false`) | JS (L5) |
| `HTTP_PROXY` / `http_proxy` / `HTTPS_PROXY` / `https_proxy` set | JS (L5) |
| URL with userinfo (`user:pass@`) | JS (undici rejects it; kept for parity) |
| Otherwise: plain http, and https with the default `strictSSL: false` | Rust |

- **Checker.** `crates/wait-on-core/src/http.rs` holds `HttpChecker`: one reqwest `Client` per resource (rustls with the `ring` provider, no OpenSSL, `no_proxy()`, invalid certs accepted), mirroring the one undici dispatcher per resource on the JS path. `followRedirect: true` maps to `redirect::Policy::limited(20)`, `false` to `Policy::none()` so the 3xx is the status checked. `httpTimeout` becomes a whole-request timeout covering the GET body; unset means none. GET reads the body only when the status passed.
- **Binding.** `crates/wait-on-napi/src/http.rs` exposes `new HttpChecker({ url, method, headers, followRedirect, timeoutMs })`, `check(validateStatus?)` (a Promise of `{ ok, status?, statusText?, error? }` that never rejects for not-ready) and `cancel()`.
- **`validateStatus` across the boundary.** JS wraps the user function as `s => Boolean(fn(s))` with a throw mapped to `false`, then passes it as a napi `ThreadsafeFunction` (`CalleeHandled = false`, weak). Rust calls it with `call_async_catch`, so a JS throw or non-boolean is an `Err` (not ready), never a fatal exception. The JS thread is free while it awaits the check Promise, so callbacks from concurrent checks under `simultaneous` do not deadlock.
- **Headers and auth.** JS builds the final header map (the `auth` Basic header and its case-insensitive override) for both engines and passes string values (`String(v)`); Rust sets them verbatim.
- **Teardown.** `finalize` calls `checker.cancel()`, a tokio `watch` flag: every in-flight check and any later one resolves `{ ok: false, error: 'cancelled' }`. Without it a pending Promise to a hung server would keep an API caller's process alive after `waitOn` settles (`test/engine.mocha.js`, process lifetime).
- `reverse` is still applied in JS (`negateAsync`) to the addon's un-negated `ok`.

Deliberate JS vs Rust differences on the Rust path:

- Default request headers: undici sends `user-agent: undici`, `accept-language`, `sec-fetch-mode`; reqwest sends `accept: */*` only.
- Redirect hop limit: 20 on both (reqwest `Policy::limited(20)`, undici fetch's limit).
- TLS: rustls negotiates TLS 1.2/1.3 with ECDHE AEAD suites only. A legacy https target offering only RSA key exchange or DHE suites handshakes under Node/OpenSSL (with `strictSSL: false`) but not under Rust, and polls until `timeout` (under `reverse` it reads as gone on the first poll).
- `httpTimeout` above 2^31-1 ms: Node's `AbortSignal.timeout` overflows (every check fails); Rust clamps to 2^32-1 ms.

**Per-check overhead** (`node benchmarks/http-ffi.js --iterations 200`; darwin arm64, Node v26.3.1). Per-call rows time a full `waitOn` against a ready local server (one new checker and connection each); `steady` rows time the gap between polls on one resource (connection reuse); `ffi noop` is the bare boundary (`addon.noop()`, mean of 1000 calls).

| configuration | median ms | p95 ms |
|---|---|---|
| js | 1.959 | 3.181 |
| js steady | 1.497 | 1.779 |
| rust-strict | 1.494 | 1.697 |
| rust-strict + validateStatus | 1.556 | 1.735 |
| rust-strict steady | 1.272 | 1.382 |
| rust-strict ffi noop | 0.000014 | 0.000020 |

The bare FFI call is about 14 ns, and the `validateStatus` threadsafe round trip adds about 0.06 ms per check. The Rust check is not slower than the JS check on this host. L8 owns the regression threshold.

## Prebuilds and loader

Prebuilt addons live in `prebuilds/<platform>-<arch>[-musl]/wait-on.node` (gitignored; shipped via `files` in `package.json`), loaded by the hand-written loader in [`lib/engine.js`](../../lib/engine.js) with a plain `require()` (no new runtime dependency). The directory is `process.platform`-`process.arch`, plus `-musl` on linux when the process report shows no glibc runtime. `npm run build:napi` builds the host addon (see [development.md](development.md#building-the-addon)).

Status: multi-target prebuilds and the install matrix planned (lane L9)

## Engine selection and fallback

`lib/engine.js` reads `WAIT_ON_ENGINE` on every `waitOn` call, after option and resource validation:

| Value | Engine | Addon load failure |
|---|---|---|
| unset, empty, `js` | JS; the addon is never loaded | n/a |
| `rust` | Rust addon loaded | silent fallback to JS, no output change |
| `rust-strict` | Rust addon loaded | `waitOn` rejects / calls back with `WAIT_ON_ENGINE=rust-strict: failed to load the native addon at <path>: <cause>`; the CLI prints it and exits 1 |
| anything else | none | `WAIT_ON_ENGINE="<v>" is not one of js, rust, rust-strict` (same delivery) |

`WAIT_ON_NATIVE_LIBRARY_PATH` (absolute path) overrides the addon location. It is a dev/test hook, not a public option; wait-on does not reuse napi-rs's global `NAPI_RS_NATIVE_LIBRARY_PATH` so it cannot pick up another package's addon. Public API, CLI, schema, and `index.d.ts` do not change.
