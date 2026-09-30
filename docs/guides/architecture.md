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

Status: planned (lane L7)

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
