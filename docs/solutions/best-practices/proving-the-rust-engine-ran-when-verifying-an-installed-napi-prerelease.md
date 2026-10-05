---
title: Verifying a napi prerelease must prove the Rust path ran, not just that the check passed
date: 2026-10-01
category: best-practices
module: docs/guides/releasing.md
problem_type: best_practice
component: rust-engine
severity: high
last_updated: 2026-10-05
applies_when:
  - Verifying a wait-on prerelease from its installed tarball (lane L10, #62, or any later rs-* tag)
  - Writing a driver or test that claims "the Rust engine ran" under WAIT_ON_ENGINE=rust or rust-strict
  - Exercising TLS roots (ca) on the Rust http checker
  - Counting crates shipped in a multi-target napi addon, or lifecycle hooks in an npm tree
symptoms:
  - A rust-strict AE2 driver passes with identical predicate call counts whether Rust or JS ran the wait
  - An https check with ca but no strictSSL passes without verifying anything
  - cargo tree reports 100 third-party crates on darwin while the shipped addon set is 134
related_components: [lib/engine.js, lib/wait-on.js, lib/engine-rust.js, xtask/src/package.rs]
tags: [napi-rs, prerelease, rust-strict, engine-routing, tls, cargo-tree, supply-chain, spike-next-rs]
---

# Verifying a napi prerelease must prove the Rust path ran, not just that the check passed

## Context

Lane L10 (#62) verified the fork prerelease `rs-10.0.0-rc.1-832c588` end to end from its
installed tarball: checksum, install, strict load, AE1 in a container, and AE2 (a JS
`validateStatus` called by the Rust loop across the FFI boundary). A naive driver would have
passed while proving nothing. Plan doc review (the feasibility and adversarial reviewers) caught
traps 1 to 4 below before the run. The run itself did not catch them. The runbook, the AE2
driver, and the measured results are in
[`docs/guides/releasing.md`](../../guides/releasing.md#end-to-end-prerelease-verification).
This doc records why the driver has its current shape. It does not repeat the runbook.

Earlier lanes gated on the same pair (`npm test` and mocha under `WAIT_ON_ENGINE=rust-strict`), and for http they proved the path with stub servers that counted CONNECT tunnels (L4 and L5). All of that ran against the source tree. No lane ran the Rust engine from an installed package, and in these sessions "passes under rust-strict" stood in for "Rust ran". The L5 security review set the `strictSSL` semantics that trap 2 depends on (session history).

## Guidance

1. **`rust-strict` controls addon loading, not routing, so assert that the JS engine never loaded.**
   `resolveEngine` (`lib/engine.js:34-56`) only decides whether the addon loads. Under
   `rust-strict` a load failure throws (`lib/engine.js:45-46`, `:52`). Routing is a second
   decision, made for each wait, at `lib/wait-on.js:165-166`:
   `rust && rust.routable(validatedOpts, process.env) ? rust : require('./engine-js')`.
   `routable` (`lib/engine-rust.js:16-23`) returns false when any http resource must stay on
   undici, for example when an env proxy applies to an https target
   ([architecture.md, Engine selection and fallback](../../guides/architecture.md#engine-selection-and-fallback)).
   The JS engine calls `validateStatus` too (`lib/engine-js.js:177`, `:218`), so predicate call
   counts are the same on both paths. The proof: `engine-js` is required lazily, so after the
   wait, assert that no `require.cache` key ends with `wait-on/lib/engine-js.js` and none is
   under `node_modules/undici` or `node_modules/rxjs` (`docs/guides/releasing.md:121-124`).
   First unset `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` and their lowercase forms
   (`docs/guides/releasing.md:50`, `:144`).

2. **A TLS-roots check needs `strictSSL: true` as well as `ca`.** `WAIT_ON_SCHEMA` defaults
   `strictSSL` to false (`lib/wait-on.js:56`), and `rustTlsOptions` sets `roots` only when
   `strictSSL` is true (`lib/engine-rust.js:86-87`). With `ca` alone the check skips
   verification and still passes (`docs/guides/releasing.md:104`).

3. **Count a multi-target addon's crates with `--target all`.** `cargo tree -p wait-on-napi
   -e normal` resolves for the host target only and drops platform-gated crates such as
   `windows-sys`. On darwin it lists 100 third-party crates. With `--target all --prefix none`
   (deduplicated, minus `wait-on-core` and `wait-on-napi`) it lists 134. A derived figure such as
   "dev/build/xtask = `Cargo.lock` total minus runtime" then puts the missing 34 in the wrong
   bucket, and nothing reports an error. Correct split: 156 lock packages - 134 - 2 workspace
   crates = 20.

4. **Realpath checks must compare against the canonicalized project root.** On macOS a temp
   project under `/var/...` resolves to `/private/var/...`, and pnpm installs the addon under
   `node_modules/.pnpm/...`. A string prefix check against the raw root fails, and the
   tempting fix is to loosen the check until it accepts anything. `assert_installed_addon`
   (`xtask/src/package.rs:229-248`) canonicalizes the root, strips the Windows `\\?\` prefix
   (`strip_verbatim`, `:223-227`), and requires the realpath to start with the root and end
   with `prebuilds/<dir>/wait-on.node`.

5. **Count "declares a lifecycle hook" separately from "has an install-time hook".** In the
   installed runtime tree, `@hapi/tlds` and `undici` declare `prepare`. npm does not run
   `prepare` for registry installs. Today the two counts are 2 (declares any hook) and 0 (has an
   `preinstall`/`install`/`postinstall` hook). Merging them into one column overstates what runs
   at install time (`docs/guides/releasing.md`, Supply-chain delta).

## Why This Matters

A verification that passes on the fallback path is worse than having none. It records the Rust
engine as shipped and working when the JS engine did the work. Every trap above gives a green
result, never a red one, so the run has no signal that anything went wrong. The supply-chain
numbers feed priority #1 of the requirements plan (a smaller runtime tree). A host-only crate
count or a merged hook column would misstate that headline metric.

Measured on 2026-10-01 against `rs-10.0.0-rc.1-832c588` with the hardened driver: under
`rust-strict`, AE2 resolved the 200 case after 1 predicate call and rejected the 204 case
(`Timed out waiting for: https-get://localhost:<port>/health`) after 15. `engine-js`, `undici`
and `rxjs` never loaded, and the addon's realpath was inside the project.

## When to Apply

- Any manual or scripted verification of an `rs-*` prerelease, and any test (mocha under
  `WAIT_ON_ENGINE=rust-strict` included) that claims "ran on the Rust engine" when a JS fallback
  would give the same observable outcome.
- Any test of `ca`, `cert` or `key` on the Rust http checker.
- Refreshing the supply-chain delta tables in `docs/guides/releasing.md`.
- Adding a routing condition to `routable`/`routesHttpToRust`. Each new reason to fall back is
  one more way for a "Rust" check to pass on JS.

## Examples

Proof that the path ran, checked after the wait (from the AE2 driver, `docs/guides/releasing.md:121-124`):

```js
// engine-js (and with it undici and rxjs) is required lazily: absent means Rust ran the wait
const loaded = Object.keys(require.cache).filter((k) =>
  /wait-on[\\/]lib[\\/]engine-js\.js$|node_modules[\\/](undici|rxjs)[\\/]/.test(k)
);
```

TLS options that actually verify:

```js
const opts = {
  resources: [`https-get://localhost:${port}/health`],
  ca: pem('ca.pem'),
  strictSSL: true, // the schema default is false, and Rust gets TLS roots only with strictSSL
  validateStatus
};
```

Crate count:

```bash
cargo tree -p wait-on-napi -e normal --target all --prefix none   # dedupe: 134 third-party + 2 workspace
cargo tree -p wait-on-napi -e normal --prefix none                # host only: 100 on darwin (wrong)
```

Related:
[`napi-gnu-addon-glibc-floor-follows-the-build-runner.md`](napi-gnu-addon-glibc-floor-follows-the-build-runner.md)
(the other silent fallback: under `WAIT_ON_ENGINE=rust` a failed addon load falls back to JS),
[`porting-undici-tls-options-to-reqwest-rustls.md`](porting-undici-tls-options-to-reqwest-rustls.md).
Lane plan: `docs/plans/2026-09-30-spike-rs-l10-test-release-plan.md` (#62).

## Update 2026-10-05: per-wait routing, child processes, and the counting-addon trap

- `WAIT_ON_ENGINE=rust-strict` proves only that the addon loaded. Routing is per wait: an https
  target behind an env proxy and a URL with userinfo still run on the JS engine
  (`routesHttpToRust` in `lib/resources.js`). Assert the route per scenario.
- For child processes (CLIs, `command:` children, dependents' suites), ride a
  `NODE_OPTIONS=--require` preload that records `process.dlopen` calls and whether
  `lib/engine-js.js` entered `require.cache`, and writes them at exit
  (`features/support/proof-preload.js`). Judge all of a run's records together.
- `test/fixtures/counting-addon.js` delegates to `prebuilds/<host>/wait-on.node` when it exists
  and otherwise answers canned results. Without a host prebuild, about 18 rust-strict mocha tests
  fail for reasons unrelated to the change. Build it first (`npm run build:napi`; `ci:rs` does).

