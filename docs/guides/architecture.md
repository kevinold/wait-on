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

Cargo workspace at repo root (`Cargo.toml`), `crates/wait-on-core` (pure Rust engine, no napi), `crates/wait-on-napi` (napi-rs `cdylib` binding). The npm package stays in `lib/` and `bin/`.

Status: planned (lane L1)

## Resource checks in Rust

Checks move to Rust one at a time behind the existing rxjs pipeline: `tcp:`/`socket:` (L2), `file:` (L3), `http(s)` (L4), TLS/proxy/unix (L5), `command:` (L6); then the polling loop itself (L7).

Status: planned (lane L7)

## Prebuilds and loader

Prebuilt addons live in `prebuilds/<platform>-<arch>[-musl]/wait-on.node`, loaded by a small hand-written loader in `lib/` (no new runtime dependency).

Status: planned (lane L9)

## Engine selection and fallback

`WAIT_ON_ENGINE=rust` loads the addon and falls back to JS if it fails to load; `WAIT_ON_ENGINE=rust-strict` makes a load failure an error (for CI). Unset means JS. Public API, CLI, schema, and `index.d.ts` do not change.

Status: planned (lane L1)
