---
title: "L5: TLS options, proxies and http-over-unix in Rust - Plan"
type: feat
date: 2026-09-30
topic: rust-port
spine: kevinold/wait-on#35
sub_issue: kevinold/wait-on#57
lane: L5
branch: rs-57-tls-proxy-unix
base: spike-next-rs
parent_plans:
  - docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
  - docs/plans/2026-09-28-1239-feat-rust-port-plan.md
  - docs/plans/2026-09-30-spike-rs-l4-http-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-issue-57
execution: code
---

# L5: TLS options, proxies and http-over-unix in Rust - Plan

Lane plan for sub-issue #57. Requirements only; planning (technical decisions,
units) is added by `/ce-plan` inside `/lfg`. Spine rules (KD-S1..KD-S9) and the
AGENTS.md TDD rules apply; this plan names only what L5 adds.

## Goal Capsule

- **Objective:** under `WAIT_ON_ENGINE=rust`, the http cells L4 left on undici
  (`ca`/`cert`/`key`/`passphrase`, `strictSSL: true`, `proxy` object or `false`,
  `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` env, `http://unix:<sock>:<path>` including
  Windows named pipes) are answered by the Rust engine with the JS engine's observable
  behavior, or are a documented, reasoned parity delta. The JS engine is unchanged.
- **Proves:** PO9 (TLS/proxy parity baseline against undici `Agent`/`ProxyAgent`/
  `EnvHttpProxyAgent`), per the spine lane table: "`ca/cert/key/passphrase/strictSSL`,
  `proxy` + env proxies, `http://unix:` (and Windows pipes) pass; parity deltas documented".
- **Authority:** spine plan KDs > AGENTS.md > lane issue #57 > this plan.
- **Stop conditions:** a needed `.github/workflows/` change (report to PM, do not edit);
  any need to grow `test/rust-pending.js`; a merge of `origin/spike-next-rs` that
  reshapes `createHTTP$`/`routesHttpToRust` (re-thread, rerun both gates).

## Product Contract

### Starting state (from L4, merged in #70)

- `routesHttpToRust` in `lib/wait-on.js` sends an http resource to the addon's
  `HttpChecker` only when no L5 condition holds; the L5 rows are listed in
  `docs/guides/architecture.md` ("http(s) (L4)" routing table).
- `crates/wait-on-core/src/http.rs` builds reqwest (rustls + ring, no OpenSSL) with
  `no_proxy()` and `tls_danger_accept_invalid_certs(true)`; no TLS client options, no
  proxy, no unix transport.
- JS reference behavior is `buildDispatcher` in `lib/wait-on.js`: TLS options ride on
  `connect`; `socketPath` always uses a plain `Agent` (never proxied); proxy object →
  `ProxyAgent` (axios-shape normalization: protocol with/without `:`, bare IPv6 host
  bracketed, percent-encoded credentials; TLS options as `requestTls`); `proxy: false` →
  plain `Agent`; proxy unset → `EnvHttpProxyAgent`.
- `test/rust-pending.js` is `[]`.

### Requirements

**Routing**

- R-L5-1. Every L5 row of the routing table routes to Rust when the addon is loaded,
  except rows the plan carves out with a recorded reason (R-L5-9). The URL-with-userinfo
  row stays JS (not an L5 row).
- R-L5-2. Under `WAIT_ON_ENGINE=js` nothing changes: `npm test` green, no new behavior.

**TLS**

- R-L5-3. `strictSSL: true` verifies the server chain and hostname under Rust; a
  self-signed target fails, a publicly trusted one passes. Default roots match what
  undici trusts (Node's bundled Mozilla set), not the OS store (KD-S6).
- R-L5-4. `ca` (string, Buffer, and whatever array forms the schema admits) is honored
  with `strictSSL: true` exactly as Node treats it (a supplied `ca` replaces the default
  roots), so a self-signed target with its matching `ca` passes.
- R-L5-5. `cert` + `key` present a client certificate; a server requiring client auth
  accepts the Rust request when they are supplied and rejects it when absent.
  `passphrase` decrypts an encrypted `key`, or the cell is a documented delta that stays
  on JS (see Outstanding Questions).

**Proxy**

- R-L5-6. `proxy` object routes through that proxy (http target via absolute-form
  request, https target via CONNECT tunnel), with the same normalization as
  `buildDispatcher` and Basic proxy credentials. `proxy: false` connects directly even
  when env proxies are set. A malformed proxy object reaches the callback, never a
  synchronous throw (existing test).
- R-L5-7. With `proxy` unset, `HTTP_PROXY`/`http_proxy`, `HTTPS_PROXY`/`https_proxy`
  and `NO_PROXY`/`no_proxy` select the proxy per target scheme and host the way
  `EnvHttpProxyAgent` does (including which variable an https target reads and how
  `NO_PROXY` matches). Differences are deltas (R-L5-10).

**http-over-unix**

- R-L5-8. `http://unix:<sock>:<path>` in both URL forms (short and absolute) and
  `http-get://unix:...` connect over the unix socket on POSIX and the named pipe on
  Windows, with the request path/`Host` the JS path sends; a unix resource is never
  proxied, even with env proxies set (existing test).

**Proof and gates**

- R-L5-9. The option × scheme × env-proxy matrix below is enumerated in the plan; each
  reachable cell has a test on both engines, or a reasoned carve-out written in the plan.
- R-L5-10. Every deliberate JS/Rust difference is listed under "Deliberate JS vs Rust
  differences" in `docs/guides/architecture.md`, and the routing table there reflects
  post-L5 routing.
- R-L5-11. Tests of routed paths prove the path taken: under `rust*` the counting addon
  saw the check; proxy tests assert the stub proxy counted the request or CONNECT; unix
  tests assert the socket server saw the request; direct (`proxy: false`, `NO_PROXY`)
  tests assert the proxy saw nothing.
- R-L5-12. `npm test` and `npm run ci:rs` green; `test/rust-pending.js` does not grow
  (it is empty, so "shrink" means nothing is added).

### Input matrix (to be completed in planning)

Dimensions: target scheme (`http`, `https`, `http://unix:` socket, Windows pipe) ×
TLS (`none`, `strictSSL: true`, `ca`, `cert`+`key`, `cert`+`key`+`passphrase`) ×
proxy option (unset, object http, object with auth, `false`) × env (none, `HTTP_PROXY`,
`HTTPS_PROXY`, `NO_PROXY` matching target, lowercase spellings) × engine (JS, Rust).
Reduction hints, each to be confirmed or rejected by a test or recorded as a carve-out:

- TLS options on an `http` target have no effect on either engine (one test, not a row per option).
- unix/pipe targets ignore proxy option and env (one test per proxy source).
- TLS options apply to the target through a CONNECT tunnel (`requestTls`), not to the
  proxy connection; the https × proxy × `strictSSL`/`ca` cells are the ones #238 missed
  and are not carved out.

### Acceptance Examples

- AE-L5-1. `rust-strict`, self-signed https server, `strictSSL: true` → rejects on
  timeout; same with its `ca` → resolves; counting addon saw the check both times.
- AE-L5-2. `rust-strict`, `HTTP_PROXY` pointing at a stub proxy, plain http target →
  resolves via the proxy (stub counted one request); with `NO_PROXY` matching the target
  → resolves direct (stub counted zero).
- AE-L5-3. `rust-strict`, https target, `proxy: { host, port }` stub CONNECT proxy,
  `strictSSL: true` + matching `ca` → resolves; stub counted a CONNECT.
- AE-L5-4. `rust-strict`, `http://unix:<sock>:/health` with `HTTP_PROXY` set → resolves;
  socket server saw `/health`. Windows: same over `\\.\pipe\...`.

### Scope Boundaries

- Not L5: `command:` (L6), polling loop in Rust (L7), parser differential (L8),
  prebuild matrix (L9). No change to the JS engine's behavior or option schema.
- No new runtime npm dependency; new Rust crates only as needed and `cargo deny` clean.
- No `.github/workflows/` edits.

### Outstanding Questions (each names the test that answers it)

- OQ1. Encrypted private keys: rustls does not read encrypted PEM. Decrypt in Rust (e.g.
  a pkcs8 decryption crate) or keep `passphrase` cells on JS as a delta? Answered by the
  `passphrase` × https test under `rust-strict` (counting addon shows which engine ran).
- OQ2. Windows named pipes: does reqwest's unix transport cover pipes, or does the pipe
  cell need a custom connector or a JS carve-out? Answered by the named-pipe test on the
  Windows `rust` CI row.
- OQ3. Root store: `webpki-roots` vs Node's `tls.rootCertificates` passed from JS. Answered
  by a `strictSSL: true` test against a target signed by a CA only in one of the sets, or
  a carve-out recorded if no such target is testable offline.
- OQ4. `EnvHttpProxyAgent` edge semantics (https target with only `HTTP_PROXY`; `NO_PROXY`
  with port, wildcard, leading dot; upper vs lower case precedence) vs reqwest's env
  parsing. Answered by one env test per case on both engines.
- OQ5. `key` accepts `Joi.object()` in the schema; what shape reaches the Rust path, and is
  it reachable? Answered by a validation/API test, or a carve-out if unreachable.

## Resume notes

<!-- Lane worker notes go here only. -->
