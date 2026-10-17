---
title: "Probing with undici's dispatcher.request instead of fetch needs maxRedirections 21 and body.destroy(), not dump()"
date: 2026-10-17
category: integration-issues
module: lib/engine-js.js
problem_type: integration_issue
component: service_layer
severity: medium
symptoms:
  - "A 2xx reached after exactly 20 redirects times out on the JS engine (verbose log: max redirects); fetch and the Rust engine resolve it"
  - "A non-2xx GET whose body never ends holds the only poll slot until the wait's timeout instead of being retried at the next interval"
  - "Mocha bad-port tests pass locally while silently skipped, because macOS XQuartz holds port 6000"
root_cause: wrong_api
resolution_type: code_fix
applies_when:
  - "Changing the JS engine HTTP probe's redirect interceptor or how it releases a response body"
  - "Writing a test that must listen on a WHATWG Fetch bad-list port"
related_components: [crates/wait-on-core/src/http.rs, test/helpers/cli-conformance.js, features/support/servers.js, test/https-proxy.mocha.js]
tags: [undici, fetch-parity, redirect, bad-port, js-engine, spike-next-rs]
---

# Probing with undici's dispatcher.request instead of fetch needs maxRedirections 21 and body.destroy(), not dump()

## Problem

PR #110 (issue #104) moved the JS engine's HTTP probe from undici `fetch` to `dispatcher.request` so it can reach Fetch bad-list ports such as 6000. That first version used `maxRedirections: 20` and `body.dump()`. Both differ from what fetch did, and review of the upstream port (jeffbski/wait-on#262) found them after it shipped. Issue #111 ported the upstream fixes to the spike.

## Symptoms

- `waitOn({ resources: ['http://host/'] })` where the server answers 302 twenty times and then 200: rejects with `Timed out waiting for` on the JS engine. Before the change, fetch resolved it, and the Rust engine still does.
- An `http-get:` resource that first answers 503 and writes a body it never ends, with `simultaneous: 1` and no `httpTimeout`, never recovers. The wait times out even after the server turns healthy.
- The bad-port mocha tests listened on 6000 and called `skip()` on `EADDRINUSE`. On a Mac where XQuartz holds 6000, the whole describe was pending and looked green.

## What Didn't Work

- `maxRedirections: 20`, chosen because fetch's limit is 20. undici's redirect interceptor counts the final response against the cap, and fetch does not. So 20 is one too few.
- `await body.dump()` on a rejected response, chosen because it returns the connection to the pool. `dump()` waits for the body to end. A body that never ends blocks the `finally` block, and with `simultaneous: 1` it blocks every later poll too.
- Hardcoding 6000 with skip-on-`EADDRINUSE` in mocha. It never fails, so nobody notices the skip. The contract (`features/support/servers.js`) had already solved this with a first-free loop, but the mocha suites did not reuse it.

## Solution

`lib/engine-js.js:212-214`:

```js
// 21: undici counts the final response against maxRedirections, fetch does not.
? dispatcher.compose(interceptors.redirect({ maxRedirections: 21, throwOnMaxRedirect: true }))
```

`lib/engine-js.js:262`, in the `finally` that runs whenever the body was not consumed:

```js
if (!consumed) body.on('error', () => {}).destroy();
```

`destroy()` emits a `RequestAbortedError`. The empty listener swallows it, the way fetch's `body.cancel()` was silent.

Bad-port tests: `test/helpers/cli-conformance.js:94-96` now holds the one `FETCH_BAD_PORTS` list and `listenOnBadPort(server, host)`. It tries each port, moves on after `EADDRINUSE`/`EACCES`, rethrows anything else, and throws only when every port is taken. `features/support/servers.js` (`badPortHttpServer`), `test/https-proxy.mocha.js`, and `test/cli.mocha.js` all use it. A failed `server.listen()` can be retried on the same server object, so the helper takes a prebuilt http or https server.

## Why This Works

- undici's redirect handler throws once the redirect history reaches `maxRedirections`, and the final 2xx response counts toward that. 21 lets exactly 20 redirects reach their 2xx, and a 21st redirect still fails. reqwest's `Policy::limited(20)` (`crates/wait-on-core/src/http.rs:74`) counts only the redirects, so the Rust engine needed no change.
- `destroy()` releases the socket at once whatever the body does, so the poll slot frees and the next interval probes again.
- A first-free port always yields a running test, or a loud error when all 14 ports are held.

## Prevention

- Pin both boundaries with tests that run under both engines. `test/https-proxy.mocha.js` covers: a 2xx after exactly 20 redirects resolves, a non-2xx body that never ends does not stall, and `opts.auth` is not sent to another origin after a redirect. The Rust side is `redirect_chain_of_exactly_20_hops_is_ready` next to `redirect_chain_over_20_hops_is_not_ready`.
- The cross-origin auth test passes on its first run, because the interceptor already strips the header. It goes red only when you make `shouldRemoveHeader` in undici's `redirect-handler.js` return `false`. Prove a guard test that way, then restore `node_modules`.
- Never listen on a fixed bad-list port in a test. Call `listenOnBadPort`.
- When replacing `fetch` with a lower-level undici API, list everything fetch did for you, such as the redirect counting, body cancel, the default headers, and the userinfo refusal. Give each one a test at the `waitOn` front door.
