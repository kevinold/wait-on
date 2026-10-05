---
title: Shipping a napi addon in one npm package - bundle every target, then shrink it, and keep panic=unwind
date: 2026-10-05
category: best-practices
module: Cargo.toml
problem_type: tooling_decision
component: rust-engine
severity: medium
applies_when:
  - "Choosing how to deliver a native addon to npm consumers without install scripts"
  - "Tuning [profile.release] for the napi addon size"
  - "Considering panic = \"abort\" to save bytes"
related_components: [xtask/tests/release_profile.rs, xtask/src/package.rs, docs/guides/ci.md]
tags: [napi-rs, npm, packaging, lto, panic, size-budget, spike-next-rs]
---

# Shipping a napi addon in one npm package - bundle every target, then shrink it, and keep panic=unwind

## Context

npm cannot ship a host-only binary inside one package without an install script or a
download. npm RFC #519 (per-file os/cpu filtering) was closed unshipped; `os`/`cpu`/`libc` in
`package.json` filter whole packages, not files. The spike rules out install scripts
(`xtask/src/package.rs` refuses `preinstall`/`install`/`postinstall`/`prepare` and
`optionalDependencies`).

## Guidance

Two shapes work without scripts:

| shape | used by | cost |
|---|---|---|
| per-platform `optionalDependencies` packages | esbuild, swc, biome | N+1 packages to publish and version in lockstep |
| one package with every target's binary | better-sqlite3 13, sodium-native | every consumer downloads every target |

The spike keeps one bundle (KD1) and shrinks it instead. `[profile.release]` with
`lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `opt-level = "z"` took the CI-measured
packed tarball from 15,035,917 to 9,766,031 bytes (36.3 to 18.6 MB unpacked). The TLS/HTTP
stack (rustls, ring, reqwest, hyper) is about 40% of the addon.

Keep `panic` at its default, unwind. napi-rs wraps async polls in `catch_unwind`, so a Rust
panic rejects the `waitOn` promise. `panic = "abort"` saves about 0.5 MB more but turns any
panic into the death of the consumer's Node process. `xtask/tests/release_profile.rs` pins this.

## When to revisit

If the bundle outgrows its size budget even after tuning, the next step is per-platform
`optionalDependencies` packages, not install scripts.
