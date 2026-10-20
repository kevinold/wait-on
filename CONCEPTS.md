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
The check an `http:`/`https:` (HEAD) or `http-get:`/`https-get:` (GET) resource runs on each poll. The JS engine uses undici's `dispatcher.request`, not `fetch`; the Rust engine uses reqwest.

### Fetch bad-port list
The WHATWG Fetch list of ports (e.g. 5060, 5061, 6000, 6665-6669, 10080) that any `fetch`, undici's included, refuses before connecting. An HTTP probe must reach them (#104, upstream #260).

## Verification

### Consumer contract
`features/*.feature`, run by cucumber-js against the packed, installed package under `js` and `rust-strict` (`npm run contract`); `@engine` scenarios also run in cucumber-rs. It allows no skipped scenario.

### Dependents run
Running a published dependent's own test suite against the packed tarball under each engine, compared with its pinned published wait-on as a baseline (`npm run dependents`); failing only on the tarball is a regression.

### Window-equals-interval boundary
A `file:` wait whose `window` is raised to `interval` resolves after one or two intervals depending on stat and timer jitter; timing assertions must avoid it.
