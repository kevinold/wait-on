---
title: "Redirect and header parity between undici dispatcher.request and reqwest: per-hop accept-encoding, 21 redirects, the empty query, and body.destroy()"
date: 2026-10-17
last_updated: 2026-10-24
category: integration-issues
module: lib/engine-js.js
problem_type: integration_issue
component: service_layer
severity: medium
symptoms:
  - "A server that answers 406 without accept-encoding times out on both engines; fetch and 9.x resolved it"
  - "A 2xx after 21 redirects times out (9.x followed 21); on undici 8.11 a 2xx after exactly 20 also timed out with max redirects"
  - "A redirect Location of /final? is requested as /final"
  - "The Rust engine does not follow a 300 with a Location, sends a Referer on redirect hops, and never reports a redirect loop"
  - "A non-2xx GET whose body never ends holds the only poll slot until the wait's timeout instead of being retried at the next interval"
  - "Mocha bad-port tests pass locally while silently skipped, because macOS XQuartz holds port 6000"
root_cause: wrong_api
resolution_type: code_fix
applies_when:
  - "Changing either engine's HTTP probe: redirect following, default headers, or how a response body is released"
  - "Setting maxRedirections on undici's redirect interceptor, or widening the undici range"
  - "Reaching for reqwest's redirect::Policy when a header or path must change per hop"
  - "Writing a test that must listen on a WHATWG Fetch bad-list port"
related_components: [crates/wait-on-core/src/http.rs, test/helpers/cli-conformance.js, test/helpers/stub-proxy.js, features/support/servers.js, test/https-proxy.mocha.js]
tags: [undici, reqwest, fetch-parity, redirect, accept-encoding, bad-port, js-engine, rust-engine, spike-next-rs]
---

# Redirect and header parity between undici dispatcher.request and reqwest

## Problem

PR #110 (issue #104) moved the JS engine's HTTP probe from undici `fetch` to `dispatcher.request` so it can reach Fetch bad-list ports such as 6000. fetch had done several things implicitly that `dispatcher.request` does not, and upstream review (jeffbski/wait-on#262) kept finding them. PR #112 (issue #111) ports every upstream fix to the spike, and brings the Rust engine (reqwest) to the same behaviour, which turned out to need more than the JS side.

## Symptoms

- A server that answers 406 when a request has no `accept-encoding` never becomes ready on either engine. fetch sent one; `dispatcher.request` and reqwest (built without compression features) send none.
- A 2xx after 21 redirects times out. 9.x (follow-redirects, `maxRedirects` 21) followed it.
- A `302` to `/final?` is requested as `/final`.
- Rust only: a `300` with a `Location` is reported as the final status, every hop carries a `Referer`, and a redirect cycle runs to the hop cap instead of failing as a loop.
- An `http-get:` resource that first answers 503 with a body it never ends, with `simultaneous: 1` and no `httpTimeout`, never recovers.
- The bad-port mocha tests listened on 6000 and called `skip()` on `EADDRINUSE`. On a Mac where XQuartz holds 6000, the whole describe was pending and looked green.

## What Didn't Work

- `maxRedirections: 20`, then `21` "because undici counts the final response". That was true through undici 8.11.2 only. undici 8.12.0 (nodejs/undici#5879) counts only a redirect it would follow, so the same `21` started resolving a 2xx after 21 redirects. CI ran `npm ci` on the lockfile (8.11.2) and passed while any fresh install resolved 8.12.x under the `^8.10.2` range and behaved differently.
- Setting accept-encoding once on the request. fetch picks it per request from that request's own URL, so an http to https redirect changes it. A header set before the redirect interceptor is sent unchanged on every hop.
- Building each hop's path from `pathname + search`. `search` is `''` for an empty `?`, so the `?` is lost. undici's redirect handler does exactly this.
- Wrapping the redirect handler in a JS `Proxy` to watch each hop's response. `ProxyAgent` assigns `handler.onResponseStart` on the handler it gets; without a `set` trap that lands on the shared redirect handler, the two wrappers call each other, and every proxied request fails with `RangeError: Maximum call stack size exceeded`. undici's `DecoratorHandler` asserts `onResponseStart` runs once, which a `100 Continue` breaks.
- Matching on Rust with reqwest's `redirect::Policy`. `Policy::limited(21)` gets the cap right, but a policy can only follow, stop or error: it cannot set a header per hop. reqwest delegates to tower-http's `FollowRedirect`, which follows only `301 302 303 307 308`, not `300`. reqwest adds a `Referer` on each hop by default, and `Policy::limited` has no loop check. `docs/guides/architecture.md` had claimed both engines follow `300`; the Rust test `multiple_choices_with_location_is_followed` disproved it.
- `await body.dump()` on a rejected response. `dump()` waits for the body to end, so a body that never ends blocks the `finally` and, with `simultaneous: 1`, every later poll.
- Hardcoding 6000 with skip-on-`EADDRINUSE` in mocha. It never fails, so nobody notices the skip.

## Solution

**JS engine** (`lib/engine-js.js`). The undici floor is `^8.12.0` (lockfile 8.12.3), and the request dispatcher composes three interceptors (`lib/engine-js.js:217-219`):

```js
const requestDispatcher = dispatcher.compose(
  callerNames.includes('accept-encoding') ? null : fetchAcceptEncoding(callerNames.includes('range')),
  followRedirect ? keepRedirectQuery() : null,
  followRedirect ? interceptors.redirect({ maxRedirections: 21, throwOnMaxRedirect: true }) : null
);
```

- `compose` applies the first-listed interceptor innermost. `RedirectHandler` dispatches every hop through the dispatch it was given, so the two listed before `interceptors.redirect` see every hop with that hop's `origin` and `path`. Each returns a copy of `opts`; mutating it leaks into the redirect handler's own state.
- `fetchAcceptEncoding` (`lib/engine-js.js:276`) sends `br, gzip, deflate, zstd` over https, `gzip, deflate` otherwise, `identity` when the caller set `Range`. The body is destroyed, never decoded, so any value is safe.
- `keepRedirectQuery` (`lib/engine-js.js:253`) records, per redirect handler (a `WeakMap` key, since one handler dispatches every hop of one request), the raw path a 3xx `Location` resolves to, and sends it on the next hop only when that hop's origin matches. It observes the response through a plain object that forwards each handler hook, not a `Proxy`.
- `requestPath` (`lib/engine-js.js:242`) slices `href` after `origin`, minus any `#fragment`, for the first request.
- A rejected body is `body.on('error', () => {}).destroy()` (`lib/engine-js.js:313`). `destroy()` emits a `RequestAbortedError`; the empty listener swallows it, as fetch's `body.cancel()` was silent.

**Rust engine** (`crates/wait-on-core/src/http.rs`). The client never follows redirects (`Policy::none()`, `http.rs:88`). `HttpChecker::send` follows them itself, mirroring undici's `redirect-handler.js`:

- Redirectable: `300 301 302 303 307 308` with a `Location` (`REDIRECTS`, `http.rs:64`). Anything else, including a 3xx without `Location`, is the response the status rule sees.
- Cap: `MAX_REDIRECTS = 21` (`http.rs:62`); a 22nd redirect fails with `max redirects`.
- accept-encoding is set inside the loop from each hop's scheme (`http.rs:174`), unless the caller sent one.
- Loop check (`http.rs:208`): a history of visited URLs, the first as sent and each later one as undici names it (`loop_key`, `http.rs:280`: no fragment, an empty `?` dropped). A target already in the history fails with `Redirect loop detected`.
- Headers: `host` dropped on every hop; `authorization`, `cookie` and `proxy-authorization` dropped when the origin changes (`http.rs:215`), as undici's `cleanRequestHeaders`. No `Referer`.
- One deadline for the whole chain and the final body (`http.rs:156`), each hop sending with the time left, as the client-level timeout did when reqwest followed the chain.
- `Url::join` keeps an empty `?` from the `Location`, so the Rust side needed no equivalent of `keepRedirectQuery`.

**Bad-port tests**: `test/helpers/cli-conformance.js` holds the one `FETCH_BAD_PORTS` list and `listenOnBadPort(server, host)`, which tries each port, moves on after `EADDRINUSE`/`EACCES`, rethrows anything else, and throws only when every port is taken.

## Why This Works

- Every behaviour fetch computed per request now runs per hop on both engines: on JS because the interceptors sit inside the redirect interceptor, on Rust because the hop loop is ours.
- With the undici floor at 8.12, no install can resolve the old redirect counting, so `maxRedirections: 21` means 21 followed for every consumer, matching the Rust `MAX_REDIRECTS`.
- `destroy()` releases the socket at once, so the poll slot frees and the next interval probes again.

## Known delta

A redirect from a path to the same path plus an empty `?` (`/selfq` to `/selfq?`) fails as `Redirect loop detected` on both engines. undici builds its loop history from `pathname + search`, so the target equals the current URL; `keepRedirectQuery` changes only what is sent and cannot reach that check. fetch followed it, and 9.5.1 failed differently (dropping the `?` until its cap). The Rust loop reproduces undici's comparison on purpose, so both engines agree. Tests: `should stop a redirect to the same path plus an empty query as a loop` (mocha, both engines), `redirect_to_a_visited_url_is_a_loop` and `http_forward_redirect_to_the_same_path_with_an_empty_query_is_a_loop` (Rust).

## Prevention

- When a behaviour depends on a dependency's edge semantics, the range floor must be the version whose semantics the tests assert. Install the newest version the range allows and rerun the boundary tests, not only the locked one.
- When an engine delegates redirects to a library, list what fetch did per hop (headers, path, method, credentials, loop check, cap) and test each across a hop on both engines. reqwest's policy hides three differences from undici behind one call.
- Pin the boundaries in `test/https-proxy.mocha.js`, which runs under both engines through `outcome()`: 20 and 21 redirects resolve, 22 fail; `300` is followed; a 3xx without `Location` reaches `validateStatus`; accept-encoding per scheme, per hop, with `followRedirect: false`, under `Range`, and with a caller value; `/final?` after a redirect, cross-origin, and through the proxy option and `HTTP_PROXY`; a non-2xx body that never ends does not stall; `opts.auth` is not sent to another origin. Rust unit tests in `src/http.rs` and `tests/https.rs` cover the same rows, and `features/api-http-options.feature` pins accept-encoding, the 21/22 boundary and the empty-`?` hop for consumers.
- The stub proxy (`test/helpers/stub-proxy.js`) must forward the path as received; `http.request(url)` alone drops an empty `?`.
- A guard test that passes on its first run proves nothing until you break the code it guards. The cross-origin guard in `keepRedirectQuery` (saved path reused only on the same origin) is not reached by `should not reuse a saved empty-query path on a later cross-origin hop`, because each 3xx overwrites the saved path; removing the guard keeps that test green. The guard is defensive.
- Never listen on a fixed bad-list port in a test. Call `listenOnBadPort`.
