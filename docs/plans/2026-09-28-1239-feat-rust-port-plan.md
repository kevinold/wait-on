---
title: Rust Port of wait-on - Plan
type: feat
date: 2026-09-28
topic: rust-port
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-brainstorm
execution: code
---

# Rust Port of wait-on - Plan

## Goal Capsule

- **Objective:** wait-on's resource-checking engine runs in Rust and ships to users through npm as a drop-in, non-breaking upgrade — the full CLI and the full programmatic Node API keep working unchanged — and, in a later phase, as a standalone binary for people without Node. The delivery requires no install/postinstall scripts and no download at install or first run.
- **Means:** one Rust core crate, consumed two ways from the same source — a napi-rs `.node` addon that is the existing npm package (prebuildify-bundled), and (later) a standalone binary.
- **Product authority:** kevinold (fork owner, `kevinold/wait-on`). Upstream maintainer (Jeff Barczewski) was consulted and endorses the napi-rs + prebuildify direction, which matters for eventual upstreaming to `jeffbski/wait-on`.
- **Open blockers:**
  - Scheduled into the **11.x line — after everything else ships** (the current 9.x/10.x trains and the in-flight backlog). This is future work with no rush; the Node wait-on backlog ships first.
  - The Rust cutover ships first to an **alpha prerelease channel** for real-world parity testing, then promotes to `latest` at the 11.x GA.
  - Gated on the Node backlog (#19–#34) landing — the port targets the post-backlog code, not today's tree.
  - Depends on semantic-release (#33 / PR #34) being merged first: it is the release mechanism both channels rely on. #33 explicitly scopes out prerelease channels, so enabling an `alpha` channel is added work this plan depends on.

---

## Product Contract

### Summary

Reimplement wait-on's resource-checking engine in Rust and deliver it over npm as a napi-rs `.node` addon, bundled with prebuildify into a single package with npm OIDC provenance and zero install scripts. The Node public surface (CLI and programmatic API, including function-valued options and JS config files) stays byte-compatible. A standalone binary for non-Node users follows as a later phase from the same Rust core.

### Problem Frame

wait-on's runtime dependency tree — axios, joi, lodash, rxjs — is exactly the supply-chain surface that npm hardening now targets. Post-Shai-Hulud guidance (pnpm v10, Palo Alto Unit 42's 2026 threat analysis, widely cited npm checklists) treats lifecycle-script execution as the primary wormable attack surface and recommends `ignore-scripts=true` with a small audited allowlist. A single audited Rust core shrinks that surface while keeping npm as the channel.

Three further pressures rank behind supply chain: reaching users who do not have Node installed, faster startup and lower overhead in CI and containers, and stronger control over TLS, sockets, and timers than the current rxjs+axios pipeline. The reference project (tincan-cli) downloads and spawns an executable and writes to the system after install — the pattern the guidance above warns against, and explicitly out of scope here.

### Key Decisions

Each entry is a framing choice that constrains the Requirements below; the `Governs` links name the requirements that carry the full rule. `ce-plan` may inherit any of these into a numbered KTD during enrichment.

- **napi-rs core + prebuildify single package** — one Rust crate compiled to a `.node` addon, with every supported platform/arch binary bundled in one npm tarball; `node-gyp-build` selects at load. Chosen over optionalDependencies and over a standalone-only binary because it preserves the full Node API and yields one package with one OIDC provenance attestation. (session-settled: user-approved — chosen over optionalDependencies: a per-platform-package fan-out fights #33's single-package publish and breaks under `--no-optional`.) Governs R6, R7, R8.
- **Full programmatic Node API preserved, non-breaking** — including function-valued options (`validateStatus`) and `require()`'d JS config files. (session-settled: user-directed — chosen over "API minus niche bits" and "CLI-parity-only": wait-on is used as a library, not only a CLI.) Governs R2, R3.
- **Priority order: supply-chain surface > non-Node reach > startup > correctness** — this ranking drove the napi-first, binary-later split. (session-settled: user-directed — chosen over treating the four drivers as co-equal.) Governs R6, R10, and the sequencing in R14.
- **Rust swaps only the engine behind the unchanged JS surface** — option parsing, schema validation, JS config loading, and `validateStatus` evaluation stay JS-side; `validateStatus` is invoked across the FFI boundary via a napi threadsafe callback. Accepted as the price of zero-break parity. Governs R2, R3, R5.
- **Standalone binary deferred to a later phase** — the first release proves the Rust core in production behind the existing API (delivering priorities #1/#3/#4 to all current users); non-Node reach follows. (session-settled: user-approved — recommended and confirmed.) Governs R10, R14.
- **Attested binary distribution, no `curl | sh`** — when the binary ships: GitHub Releases with `SHA256SUMS` + SLSA build provenance (GitHub artifact attestations, same OIDC identity as the npm publish) + cosign signatures, installable via `cargo binstall`; brew tap and Scoop/winget are demand-driven. Governs R11, R12.
- **Reject download-at-install and postinstall fetch entirely** — including `prebuild-install`-style install-script fetch and the tincan download+spawn+write-after-install pattern. Governs R9.
- **Release through semantic-release (#33)** — single npm package published with OIDC provenance and no stored token; `@semantic-release/github` attaches the attested binaries to the GitHub Release in the later phase. Governs R8, R11, R14.
- **Monorepo, Node retained until cutover** — the Rust crate and the Node package coexist in one repo; the existing Node implementation and version stay in place through the transition. Governs R13.
- **11.x target, alpha channel first** — the engine swap is non-breaking in itself, but it is scheduled into the 11.x line (after the intervening trains and other breaking work) and ships first to an `alpha` dist-tag for real-world parity testing before promotion to `latest`. The version number reflects the timeline, not a break in the API. (session-settled: user-directed — chosen over shipping straight to `latest` in the next available release.) Governs R14, R16.

### Requirements

**Engine and API parity**

- R1. The Rust engine reproduces every resource type the Node engine supports: `file:` (with the size-stabilization window), `http:`/`https:` HEAD, `http-get:`/`https-get:` GET, `tcp:`, `socket:` (unix domain socket), and http-over-unix-socket (`http://unix:SOCK:PATH`), including reverse mode for each.
- R2. The programmatic Node API is byte-compatible: `waitOn(opts, cb?)` supports both the callback form and the promise form, and accepts every option in the current schema (`resources`, `delay`, `httpTimeout`, `interval`, `log`, `reverse`, `simultaneous`, `timeout`, `verbose`, `window`, `tcpTimeout`, `validateStatus`, `ca`, `cert`, `key`, `passphrase`, `proxy`, `auth`, `strictSSL`, `followRedirect`, `headers`).
- R3. Function-valued options (`validateStatus`) and `require()`'d JS config files continue to work through the JS shim; a JS function passed as an option is honored during resource checks.
- R4. The CLI preserves every flag and alias (`-c/-d/-i/-l/-r/-s/-t/-v/-w/-h`, `httpTimeout`, `tcpTimeout`), the interval-suffix parsing (`ms/s/m/h`), config-file precedence over positional resources, exit codes, and stderr/stdout behavior.
- R5. Behavior parity is the acceptance gate: the existing test suite passes byte-for-byte against the Rust-backed build, and any timing or TLS-behavior drift is treated as a defect, not an acceptable minor change.

**Packaging and supply chain**

- R6. The npm package ships as a single prebuildify-bundled package containing all supported platform/arch `.node` binaries; `npm install` performs no network fetch and runs no `install`/`postinstall` script.
- R7. Install and run succeed under `ignore-scripts=true`, under `--no-optional`, and inside read-only / no-network runtime containers (writable only during the image build step).
- R8. The published npm package carries npm OIDC provenance with no stored token, via the #33 release flow.
- R9. No download-at-install, no postinstall fetch, no spawn of a downloaded executable, and nothing written to the system after install.

**Standalone binary (later phase)**

- R10. A standalone binary provides full CLI parity, with two documented and enforced exceptions for that channel only: no JS-function options and no `require()`'d JS config files (JSON/other static config only).
- R11. Binary distribution is attested: GitHub Releases carry `SHA256SUMS`, SLSA build provenance, and cosign signatures; the binary is installable via `cargo binstall`; Homebrew tap and Scoop/winget are added on demand.
- R12. The binary channel offers no `curl | sh` default install path.

**Repo layout and release**

- R13. The Rust crate and the Node package coexist in the same repository; the existing Node code and its published version are retained until the cutover.
- R14. semantic-release (#33) versions and publishes both channels; the engine swap is published first to an `alpha` prerelease channel (dist-tag `alpha`) and promoted to `latest` only after parity is validated there. The swap itself introduces no API break; a major bump reflects the 11.x timeline (R16), not the swap.
- R15. The port targets the post-backlog Node code (after #19–#34 land: `util.parseArgs` for minimist, drop-lodash, axios→fetch/undici, TypeScript definitions), not the current tree.
- R16. The Rust cutover ships in the 11.x line, after the current 9.x/10.x trains and other breaking work land, and reaches `latest` only after the alpha channel confirms byte-for-byte parity (R5).

### Key Flows

- F1. Install in a locked-down container
  - **Trigger:** `npm ci` during a Docker build stage with `ignore-scripts=true`, root FS made read-only afterward.
  - **Steps:** npm resolves the single package from the registry; the tarball already contains every platform binary; no script runs and no network call is made beyond the registry download; at load, `node-gyp-build` selects the matching `.node`.
  - **Outcome:** wait-on runs in a fully read-only, no-network runtime. Covers R6, R7, R9.

- F2. Programmatic call with a function option
  - **Trigger:** a consumer calls `require('wait-on')({ resources, validateStatus })`.
  - **Steps:** the JS shim validates options and loads any JS config; it invokes the Rust engine over napi; for an HTTP check the engine consults `validateStatus` through a napi threadsafe callback into JS, then resolves.
  - **Outcome:** identical result and option semantics to the current Node implementation. Covers R2, R3, R5.

- F3. Release
  - **Trigger:** a push to the release branch after cutover.
  - **Steps:** semantic-release computes the version from conventional commits, runs a gated dry-run, then on approval publishes the Rust-backed build to the `alpha` prerelease channel (`wait-on@alpha`) with OIDC provenance; after parity is confirmed on `alpha`, the 11.x GA promotes to `latest`; in the later phase `@semantic-release/github` attaches the attested binaries to the GitHub Release.
  - **Outcome:** the Rust build is validated on `alpha` before it reaches `latest`, both channels published from one pipeline. Covers R8, R11, R14, R16.

### Acceptance Examples

- AE1. **Covers R6, R7, R9.** Given a Dockerfile that runs `npm ci` with `ignore-scripts=true` and then sets the root filesystem read-only, when the image runs `wait-on tcp:db:5432`, then it waits and exits normally with no network fetch and no script execution at install or run.
- AE2. **Covers R3.** Given `waitOn({ resources: ['https-get://host/health'], validateStatus: (s) => s === 200 })`, when the endpoint returns 200, then the check passes; when it returns 204, then the check does not pass — the JS function is consulted across the FFI boundary.
- AE3. **Covers R5.** Given the current mocha suite, when it runs against the Rust-backed build, then every test passes with no change to expected timing, exit codes, or TLS behavior.
- AE4. **Covers R10.** Given the standalone binary and a `--config config.js` argument, when invoked, then it exits with a clear error that JS config files are unsupported on the binary channel (JSON config accepted).

### Scope Boundaries

**Deferred for later**
- The standalone binary and its package-manager formulas (brew, Scoop, winget) — a later phase after the napi core proves out.
- optionalDependencies delivery — reconsidered only if the bundled package size becomes a real problem across many platform/arch targets.

**Outside this product's identity**
- Download-at-install, postinstall fetch, and `curl | sh` install paths — rejected on supply-chain grounds regardless of convenience.
- The tincan-cli pattern (download + spawn an executable, write to the system after install).
- Changing the Node public API — the port is non-breaking by contract.
- Any direct change to `jeffbski/wait-on`; work stays on the fork.

### Dependencies / Assumptions

- Depends on semantic-release (#33 / PR #34) merged — the shared release mechanism.
- Depends on an `alpha` prerelease channel being added to that mechanism; #33 explicitly scopes prerelease channels out, so this is net-new release config (see PO6).
- Depends on the current 9.x/10.x trains and the Node backlog (#19–#34) landing first; the Rust cutover is 11.x work and the port target is the post-backlog code (R15, R16).
- Assumes napi-rs threadsafe callbacks can express `validateStatus` (and any other function option) without an unacceptable per-check performance regression — to be proven (see PO2).
- Assumes napi-rs cross-compilation can cover the platform/arch matrix wait-on supports today, and that the bundled size stays acceptable — to be proven (see PO4, PO5).
- Assumes the axios→fetch/undici change (#2) settles TLS/proxy/redirect behavior before the Rust HTTP engine is written, so parity is measured against the post-#2 behavior, not axios's.

### Outstanding Questions and Proof Obligations

The user's explicit ask: capture every decision and consideration as something to be proven before committing. Each item below is a proof obligation — a claim the decisions above rest on, the smallest spike that would confirm or break it, and whether it blocks planning. This section is the exhaustive-proof gate; nothing proceeds to build until the "Resolve Before Planning" items are answered.

**Resolve before planning (blocks committing to the approach)**

- PO1. **napi non-breaking API bridge.** Prove a napi-rs addon can expose `waitOn(opts, cb?)` with both callback and promise forms and full option pass-through, behind an unchanged `lib/wait-on.js` public signature. Spike: port one resource type (tcp) end-to-end and run the existing tcp tests unmodified. Covers R2.
- PO2. **`validateStatus` across the FFI boundary.** Prove a JS function option can be invoked from the Rust HTTP check via a napi threadsafe callback, with correct semantics and no deadlock under the concurrency `simultaneous` allows. Spike: implement the HTTP check in Rust calling back into a JS `validateStatus`; measure per-check overhead vs the current path. Covers R3. This is the single riskiest parity item.
- PO3. **JS config file handling.** Confirm `require()`'d JS config files remain a JS-shim responsibility with no Rust involvement, and that nothing about the engine split forces a config-format change on the npm channel. Spike: run the existing config-file tests against the shim. Covers R3, R4.
- PO4. **Platform/arch matrix and cross-compilation.** Enumerate the exact targets to support (at minimum: darwin arm64/x64, linux x64/arm64 glibc, linux x64/arm64 musl, win32 x64; decide on win32 arm64 and linux armv7). Prove napi-rs cross-compiles all of them in CI. Spike: a CI matrix that builds every target's `.node`. Covers R6. Blocks planning because it sizes the build and the bundle.
- PO5. **Prebuildify bundle size vs the `--no-optional` guarantee.** Measure the single-package tarball size with all targets bundled and decide whether it is acceptable, given that the rejected alternative (optionalDependencies) exists precisely to shrink it. Spike: build the full bundle, record packed and unpacked sizes, compare against the current package size. Covers R6, R7.
- PO6. **semantic-release publishes a prebuildify package, with OIDC provenance, on an alpha prerelease channel.** Confirm the #33 flow can (a) publish a package containing prebuilt binaries with valid provenance (binaries are part of the published artifact, not fetched), and (b) run an `alpha` prerelease branch/dist-tag that #33 does not currently configure — publishing `wait-on@alpha` without touching `latest`, and promoting cleanly afterward. Spike: a dry-run prerelease publish of a stub package with a dummy `.node`, verifying provenance and the `alpha` dist-tag. Covers R8, R14, R16. Coordinate with #33 / PR #34, which scopes prereleases out today.
- PO7. **`node-gyp-build` loader and `ignore-scripts` behavior.** Prove the prebuildify loader resolves the correct binary at `require()` time with no install script, under `ignore-scripts=true`, `--no-optional`, npm and pnpm. Spike: install the stub package under each package manager with scripts disabled and load it. Covers R6, R7, R9.
- PO8. **Read-only / no-network runtime.** Prove the loaded addon runs with a read-only root filesystem and no network egress beyond what a resource check itself performs. Spike: run AE1 in a `--read-only` container. Covers R7, R9.
- PO9. **TLS/proxy/redirect parity baseline.** Decide the Rust HTTP stack (e.g. reqwest/rustls vs system TLS) and prove it reproduces the post-#2 (fetch/undici) behavior for `strictSSL`, `ca/cert/key/passphrase`, `proxy`, `followRedirect`, and `auth`. Spike: a parity harness comparing Node-fetch and Rust responses across those option combinations. Covers R2, R5, and carries an ordering dependency on #2.
- PO10. **File size-stabilization semantics.** Prove the Rust file check reproduces the stabilization-window logic (size stable across `window`, `window` floored to `interval`) with identical timing behavior. Spike: run the existing file tests against the Rust check. Covers R1, R5.
- PO11. **Non-breaking release classification.** Confirm that an engine swap with byte-for-byte parity is a minor/patch under semantic-release and does not, by itself, warrant a major; and define the trigger that would force a major. Covers R14.
- PO12. **Backlog dependency ordering.** Confirm which of #19–#34 must land before the port begins and which can land in parallel, and that the port branches from post-backlog `master`. Covers R15 and the Goal Capsule blockers.

**Deferred to planning (answered during planning or a later phase)**

- PO13. **Monorepo layout.** Decide the concrete layout (Cargo workspace + npm package location, where `bin/` and `lib/` live, how the crate and the addon reference each other). Covers R13.
- PO14. **Incremental porting order.** Decide the sequence of resource types to move into Rust and whether the engine swap ships behind a flag or all at once.
- PO15. **Standalone binary internals (later phase).** Decide how the same core produces the binary (a `bin` target on the crate), how the CLI is shared between the addon and the binary, and how the documented exceptions are enforced. Covers R10.
- PO16. **Binary attestation pipeline (later phase).** Decide the exact SLSA/cosign tooling and how `@semantic-release/github` attaches and signs the release assets. Covers R11.
- PO17. **`cargo binstall` metadata (later phase).** Decide the `[package.metadata.binstall]` configuration and release-asset naming. Covers R11.
- PO18. **TypeScript types.** Decide whether napi-rs's generated `.d.ts` replaces or must match the hand-written definitions from #29, and who owns the type surface after the port. Covers R2, R15.
- PO19. **MSRV and toolchain pinning.** Decide the minimum Rust version and how the toolchain is pinned in CI.
- PO20. **Performance targets.** Decide whether startup/overhead (priority #3) gets an explicit measured target, or is left as "no regression."

### Brainstorm Q&A — what we reviewed

Recorded at the user's request, as the questions and decisions this plan rests on.

- Q1. **Primary driver for the Rust port?** Answer: all four, in priority order — (1) shrink the supply-chain surface, (2) reach non-Node users, (3) faster startup / lower overhead, (4) correctness and a durable engine. Set the "priority order" decision.
- Q2. **How much of the programmatic API must the Rust release preserve?** Answer: full API, non-breaking, including function options (`validateStatus`) and JS config files. Set the "full programmatic Node API preserved" decision.
- Q3. **How should the standalone binary be distributed and trusted?** Answer: recommend one → deferred to a later phase, then attested GitHub Releases (`SHA256SUMS` + SLSA provenance + cosign) + `cargo binstall`, brew/scoop demand-driven, no `curl | sh`. Set the "standalone binary deferred" and "attested binary distribution" decisions.
- Q4. **prebuildify vs optionalDependencies for the npm/.node channel?** Resolved from Jeff's research plus the #33 constraint: prebuildify (single package, one OIDC provenance attestation, works under `ignore-scripts`/`--no-optional`/read-only containers). optionalDependencies rejected. Set the "napi-rs core + prebuildify" and "reject download-at-install" decisions.
- Q5. **When does this ship, and how is it tested?** Answer (user, mid-session): 11.x work, after everything else ships; ship on an `alpha` channel for testing before promotion. Set the "11.x target, alpha channel first" decision.

### Sources / Research

- `lib/wait-on.js` — the current engine and full option schema (Joi), resource-type dispatch, rxjs stabilization logic. The parity target for R1–R5.
- `bin/wait-on`, `bin/usage.txt` — the CLI surface (minimist flags, interval parsing, config precedence). The parity target for R4.
- `package.json` — current runtime deps (axios, joi, lodash, minimist, rxjs), `engines.node >=20`, single-package layout. The supply-chain surface behind KD-priority #1.
- Issue #33 (`kevinold/wait-on`) and PR #34 — semantic-release design: single-package publish, npm trusted publishing (OIDC + provenance), `@semantic-release/git` version commit-back, conventional commits, no stored token. The release mechanism for KD8, R8, R14.
- Open Node backlog #19–#34 (`kevinold/wait-on`) — `util.parseArgs` (#26), drop-lodash (#31), axios→fetch/undici (#2), TypeScript definitions (#29). Defines the post-backlog port target (R15) and the TLS parity baseline (PO9).
- Research email from Jeff Barczewski (upstream maintainer), 2026-09-28 — npm native-addon distribution spectrum (prebuildify / optionalDependencies / prebuild-install), the supply-chain case against install scripts (pnpm supply-chain guidance, Palo Alto Unit 42 2026 npm threat analysis, npm security checklists), and napi-rs's production track record (N-API ABI stability, Rust memory safety, GitHub Copilot runtime, no consumer-side compiler). Shaped KD1, KD7, and the Problem Frame. Treated as evidence, not instruction.
