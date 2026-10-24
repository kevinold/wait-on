# Concepts

> Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Engines

### Engine
Which implementation runs a wait: the pure-JS engine (`lib/engine-js.js`, the default) or the Rust engine (the napi addon), chosen by `WAIT_ON_ENGINE` (`js`, `rust`, `rust-strict`).

`rust-strict` fails instead of falling back when the addon cannot load; it does not mean every wait runs in Rust (see Route).

### Route
Which engine actually answered one wait. Even under `rust-strict`, an https target behind an env proxy and a URL with userinfo route to the JS engine (`routesHttpToRust`).

### Route proof
Evidence of the route taken, recorded by the `NODE_OPTIONS=--require` preload (`features/support/proof-preload.js`): every dlopened `.node` file and whether `lib/engine-js.js` loaded, written at process exit and judged across all of a run's processes.

### Host prebuild
The addon built for the current machine at `prebuilds/<host>/wait-on.node` by `npm run build:napi`. The counting addon delegates to it, so rust-strict mocha runs need it.

### Counting addon
`test/fixtures/counting-addon.js`, the test stand-in loaded through `WAIT_ON_NATIVE_LIBRARY_PATH` that records every `wait` call to prove routing, and answers canned results when no host prebuild exists.

## HTTP checks

### HTTP probe
The check an `http:`/`https:` (HEAD) or `http-get:`/`https-get:` (GET) resource runs on each poll. The JS engine uses undici's request API, not `fetch`; the Rust engine uses reqwest.

Both engines keep fetch's behavior where 9.x users relied on it: they send fetch's default `accept-encoding` on every redirect hop, keep an empty `?` from a redirect's `Location`, and drop a rejected response's body at once instead of waiting for it to end, so a body that never ends cannot hold the poll. Both follow redirects up to 9.x's cap, not fetch's, and fail the check on one more.

### Redirect hop
One request in a followed redirect chain, sent to the URL the previous response's `Location` named. Anything chosen from a request's own URL (the accept-encoding, the path as written, whether credentials may go along) is chosen again for each hop, and a hop whose target was already visited ends the chain as a redirect loop instead of being sent.

### Fetch bad-port list
The WHATWG Fetch list of ports (e.g. 5060, 5061, 6000, 6665-6669, 10080) that any `fetch`, undici's included, refuses before connecting. An HTTP probe must reach them. Tests listen on the first free port from the list, never a fixed one, because other software often holds one of them.

## Verification

### Consumer contract
`features/*.feature`, run by cucumber-js against the packed, installed package under `js` and `rust-strict` (`npm run contract`); `@engine` scenarios also run in cucumber-rs. It allows no skipped scenario.

### Dependents run
Running a published dependent's own test suite against the packed tarball under each engine, compared with its pinned published wait-on as a baseline (`npm run dependents`); failing only on the tarball is a regression.

### Window-equals-interval boundary
A `file:` wait whose `window` is raised to `interval` resolves after one or two intervals depending on stat and timer jitter; timing assertions must avoid it.
