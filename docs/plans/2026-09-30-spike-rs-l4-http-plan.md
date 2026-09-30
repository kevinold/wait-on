---
title: "L4: http(s) HEAD/GET checks and validateStatus over a threadsafe callback"
type: feat
date: 2026-09-30
topic: rust-port
spine: kevinold/wait-on#35
sub_issue: kevinold/wait-on#56
lane: L4
branch: rs-56-http
base: spike-next-rs
parent_plans:
  - docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
  - docs/plans/2026-09-28-1239-feat-rust-port-plan.md
---

# L4: http(s) HEAD/GET checks + `validateStatus` over a threadsafe callback

Requirements-only lane plan for sub-issue #56. Spine rules (KD-S1..KD-S9) and AGENTS.md
TDD rules apply; this plan names only what L4 adds.

## Goal

Prove PO2 (the riskiest parity item): under `WAIT_ON_ENGINE=rust`, the `http:`/`https:`
(HEAD) and `http-get:`/`https-get:` (GET) resource checks run in Rust, and a JS
`validateStatus` is consulted from Rust through a napi threadsafe function without
deadlock under `simultaneous`. JS still orchestrates polling (KD-S3); Rust answers
"is this HTTP resource ready?".

## Scope

In:

- Rust HTTP check in `crates/wait-on-core` using `reqwest` + `rustls` + `tokio` (KD-S6,
  no OpenSSL): HEAD for `http(s):`, GET for `http(s)-get:`.
- Options honored in Rust: `headers`, `auth` (basic, `username`/`password`),
  `httpTimeout` (bounds request and GET body read, like the JS path), `followRedirect`
  (true follows 3xx to final status; false leaves the 3xx as the status), default
  readiness 2xx.
- `validateStatus`: when present, Rust calls the JS function with the status via a napi
  threadsafe function and uses its (truthy) result; a throwing `validateStatus` counts as
  not ready (JS parity).
- `reverse` stays correct (JS pipeline negates the Rust result as it does today).
- Binding in `crates/wait-on-napi`: one async export for the HTTP check, returning a
  Promise to JS. Additive registration only.
- `lib/wait-on.js`: when the Rust engine is loaded, `createHTTP$` asks the addon instead
  of undici, for the cells in scope. Cells owned by L5 (any of `ca`, `cert`, `key`,
  `passphrase`, `strictSSL: false`, `proxy`, env proxies `HTTP_PROXY`/`HTTPS_PROXY`,
  `http://unix:` URLs) keep using the JS check. The JS engine's behavior is unchanged.
- FFI overhead measurement: per-check cost of the Rust path (incl. threadsafe
  `validateStatus` round trip) vs the JS path against a local server, recorded in
  `docs/guides/architecture.md` with how to rerun it (script under `benchmarks/`).
- Guides: `docs/guides/architecture.md` (HTTP in Rust, routing table, overhead numbers),
  and any `testing.md`/`contributing-dual-engine.md` changes the work needs.

Out: TLS options, proxies, unix sockets/named pipes (L5); moving the polling loop (L7);
multi-target prebuilds (L9).

## Requirements

- R-L4-1. Under `WAIT_ON_ENGINE=rust-strict`, every in-scope HTTP test in
  `test/api.mocha.js` and the CLI suites passes, and it is proven the Rust path ran (not
  a silent JS fallback).
- R-L4-2. `validateStatus` from JS decides readiness for Rust checks (AE2: 200 passes,
  204 does not when `validateStatus: s => s === 200`).
- R-L4-3. No deadlock or starvation when several HTTP resources call `validateStatus`
  concurrently under `simultaneous` > 1, and with `simultaneous: 1`.
- R-L4-4. `httpTimeout` is enforced by Rust: a server that does not respond in time makes
  the check fail and wait-on time out, same as JS.
- R-L4-5. Out-of-scope cells (L5) still behave exactly as today under the Rust engine.
- R-L4-6. JS engine behavior unchanged; `npm test` green; `ci:rs` green (fmt, clippy
  `-D warnings`, `cargo test`, `cargo deny`, mocha under `rust-strict`).
- R-L4-7. The pending list (`test/rust-pending.js`) does not grow; any entry L4 can
  clear is removed.
- R-L4-8. FFI overhead number recorded in `docs/guides/architecture.md`.

## Test matrix (named tests, written before code)

Each reachable cell gets a test or a reasoned carve-out. Rust-path proof: a test hook or
observable signal (e.g. the addon's check being called, or a Rust-only header/marker)
asserts the Rust check handled the request under `rust-strict`.

| Cell | Test (behavior) |
|---|---|
| HEAD 2xx later | existing `should succeed when http resources are become available later` under rust |
| GET 2xx later | existing `should succeed when http GET resources become available later` |
| HEAD / GET via redirect, `followRedirect` true | existing redirect tests (both methods) |
| `followRedirect: false` + 3xx | new: should time out when redirect not followed (HEAD and GET) |
| 404 → timeout | existing `should timeout when an http resource returns 404` |
| `httpTimeout` | existing `should timeout when an http resource does not respond before httpTimeout` |
| `headers` | new/existing: server asserts header received, HEAD and GET |
| `auth` | new/existing: server asserts basic auth header, HEAD and GET |
| `validateStatus` 401 accepted | existing `should succeed when custom validateStatus fn is provided http resource returns 401` |
| `validateStatus` rejects 2xx (AE2 204) | new: should time out when validateStatus rejects the status |
| `validateStatus` throws | new: should treat a throwing validateStatus as not ready |
| `validateStatus` × `simultaneous` (1 and >1, several resources) | new: should succeed without deadlock |
| reverse × http | new or existing reverse http test under rust |
| Rust path ran | new: under rust-strict, in-scope HTTP check is served by the addon |
| L5 cell stays on JS | new: e.g. `strictSSL:false`/`proxy`/`http://unix:` still pass under rust via JS |
| Rust unit tests | `cargo test` in `wait-on-core` for status/redirect/timeout logic |

## Risks / outstanding questions (each answered by a named test)

- Threadsafe callback deadlock when the JS thread awaits the addon Promise while Rust
  waits on the JS callback → answered by the `validateStatus` × `simultaneous` test.
- tokio runtime lifecycle inside the addon (one shared runtime; process must still exit
  under `mocha --exit` and CLI) → answered by CLI subprocess HTTP tests exiting 0/1 on
  time under rust-strict.
- Redirect semantics differ between reqwest and undici (`manual` returns the 3xx) →
  `followRedirect: false` test.
- Dependency weight (`reqwest`, `rustls`, `tokio`) must pass `cargo deny` → `ci:rs`.

## Resume notes

<!-- Lane worker notes go here only. -->
