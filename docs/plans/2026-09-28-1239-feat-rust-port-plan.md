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
  - Scheduled into a **future major — 11.x or later; the exact version stays loose** and depends on when the intervening trains and backlog land. It may land further out than 11.x; all of this work is built toward that eventual release. This is future work with no rush; the Node wait-on backlog ships first.
  - The Rust cutover ships first to an **alpha prerelease channel** for real-world parity testing, then promotes to `latest` at that major's GA.
  - Gated on the Node backlog (#19–#34) landing — the port targets the post-backlog code, not today's tree.
  - Depends on semantic-release (#33 / PR #34) being merged first: it is the release mechanism both channels rely on. #33 explicitly scopes out prerelease channels, so enabling an `alpha` channel is added work this plan depends on.
  - **Release identity is two-phase (verified gap).** #33/PR #34's release job is guarded `if github.repository == 'jeffbski/wait-on'` with an upstream-only OIDC trusted-publisher, so it does not publish from the fork as-is. The buildout runs as a PoC in `kevinold/wait-on` and publishes the `alpha` channel **from the fork** — which requires the fork's own npm trusted-publisher registration and a release-workflow guard extended to match `kevinold/wait-on`. GA promotion to `latest` lands **upstream** in `jeffbski/wait-on`, where #33's existing publisher applies. This reconciles with the "work stays on the fork" scope boundary: the fork is the PoC + alpha home; upstream is the GA home (see PO6a).

---

## Product Contract

### Summary

Reimplement wait-on's resource-checking engine in Rust and deliver it over npm as a napi-rs `.node` addon, bundled with prebuildify into a single package with npm OIDC provenance and zero install scripts. The Node public surface (CLI and programmatic API, including function-valued options and JS config files) stays byte-compatible. A standalone binary for non-Node users follows as a later phase from the same Rust core.

### Problem Frame

wait-on's runtime dependency tree — axios, joi, lodash, rxjs — is exactly the supply-chain surface that npm hardening now targets. Post-Shai-Hulud guidance (pnpm v10, Palo Alto Unit 42's 2026 threat analysis, widely cited npm checklists) treats lifecycle-script execution as the primary wormable attack surface and recommends `ignore-scripts=true` with a small audited allowlist. A single audited Rust core shrinks that surface while keeping npm as the channel.

Three further pressures rank behind supply chain: reaching users who do not have Node installed, faster startup and lower overhead in CI and containers, and stronger control over TLS, sockets, and timers than the current rxjs+axios pipeline. The reference project (tincan-cli) downloads and spawns an executable and writes to the system after install — the pattern the guidance above warns against, and explicitly out of scope here.

### Key Decisions

Each entry is a framing choice that constrains the Requirements below; the `Governs` links name the requirements that carry the full rule. `ce-plan` may inherit any of these into a numbered KTD during enrichment.

- **napi-rs core + prebuildify single package** — one Rust crate compiled to a `.node` addon, with every supported platform/arch binary bundled in one npm tarball; `node-gyp-build` selects at load. Chosen over optionalDependencies and over a standalone-only binary because it preserves the full Node API and yields one package with one OIDC provenance attestation. (session-settled: user-approved — chosen over optionalDependencies: a per-platform-package fan-out fights #33's single-package publish and breaks under `--no-optional`.) Governs R6, R7, R8. Caveat from prior-art research: prebuildify is **not** the napi-rs default — every surveyed napi repo (napi-rs, oxc, rolldown, swc) ships per-platform optionalDependencies via `napi prepublish`. The prebuildify path is therefore wired manually (napi `build` → `prebuilds/<platform-arch>/` → `node-gyp-build` or a small try/catch loader, as lightningcss does) rather than through napi's publish tooling — a deliberate against-the-grain choice for the single-package / offline / one-attestation constraints, and a tooling-maintenance cost to accept (see PO7, Sources).
- **Full programmatic Node API preserved, non-breaking** — including function-valued options (`validateStatus`) and `require()`'d JS config files. (session-settled: user-directed — chosen over "API minus niche bits" and "CLI-parity-only": wait-on is used as a library, not only a CLI.) Governs R2, R3.
- **Priority order: supply-chain surface > non-Node reach > startup > correctness** — this ranking drove the napi-first, binary-later split. (session-settled: user-directed — chosen over treating the four drivers as co-equal.) Governs R6, R10, and the sequencing in R14.
- **Rust swaps only the engine behind the unchanged JS surface** — option parsing, schema validation, JS config loading, and `validateStatus` evaluation stay JS-side; `validateStatus` is invoked across the FFI boundary via a napi threadsafe callback. Accepted as the price of zero-break parity. Governs R2, R3, R5.
- **Standalone binary deferred to a later phase** — the first release proves the Rust core in production behind the existing API (delivering priorities #1/#3/#4 to all current users); non-Node reach follows. (session-settled: user-approved — recommended and confirmed.) Governs R10, R14.
- **Attested binary distribution, no `curl | sh`** — when the binary ships: GitHub Releases with `SHA256SUMS` + SLSA build provenance (GitHub artifact attestations, same OIDC identity as the npm publish) + cosign signatures, installable via `cargo binstall`; brew tap and Scoop/winget are demand-driven. Governs R11, R12.
- **Reject download-at-install and postinstall fetch entirely** — including `prebuild-install`-style install-script fetch and the tincan download+spawn+write-after-install pattern. Governs R9.
- **Release through semantic-release (#33)** — single npm package published with OIDC provenance and no stored token; `@semantic-release/github` attaches the attested binaries to the GitHub Release in the later phase. Governs R8, R11, R14.
- **Monorepo, Node retained until cutover** — the Rust crate and the Node package coexist in one repo; the existing Node implementation and version stay in place through the transition. Governs R13.
- **Future-major target (version loose), alpha channel first** — the engine swap is non-breaking in itself, but it is scheduled into a future major (11.x or later; the exact version is deliberately left loose, as it may land further out than 11.x) and ships first to an `alpha` dist-tag for real-world parity testing before promotion to `latest`. The version number reflects the timeline, not a break in the API; all of this work is built toward that eventual release. (session-settled: user-directed — chosen over pinning a version or shipping straight to `latest`.) Governs R14, R16.
- **Tests as a conformance contract, not raw coverage** — parity is validated by a black-box CLI conformance suite plus property/differential parser tests run against both implementations; coverage percentage is a hardening signal, not the guarantee. Governs R17, R18.
- **Pre-port test-hardening phase** — before any Rust engine code, harden the Node suite (branch coverage toward ~100%, `bin/wait-on` instrumented, clock frozen) and stand up the conformance and property vectors; that suite is the executable contract Rust must pass on `alpha`. Governs R19, R21.
- **Internal, reversible cutover** — the engine binding flips from pure-JS to the napi addon with the public API unchanged, gated on the conformance suite green on `alpha` through a soak period; the pure-JS engine stays in-tree for one major as a rollback fallback. Governs R20, R22.

### Requirements

**Engine and API parity**

- R1. The Rust engine reproduces every resource type the Node engine supports: `file:` (with the size-stabilization window), `http:`/`https:` HEAD, `http-get:`/`https-get:` GET, `tcp:`, `socket:` (unix domain socket), and http-over-unix-socket (`http://unix:SOCK:PATH`), including reverse mode for each.
- R2. The programmatic Node API is byte-compatible: `waitOn(opts, cb?)` supports both the callback form and the promise form, and accepts every option in the current schema (`resources`, `delay`, `httpTimeout`, `interval`, `log`, `reverse`, `simultaneous`, `timeout`, `verbose`, `window`, `tcpTimeout`, `validateStatus`, `ca`, `cert`, `key`, `passphrase`, `proxy`, `auth`, `strictSSL`, `followRedirect`, `headers`).
- R3. Function-valued options (`validateStatus`) and `require()`'d JS config files continue to work through the JS shim; a JS function passed as an option is honored during resource checks.
- R4. The CLI preserves every flag and alias (`-c/-d/-i/-l/-r/-s/-t/-v/-w/-h`, `httpTimeout`, `tcpTimeout`), the interval-suffix parsing (`ms/s/m/h`), config-file precedence over positional resources, exit codes, and stderr/stdout behavior.
- R5. Behavior parity is the acceptance gate: the existing test suite passes against the Rust-backed build with identical observable behavior (stdout/stderr, exit codes, resolve/reject outcomes). Timing is judged against the R17/PO13 tolerance — FFI-callback marshalling overhead within that tolerance is expected behavior, not a defect — and that tolerance is the single objective promote-to-`latest` criterion. TLS behavior outside the documented parity for the R2 option matrix is a defect.

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
- R14. semantic-release (#33) versions and publishes both channels; the engine swap is published first to an `alpha` prerelease channel (dist-tag `alpha`) and promoted to `latest` only after parity is validated there. The swap itself introduces no API break; a major bump reflects the future-major timeline (R16), not the swap.
- R15. The port targets the post-backlog Node code (after #19–#34 land: `util.parseArgs` for minimist, drop-lodash, axios→fetch/undici, TypeScript definitions), not the current tree.
- R16. The Rust cutover ships in a future major (11.x or later — the exact version stays loose), after the current 9.x/10.x trains and other breaking work land, and reaches `latest` only after the alpha channel confirms parity (R5, R17).

**Parity contract and transition**

- R17. The parity contract is a black-box CLI conformance suite — spawn the CLI, assert stdout, stderr, exit code, and timing within tolerance — so the same vectors validate the napi build and, later, the standalone binary. Coverage percentage is a hardening signal, not the parity guarantee.
- R18. Property/differential tests cover the pure parsers (resource-prefix, `host:port`, `ms/s/m/h` interval, `http://unix:` split), asserting identical parse results across the Node and Rust implementations.
- R19. Before any Rust engine code lands, the Node suite is hardened: branch coverage raised from the 90.8% baseline toward ~100% (including the uncovered non-timeout error path at `lib/wait-on.js:141`), `bin/wait-on` instrumented for coverage, and time-dependent tests run against a frozen or injected clock rather than real timeouts.
- R20. Through the transition the repository is a monorepo with the Node implementation authoritative and published to `latest`; the Rust crate builds in CI and publishes only to the `alpha` channel until cutover.
- R21. Node features and fixes keep shipping to `latest` during the build-out; each change adds or updates conformance vectors (R17), which become Rust requirements that stay red until implemented — the suite is the sync mechanism, not a manual port checklist.
- R22. Cutover is internal — the engine binding flips from the pure-JS engine to the napi addon with the public API unchanged — gated on Rust passing the full conformance suite on `alpha` through a soak period; the pure-JS engine stays in-tree for one major as a rollback fallback (env-flag or load-failure fallback). Because that fallback keeps axios/joi/lodash/rxjs installed by default, the priority-#1 dependency-surface reduction lands in full only when the fallback is removed one major after cutover; a future option to pull the win forward is an opt-in fallback package (JS deps loaded only on addon-load failure) rather than default-installed. The plan does not claim the full #1 reduction at the cutover release itself.

**Rust-core supply chain**

- R23. The Rust core's dependency surface is hardened and enforced: a committed `Cargo.lock`, `cargo-deny`/`cargo-audit` (and ideally `cargo-vet`) gating CI, and a reviewed, minimized crate set (the HTTP stack, tokio, napi-rs, and their transitive crates). This is a first-class supply-chain requirement because npm OIDC provenance (R8) attests the build, not the safety of its crate inputs — a compromised or vulnerable crate would ship inside an authentically-attested `.node` binary, defeating the priority-#1 goal.

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
  - **Trigger:** a push to the release branch during the Rust build-out.
  - **Steps:** semantic-release computes the version from conventional commits and runs a gated dry-run; the Rust-backed build publishes to the `alpha` prerelease channel (`wait-on@alpha`) with OIDC provenance while `latest` stays on the pure-JS engine; once parity is confirmed on `alpha` through the soak period, the cutover promotes the future-major GA to `latest`; in the later phase `@semantic-release/github` attaches the attested binaries to the GitHub Release.
  - **Outcome:** the Rust build is validated on `alpha` before it reaches `latest` — cutover is the promotion, not a precondition — and both channels publish from one pipeline. Covers R8, R11, R14, R16, R22.

- F4. Parallel maintenance and cutover
  - **Trigger:** a Node feature or fix lands during the Rust build-out.
  - **Steps:** the change ships to `latest` on the current major and adds or updates conformance vectors; those vectors run red against the Rust build on `alpha` until implemented; when Rust is green on the full suite through a soak period, the engine binding flips to the napi addon and the future-major GA promotes to `latest`.
  - **Outcome:** Node users keep getting fixes while Rust catches up against an executable contract, and cutover is a gated, reversible internal swap. Covers R20, R21, R22.

### Acceptance Examples

- AE1. **Covers R6, R7, R9.** Given a Dockerfile that runs `npm ci` with `ignore-scripts=true` and then sets the root filesystem read-only, when the image runs `wait-on tcp:db:5432`, then it waits and exits normally with no network fetch and no script execution at install or run.
- AE2. **Covers R3.** Given `waitOn({ resources: ['https-get://host/health'], validateStatus: (s) => s === 200 })`, when the endpoint returns 200, then the check passes; when it returns 204, then the check does not pass — the JS function is consulted across the FFI boundary.
- AE3. **Covers R5.** Given the current mocha suite, when it runs against the Rust-backed build, then every test passes with no change to expected timing, exit codes, or TLS behavior.
- AE4. **Covers R10.** Given the standalone binary and a `--config config.js` argument, when invoked, then it exits with a clear error that JS config files are unsupported on the binary channel (JSON config accepted).
- AE5. **Covers R22.** Given the Rust build is not yet green on the full conformance suite on `alpha`, when a release runs, then `latest` keeps publishing the pure-JS engine and the napi binding is not promoted — the flip happens only after the suite is green through the soak period.

### Success Criteria

- **Supply-chain win is quantified (priority #1).** Measured as a delta against the **post-backlog** Node tree, not today's tree: transitive runtime-dependency count and lifecycle-script count, before vs after the Rust engine lands. A stated minimum reduction is the pass/fail bar; reductions attributable to the Node backlog alone (axios→fetch, drop-lodash, joi staying JS-side) do not count toward it. Note the fallback-window caveat in R22: the full reduction is realized when the pure-JS fallback is removed.
- **Startup is falsifiable (priority #3).** The napi addon release targets **no startup improvement** — it boots under Node and adds a native-addon load — so priority #3 is delivered by the deferred standalone binary, not this release. A startup/overhead benchmark, separate from the R5/R17 timing-parity tolerance, is part of the acceptance gate so the addon release cannot silently regress startup.

### Scope Boundaries

**Deferred for later**
- The standalone binary and its package-manager formulas (brew, Scoop, winget) — a later phase after the napi core proves out.
- optionalDependencies delivery — reconsidered only if the bundled package size becomes a real problem across many platform/arch targets.

**Outside this product's identity**
- Download-at-install, postinstall fetch, and `curl | sh` install paths — rejected on supply-chain grounds regardless of convenience.
- The tincan-cli pattern (download + spawn an executable, write to the system after install).
- Changing the Node public API — the port is non-breaking by contract.
- Any direct change to `jeffbski/wait-on`; work stays on the fork.

**"Non-breaking" is qualified on platform reach.** Today's pure-JS wait-on runs anywhere Node runs; the prebuildify bundle runs only on the cross-compiled matrix (PO4). During the fallback window the R22 JS fallback preserves off-matrix architectures, so "non-breaking" holds then. The long-term supported-platform floor is set in PO4, and any architecture dropped from the matrix at fallback removal is a **documented platform-support decision**, not a silent break — it must be announced with the removal.

### Dependencies / Assumptions

- Depends on semantic-release (#33 / PR #34) merged — the shared release mechanism.
- Depends on an `alpha` prerelease channel being added to that mechanism; #33 explicitly scopes prerelease channels out, so this is net-new release config (see PO6).
- Depends on the current 9.x/10.x trains and the Node backlog (#19–#34) landing first; the Rust cutover is future-major work (11.x or later) and the port target is the post-backlog code (R15, R16).
- Assumes napi-rs threadsafe callbacks can express `validateStatus` (and any other function option) without an unacceptable per-check performance regression — to be proven (see PO2).
- Assumes napi-rs cross-compilation can cover the platform/arch matrix wait-on supports today, and that the bundled size stays acceptable — to be proven (see PO4, PO5).
- Assumes the axios→fetch/undici change (#2) settles TLS/proxy/redirect behavior before the Rust HTTP engine is written, so parity is measured against the post-#2 behavior, not axios's.
- Assumes the current mocha API suite's behaviors can be expressed as black-box CLI vectors (or driven through a thin dual-driver harness) so both implementations run the same contract — to be proven (see PO13).
- Coverage baseline (measured 2026-09-28): `lib/wait-on.js` 99.3% line / 90.8% branch / 100% funcs; `bin/wait-on` unmeasured; 74 tests. R19 raises this before the port.

### Outstanding Questions and Proof Obligations

The user's explicit ask: capture every decision and consideration as something to be proven before committing. Each item below is a proof obligation — a claim the decisions above rest on, the smallest spike that would confirm or break it, and whether it blocks planning. This section is the exhaustive-proof gate; nothing proceeds to build until the "Resolve Before Planning" items are answered.

**Resolve before planning (blocks committing to the approach)**

- PO1. **napi non-breaking API bridge.** Prove a napi-rs addon can expose `waitOn(opts, cb?)` with both callback and promise forms and full option pass-through, behind an unchanged `lib/wait-on.js` public signature. Spike: port one resource type (tcp) end-to-end and run the existing tcp tests unmodified. Covers R2.
- PO2. **`validateStatus` across the FFI boundary.** Prove a JS function option can be invoked from the Rust HTTP check via a napi threadsafe callback, with correct semantics and no deadlock under the concurrency `simultaneous` allows. Spike: implement the HTTP check in Rust calling back into a JS `validateStatus`; measure per-check overhead vs the current path. Covers R3. This is the single riskiest parity item.
- PO3. **JS config file handling.** Confirm `require()`'d JS config files remain a JS-shim responsibility with no Rust involvement, and that nothing about the engine split forces a config-format change on the npm channel. Spike: run the existing config-file tests against the shim. Covers R3, R4.
- PO4. **Platform/arch matrix and cross-compilation.** Enumerate the exact targets to support (at minimum: darwin arm64/x64, linux x64/arm64 glibc, linux x64/arm64 musl, win32 x64; decide on win32 arm64 and linux armv7). Prove napi-rs cross-compiles all of them in CI. Spike: a CI matrix that builds every target's `.node`. Covers R6. Blocks planning because it sizes the build and the bundle.
- PO5. **Prebuildify bundle size vs the `--no-optional` guarantee.** Measure the single-package tarball size with all targets bundled and decide whether it is acceptable, given that the rejected alternative (optionalDependencies) exists precisely to shrink it. Spike: build the full bundle, record packed and unpacked sizes, compare against the current package size. Covers R6, R7.
- PO6. **semantic-release publishes a prebuildify package, with OIDC provenance, on an alpha prerelease channel.** Confirm the #33 flow can (a) publish a package containing prebuilt binaries with valid provenance (binaries are part of the published artifact, not fetched), and (b) run an `alpha` prerelease branch/dist-tag that #33 does not currently configure — publishing `wait-on@alpha` without touching `latest`, and promoting cleanly afterward. Spike: a dry-run prerelease publish of a stub package with a dummy `.node`, verifying provenance and the `alpha` dist-tag. Covers R8, R14, R16. Coordinate with #33 / PR #34, which scopes prereleases out today.
- PO6a. **Two-phase release identity (fork alpha vs upstream GA).** Prove the `alpha` channel can publish from `kevinold/wait-on` with valid OIDC provenance — its own trusted-publisher registration plus a release-workflow guard extended to match the fork — and that GA promotion to `latest` runs upstream in `jeffbski/wait-on` under #33's existing publisher, with `package.json` `repository.url` reconciled for each phase so provenance binds to the repo the code ships from. Spike: a fork dry-run alpha publish with provenance verification, and confirmation of the upstream GA path. Covers R8, R14, R16 and the Goal Capsule release-identity blocker; also resolves the provenance-identity mismatch (fork vs upstream). Coordinate with #33 / PR #34.
- PO7. **`node-gyp-build` loader and `ignore-scripts` behavior.** Prove the prebuildify loader resolves the correct binary at `require()` time with no install script, under `ignore-scripts=true`, `--no-optional`, npm and pnpm. Spike: install the stub package under each package manager with scripts disabled and load it. Covers R6, R7, R9.
- PO8. **Read-only / no-network runtime.** Prove the loaded addon runs with a read-only root filesystem and no network egress beyond what a resource check itself performs. Spike: run AE1 in a `--read-only` container. Covers R7, R9.
- PO9. **TLS/proxy/redirect parity baseline.** Decide the Rust HTTP stack (e.g. reqwest/rustls vs system TLS) and prove it reproduces the post-#2 (fetch/undici) behavior for `strictSSL`, `ca/cert/key/passphrase`, `proxy`, `followRedirect`, and `auth`. Spike: a parity harness comparing Node-fetch and Rust responses across those option combinations. Covers R2, R5, and carries an ordering dependency on #2.
- PO10. **File size-stabilization semantics.** Prove the Rust file check reproduces the stabilization-window logic (size stable across `window`, `window` floored to `interval`) with identical timing behavior. Spike: run the existing file tests against the Rust check. Covers R1, R5.
- PO11. **Non-breaking release classification.** Confirm that an engine swap with byte-for-byte parity is a minor/patch under semantic-release and does not, by itself, warrant a major; and define the trigger that would force a major. Covers R14.
- PO12. **Backlog dependency ordering.** Confirm which of #19–#34 must land before the port begins and which can land in parallel, and that the port branches from post-backlog `master`. Covers R15 and the Goal Capsule blockers.
- PO13. **Conformance-harness design.** Decide whether the mocha API suite translates to black-box CLI vectors or needs a thin dual-driver harness (JS API + napi addon fed the same vectors), and fix the timing tolerance and the clock-injection approach that replaces real timeouts. This is the parity contract's foundation. Covers R17, R18, R19.
- PO14. **Coverage hardening to the contract.** Prove `bin/wait-on` can be instrumented (spawn under nyc / `NODE_OPTIONS`), close the branch gap from 90.8% toward ~100% (starting with `lib/wait-on.js:141`), and confirm the hardened suite is green under a frozen clock. Covers R19.

**Deferred to planning (answered during planning or a later phase)**

- PO15. **Monorepo layout.** Decide the concrete layout (Cargo workspace + npm package location, where `bin/` and `lib/` live, how the crate and the addon reference each other). Prior art (see Sources): two dominant shapes — (A) sibling top-level dirs, a `crates/` Cargo workspace beside the JS package under `packages/`/`npm/`, with pnpm workspaces — biome, oxc, rolldown, swc, and openai/codex (`codex-rs/` + `codex-cli/` + `sdk/typescript/`); (B) Rust crate at repo root with the JS package + napi binding in a subdir — lightningcss (`node/`). Recommended starting point: model lightningcss/rolldown — keep the existing `wait-on` JS package in place (programmatic API + `bin/`) and add the Rust core as a sibling Cargo workspace (e.g. `crates/wait-on-core` + `crates/wait-on-napi`); rolldown and oxc prove a programmatic API (`main`→`dist/index`) and a CLI (`bin`) can co-live in one package. Covers R13, R20.
- PO16. **Incremental porting order.** Decide the sequence of resource types to move into Rust and whether the engine swap ships behind a flag or all at once. Covers R22.
- PO17. **Standalone binary internals (later phase).** Decide how the same core produces the binary (a `bin` target on the crate), how the CLI is shared between the addon and the binary, and how the documented exceptions are enforced. Covers R10.
- PO18. **Binary attestation pipeline (later phase).** Decide the exact SLSA/cosign tooling and how `@semantic-release/github` attaches and signs the release assets. Covers R11.
- PO19. **`cargo binstall` metadata (later phase).** Decide the `[package.metadata.binstall]` configuration and release-asset naming. Covers R11.
- PO20. **TypeScript types.** Decide whether napi-rs's generated `.d.ts` replaces or must match the hand-written definitions from #29, and who owns the type surface after the port. Covers R2, R15.
- PO21. **MSRV and toolchain pinning.** Decide the minimum Rust version and how the toolchain is pinned in CI.
- PO22. **Performance targets — resolved (see Success Criteria).** The napi addon release targets no startup improvement (priority #3 is delivered by the deferred standalone binary); a startup/overhead benchmark separate from the R5/R17 timing-parity tolerance is part of the acceptance gate to catch regressions. Remaining sub-decision for planning: the concrete benchmark harness and the regression threshold.

### Brainstorm Q&A — what we reviewed

Recorded at the user's request, as the questions and decisions this plan rests on.

- Q1. **Primary driver for the Rust port?** Answer: all four, in priority order — (1) shrink the supply-chain surface, (2) reach non-Node users, (3) faster startup / lower overhead, (4) correctness and a durable engine. Set the "priority order" decision.
- Q2. **How much of the programmatic API must the Rust release preserve?** Answer: full API, non-breaking, including function options (`validateStatus`) and JS config files. Set the "full programmatic Node API preserved" decision.
- Q3. **How should the standalone binary be distributed and trusted?** Answer: recommend one → deferred to a later phase, then attested GitHub Releases (`SHA256SUMS` + SLSA provenance + cosign) + `cargo binstall`, brew/scoop demand-driven, no `curl | sh`. Set the "standalone binary deferred" and "attested binary distribution" decisions.
- Q4. **prebuildify vs optionalDependencies for the npm/.node channel?** Resolved from Jeff's research plus the #33 constraint: prebuildify (single package, one OIDC provenance attestation, works under `ignore-scripts`/`--no-optional`/read-only containers). optionalDependencies rejected. Set the "napi-rs core + prebuildify" and "reject download-at-install" decisions.
- Q5. **When does this ship, and how is it tested?** Answer (user, mid-session): a future major after everything else ships — the version stays loose (11.x or later, may be further out) but all work builds toward it; ship on an `alpha` channel for testing before promotion. Set the "future-major target, alpha channel first" decision.
- Q6. **How do we guarantee the Node contract holds in Rust, what does the 10.x-era codebase look like, how do we keep shipping Node fixes during the build-out, and are unit tests the right contract?** Answer: tests are necessary but not sufficient as raw coverage — the contract is a black-box CLI conformance suite plus property/differential parser tests run against both implementations, hardened to ~100% branch before the port. The repo is a monorepo with Node authoritative on `latest` and Rust on `alpha`; the conformance suite is the sync mechanism so Node fixes keep shipping; cutover is an internal, gated, reversible engine-binding flip. Set the parity-contract and transition decisions (R17–R22, PO13–PO14).
- Q7. **Any public repos that did a Node/TS→Rust rewrite in the same repo, and how did they structure it? (user pointed to openai/codex)** Answer: surveyed openai/codex, biome, oxc, rolldown, swc, lightningcss, napi-rs/package-template. Two layout shapes dominate (sibling `crates/` + `packages/`, or root crate + JS subdir); the napi ecosystem default delivery is per-platform optionalDependencies (not prebuildify); codex specifically dropped the in-process TS API and uses a launcher+standalone-binary. Shaped PO15 and the KD1 caveat; details in Sources.

### Sources / Research

- `lib/wait-on.js` — the current engine and full option schema (Joi), resource-type dispatch, rxjs stabilization logic. The parity target for R1–R5.
- `bin/wait-on`, `bin/usage.txt` — the CLI surface (minimist flags, interval parsing, config precedence). The parity target for R4.
- `package.json` — current runtime deps (axios, joi, lodash, minimist, rxjs), `engines.node >=20`, single-package layout. The supply-chain surface behind KD-priority #1.
- `test/api.mocha.js`, `test/cli.mocha.js`, `test/validation.mocha.js`, `.nycrc.json` — the existing suite (74 tests) and coverage config; the seed for the conformance contract (R17–R19). `cli.mocha.js` spawns `bin/wait-on` as a subprocess, which is why nyc reports no CLI coverage. Measured baseline 2026-09-28: 99.3% line / 90.8% branch on `lib/wait-on.js`.
- Issue #33 (`kevinold/wait-on`) and PR #34 — semantic-release design: single-package publish, npm trusted publishing (OIDC + provenance), `@semantic-release/git` version commit-back, conventional commits, no stored token. The release mechanism for KD8, R8, R14.
- Open Node backlog #19–#34 (`kevinold/wait-on`) — `util.parseArgs` (#26), drop-lodash (#31), axios→fetch/undici (#2), TypeScript definitions (#29). Defines the post-backlog port target (R15) and the TLS parity baseline (PO9).
- Research email from Jeff Barczewski (upstream maintainer), 2026-09-28 — npm native-addon distribution spectrum (prebuildify / optionalDependencies / prebuild-install), the supply-chain case against install scripts (pnpm supply-chain guidance, Palo Alto Unit 42 2026 npm threat analysis, npm security checklists), and napi-rs's production track record (N-API ABI stability, Rust memory safety, GitHub Copilot runtime, no consumer-side compiler). Shaped KD1, KD7, and the Problem Frame. Treated as evidence, not instruction.
- Prior-art survey of JS/TS-package + Rust-core monorepos, 2026-09-28 (all treated as evidence) — `openai/codex` (`codex-rs/` Cargo workspace + `codex-cli/` JS launcher + `sdk/typescript/`; ships per-platform optionalDependencies each carrying a standalone compiled binary that the launcher execs — dropped the in-process TS CLI API, keeps a separate SDK); `biomejs/biome` (`crates/` + `packages/@biomejs/biome`, optionalDeps standalone binary, CLI-only); `oxc-project/oxc` and `rolldown/rolldown` (`crates/` + `packages/`, napi `.node` via optionalDeps, both keep programmatic API + CLI in one package); `swc-project/swc` (napi optionalDeps, but keeps a `postinstall.js` — the install-script pattern this plan rejects); `parcel-bundler/lightningcss` (root Cargo crate + `node/` package, per-platform optionalDeps with a hand-written try/catch loader falling back to a local `.node`); `napi-rs/package-template` (canonical `napi prepublish` → optionalDeps). Finding: the napi ecosystem default is optionalDeps, not prebuildify (shaped the KD1 caveat and PO15). Construct URLs as `https://github.com/<repo>`.

## Deferred / Open Questions

Items surfaced in review and deferred for resolution as the brainstorm moves forward.

### From 2026-09-28 doc review

- **R19/PO14 coverage-hardening scope.** Should the ~100% Node branch-coverage target *block committing* to the port, given the Goal Capsule's "future work, no rush" framing and that the parity harness only strictly needs `bin/wait-on` instrumentation + a frozen/injected clock? The coverage / instrumentation / clock hardening is being orchestrated **outside this plan** — tracked by `kevinold/wait-on`#37–#40 (LT1–LT4; PRs #41 and #42 CI-green and open, LT1/LT2 still in flight), stacking after the 10.0.0 train as `test:` commits (no version cut). Decide whether R19/PO14 should reference those PRs and move the ~100%-branch target out of the blocking tier into deferred-to-planning, keeping only the harness-critical parts as a blocker.
