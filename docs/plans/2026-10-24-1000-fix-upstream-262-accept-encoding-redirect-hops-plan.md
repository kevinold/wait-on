---
title: Port the upstream accept-encoding, 21-redirect and empty-query hop fixes - Plan
type: fix
date: 2026-10-24
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
issue: 111
upstream: jeffbski/wait-on#262
---

# Port the upstream accept-encoding, 21-redirect and empty-query hop fixes - Plan

Follow-up to `2026-10-17-1457-fix-upstream-262-redirect-body-fixes-plan.md`, same lane and PR (kevinold/wait-on#112). #112 holds jeffbski/wait-on#262 as of 3af9a0c. #262 has since gained 6dcb83e, 77340e5, 0470cd0, f87b530, 7c4d96a, 493301f, 32a82ab, 802877f, 2df3730 and 315ab8f (head). This plan ports them to the JS engine and brings the Rust engine to the same behaviour.

## Goal Capsule

- Objective: an http(s) check on either engine sends fetch's default `accept-encoding` per hop, follows 21 redirects and fails a 22nd, keeps an empty `?` from a redirect `Location`, follows `300`, hands a 3xx without `Location` to `validateStatus`, and stops a redirect to an already-visited URL as a loop the same way on both engines.
- Means: the upstream `lib/wait-on.js` diff applied to `lib/engine-js.js` (KTD1); undici floor `^8.12.0`, lockfile 8.12.3 (KTD2); the Rust checker follows redirects itself instead of through reqwest's policy (KTD3); upstream tests ported into the `Fetch bad-list ports (#104)` describe so they run through `outcome()` under both engines (KTD4); three contract scenarios (KTD5).
- Authority: the user's request for this run > upstream commits above > `AGENTS.md` > this plan.
- Stop conditions: a public API, CLI, schema or `index.d.ts` change; a `.github/workflows/` edit; a new runtime dependency (the undici floor raise is a version change of an existing one).

## Product Contract

### Requirements

- R1. accept-encoding: unless the caller sets one, each request and each redirect hop sends `gzip, deflate` over http, `br, gzip, deflate, zstd` over https, and `identity` when the caller set a `Range` header. A caller `accept-encoding` is sent on every hop instead. Holds with `followRedirect: false`.
- R2. A 2xx after 21 redirects resolves; a 22nd redirect fails (`max redirects`).
- R3. A redirect `Location` with an empty `?` (`/final?`, `/final?#frag`) is requested with the `?`, including cross-origin and through the proxy option and an env proxy. A path saved from one hop is never sent on a hop to another origin.
- R4. `300` with a `Location` is followed.
- R5. A 3xx without `Location` is the response `validateStatus` sees.
- R6. A redirect to a URL already visited in the chain fails as `Redirect loop detected`, compared the way undici's redirect handler does (`pathname + search`, so `/selfq` → `/selfq?` is a loop). Known delta from fetch and 9.x, recorded upstream (315ab8f); both engines match.
- R7. Each of R1-R6 passes under `WAIT_ON_ENGINE=js` and `rust`/`rust-strict` with routing proof.
- R8. Contract scenarios for accept-encoding, the 21/22 boundary and the empty `?` across a hop pass under `js` and `rust-strict`.

### Scope Boundaries

- No change to the user-agent delta (Rust sends none; JS sends `undici`).
- The body is never decoded, so no compression feature is enabled in reqwest.

## Planning Contract

### Key Technical Decisions

- KTD1. JS: `fetchAcceptEncoding` and `keepRedirectQuery` interceptors composed inside `interceptors.redirect` (listed before it), as upstream 32a82ab, so they run per hop. `requestPath` helper shared with the first request.
- KTD2. undici 8.12.0 (nodejs/undici#5879) counts only a redirect it would follow, so `maxRedirections: 21` means 21 followed; 8.11 counted the final response.
- KTD3. Rust: reqwest's redirect policy cannot set a header per hop, does not follow `300`, sends `Referer`, and has no loop check. `HttpChecker::send` runs `Policy::none()` and follows redirects in a loop: redirectable statuses `300 301 302 303 307 308` with a `Location`; `max redirects` on the 22nd; undici's loop history; `host` dropped on every hop and `authorization`, `cookie`, `proxy-authorization` dropped when the origin changes (undici `cleanRequestHeaders`); one deadline across the chain and the final body, as reqwest's client timeout did.
- KTD4. Tests go in `test/https-proxy.mocha.js`'s bad-port describe and use `outcome()`; Rust unit tests in `crates/wait-on-core/src/http.rs` (scripted `serve`) come first for each engine change.
- KTD5. Contract: extend `features/api-http-options.feature` with accept-encoding, the 21/22 boundary and the empty-`?` hop, backed by a redirect-chain server in `features/support/servers.js`.

## Implementation Units

### U1. 21 followed, 22nd fails
Files: `package.json`, `package-lock.json`, `crates/wait-on-core/src/http.rs`, `test/https-proxy.mocha.js`. Rust test (21 ready, 22 not) and mocha (20 and 21 resolve, 22 rejects) red first.

### U2. Rust follows redirects itself
Files: `crates/wait-on-core/src/http.rs`. Characterize cross-origin auth stripping and the chain-wide timeout first; red tests for `300` followed and the loop check; 3xx without `Location` pinned.

### U3. accept-encoding per scheme, Range and hop
Files: `lib/engine-js.js`, `crates/wait-on-core/src/http.rs`, `test/https-proxy.mocha.js`. Red on both engines first.

### U4. Empty `?` kept on hops
Files: `lib/engine-js.js`, `test/https-proxy.mocha.js`, `crates/wait-on-core/src/http.rs`. JS red first; Rust test pins it.

### U5. Contract scenarios and docs
Files: `features/api-http-options.feature`, `features/support/*.js`, `docs/guides/*`, `docs/solutions/*`, `CONCEPTS.md`.

## Verification Contract

`npm test`; `WAIT_ON_ENGINE=rust npm run test:mocha` with a fresh `npm run build:napi`; `cargo test`; `npm run contract`; `cargo test -p wait-on-features --test features`; `cargo deny check` for the lockfile; CI green on #112.

## Definition of Done

R1-R8 pass, gates green, PR #112 body updated, not merged.
