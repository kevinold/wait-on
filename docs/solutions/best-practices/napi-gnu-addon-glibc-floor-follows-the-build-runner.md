---
title: A napi linux-gnu addon needs the glibc of the runner that built it
date: 2026-09-30
category: best-practices
module: scripts/ci-rs-package.js
problem_type: best_practice
component: rust-engine
severity: high
applies_when:
  - Choosing a glibc container image to load or test the linux-gnu prebuilds (AE1 cells, tester docs)
  - Adding Rust dependencies to crates/wait-on-core or crates/wait-on-napi (the glibc floor can rise without any build change)
  - Deciding which Linux distributions the Rust engine supports
symptoms:
  - "WAIT_ON_ENGINE=rust-strict: failed to load the native addon at .../prebuilds/linux-arm64/wait-on.node: /lib/aarch64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found"
  - Under WAIT_ON_ENGINE=rust the same host silently falls back to the JS engine
root_cause: config_error
related_components: [crates/wait-on-napi, lib/engine.js, .github/workflows/node.js.yml]
tags: [napi-rs, glibc, prebuilds, docker, musl, pnpm, read-only, spike-next-rs]
---

# A napi linux-gnu addon needs the glibc of the runner that built it

## Context

The `napi` CI job builds the two linux-gnu targets natively on `ubuntu-24.04` and `ubuntu-24.04-arm` (`.github/workflows/node.js.yml:74-75`), with no zig or older-glibc target. The addon therefore links against whatever glibc symbol versions its Rust dependencies pull in from that runner. The L1 addon (version and no-op only) needed `GLIBC_2.34`. After the tcp/socket lane added tokio-era crates, the same build needed `GLIBC_2.39`, with no change to the build itself.

The L9 plan picked `node:24-bookworm-slim` (Debian 12, glibc 2.36) for the read-only glibc container cell, from a measurement of the older addon. With the current addon, that image cannot load it: `rust-strict` fails with the `GLIBC_2.39' not found` error above, and `rust` falls back to JS without saying so (`lib/engine.js` `resolveEngine`).

## Guidance

- Measure the floor from the binary, not from memory, every time the crates change:

  ```bash
  strings prebuilds/linux-x64/wait-on.node | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1
  ```

- Pick glibc test images at or above that floor. `ci:rs:package` uses `node:24-trixie-slim` (glibc 2.41) for the glibc cell and `node:24-alpine` for musl (`scripts/ci-rs-package.js:108-111`); the musl addons are unaffected.
- Record the floor where testers read it (`docs/guides/releasing.md`), including the silent fallback under `rust`.
- Lowering the floor means building the gnu rows against an older glibc (an older-glibc build target in the `napi` job, via zig or napi's cross toolchain). That changes the `napi` matrix in `.github/workflows/`, so on this spike it is an operator change, not a lane change.

## Why This Matters

A floor that rises quietly turns the Rust engine into the JS engine for every user on an older distribution (Debian 12, Ubuntu 22.04, RHEL 9), and only `rust-strict` reports it. A glibc test cell on an image below the floor fails in a way that looks like a loader bug. A cell on an image far above the floor passes while hiding the fact that a common distro cannot load the addon.

## When to Apply

- Before choosing or changing a glibc image in `scripts/ci-rs-package.js` or in docs.
- After any change to `Cargo.lock` that adds or bumps crates in the napi build.
- When a user reports that `WAIT_ON_ENGINE=rust` behaves exactly like the JS engine on Linux.

## Examples

Verified in the same lane (run 36773818018 artifacts, arm64 Docker Desktop):

- `upload-artifact` with `path: prebuilds/**` keeps `<dir>/wait-on.node` under the artifact root, so the `package` job's merged download rebuilds `prebuilds/<dir>/` without a flatten step.
- Node running wait-on under `docker run --read-only --network none` needs no `--tmpfs`.
- A pnpm install puts the addon realpath under `<project>/node_modules/.pnpm/wait-on@file+.../node_modules/wait-on/prebuilds/<dir>/`, still inside the project, so a realpath-under-project check (`scripts/ci-rs-package.js` `assertInstalledAddon`) holds for pnpm as well as npm.
