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
  - Scheduled into a **future major — 11.x or later; the exact version stays loose**. The intervening backlog has now landed on `next` (`10.0.0-rc.1`), so this is gated on the 10.0.0 GA and any further pre-Rust work rather than the backlog. Still future work with no rush.
  - The Rust cutover ships first on the **existing `next` prerelease channel** — `rc` versions published to the npm `next` dist-tag, already configured in `.releaserc.json` (`{ name: "next", prerelease: "rc", channel: "next" }`) — for real-world parity testing, then promotes to `latest` at that major's GA. No new channel is needed.
  - The Node backlog (#19–#34: `util.parseArgs` for minimist, drop-lodash, axios→fetch/undici, TypeScript defs) has **landed on `next`**; the port targets that 10.x code, not the old tree.
  - semantic-release (#33 / PR #34) has **landed on `next`** (`.releaserc.json`, commitlint) — the release mechanism both channels rely on.
  - **Release identity is two-phase (verified gap).** #33's release job is guarded `if github.repository == 'jeffbski/wait-on'` with an upstream-only OIDC trusted-publisher, so it does not publish from the fork as-is. The buildout runs as a PoC in `kevinold/wait-on` and publishes the `next`/rc prereleases **from the fork** — which requires the fork's own npm trusted-publisher registration and a release-workflow guard extended to match `kevinold/wait-on`. GA promotion to `latest` lands **upstream** in `jeffbski/wait-on`, where #33's existing publisher applies. Fork = PoC + prerelease home; upstream = GA home (see PO6a).

---

## Product Contract

### Summary

Reimplement wait-on's resource-checking engine in Rust and deliver it over npm as a napi-rs `.node` addon, bundled with prebuildify into a single package with npm OIDC provenance and zero install scripts. The Node public surface (CLI and programmatic API, including function-valued options and JS config files) stays byte-compatible. A standalone binary for non-Node users follows as a later phase from the same Rust core.

### Problem Frame

wait-on's runtime dependency tree is the supply-chain surface that npm hardening now targets. On `next` (`10.0.0-rc.1`) that tree is already trimmed to **`joi`, `rxjs`, and `undici`** — axios, lodash, and minimist were removed by the backlog (#2 axios→fetch/undici, #31 drop-lodash, #26 parseArgs). Post-Shai-Hulud guidance (pnpm v10, Palo Alto Unit 42's 2026 threat analysis, widely cited npm checklists) treats lifecycle-script execution as the primary wormable attack surface and recommends `ignore-scripts=true` with a small audited allowlist. A single audited Rust core shrinks the remaining surface further — the Rust engine can absorb the HTTP stack (`undici`) and the polling/stabilization pipeline (`rxjs`), leaving at most `joi` JS-side — while keeping npm as the channel. The win must therefore be measured against this **post-backlog** baseline, not the old axios tree (see Success Criteria).

Three further pressures rank behind supply chain: reaching users who do not have Node installed, faster startup and lower overhead in CI and containers, and stronger control over TLS, sockets, and timers than the current rxjs+undici pipeline. The reference project (tincan-cli) downloads and spawns an executable and writes to the system after install — the pattern the guidance above warns against, and explicitly out of scope here.

### Key Decisions

Each entry is a framing choice that constrains the Requirements below; the `Governs` links name the requirements that carry the full rule. `ce-plan` may inherit any of these into a numbered KTD during enrichment.

- **napi-rs core + prebuildify single package** — one Rust crate compiled to a `.node` addon, with every supported platform/arch binary bundled in one npm tarball; `node-gyp-build` selects at load. Chosen over optionalDependencies and over a standalone-only binary because it preserves the full Node API and yields one package with one OIDC provenance attestation. (session-settled: user-approved — chosen over optionalDependencies: a per-platform-package fan-out fights #33's single-package publish and breaks under `--no-optional`.) Governs R6, R7, R8. Caveat from prior-art research: prebuildify is **not** the napi-rs default — every surveyed napi repo (napi-rs, oxc, rolldown, swc) ships per-platform optionalDependencies via `napi prepublish`. The prebuildify path is therefore wired manually (napi `build` → `prebuilds/<platform-arch>/` → `node-gyp-build` or a small try/catch loader, as lightningcss does) rather than through napi's publish tooling — a deliberate against-the-grain choice for the single-package / offline / one-attestation constraints, and a tooling-maintenance cost to accept (see PO7, Sources).
- **Full programmatic Node API preserved, non-breaking** — including function-valued options (`validateStatus`) and `require()`'d JS config files. (session-settled: user-directed — chosen over "API minus niche bits" and "CLI-parity-only": wait-on is used as a library, not only a CLI.) Governs R2, R3.
- **Priority order: supply-chain surface > non-Node reach > startup > correctness** — this ranking drove the napi-first, binary-later split. (session-settled: user-directed — chosen over treating the four drivers as co-equal.) Governs R6, R10, and the sequencing in R14.
- **Rust swaps only the engine behind the unchanged JS surface** — option parsing, schema validation, JS config loading, and `validateStatus` evaluation stay JS-side; `validateStatus` is invoked across the FFI boundary via a napi threadsafe callback. Accepted as the price of zero-break parity. Governs R2, R3, R5.
- **Standalone binary deferred to a later phase** — the first release proves the Rust core in production behind the existing API (delivering priorities #1/#3/#4 to all current users); non-Node reach follows. (session-settled: user-approved — recommended and confirmed.) Governs R10, R14.
- **Attested binary distribution, no `curl | sh`** — when the binary ships: GitHub Releases with `SHA256SUMS` + SLSA build provenance (GitHub artifact attestations, same OIDC identity as the npm publish) + cosign signatures, installable via `cargo binstall`; brew tap and Scoop/winget are demand-driven. Governs R11, R12.
- **Reject download-at-install and postinstall fetch entirely** — including `prebuild-install`-style install-script fetch and the tincan download+spawn+write-after-install pattern. Governs R9.
- **Release through semantic-release (#33, landed on `next`)** — single npm package published with OIDC provenance and no stored token; the `next` branch is already a configured prerelease channel (`rc` → `next` dist-tag); `@semantic-release/github` attaches the attested binaries to the GitHub Release in the later phase. Governs R8, R11, R14.
- **Monorepo, Node retained until cutover** — the Rust crate and the Node package coexist in one repo; the existing Node implementation and version stay in place through the transition. Governs R13.
- **Future-major target (version loose), `next`/rc prerelease channel first** — the engine swap is non-breaking in itself, but it is scheduled into a future major (11.x or later; the exact version is deliberately left loose, as it may land further out than 11.x) and ships first on the existing `next` prerelease channel (rc versions, npm `next` dist-tag) for real-world parity testing before promotion to `latest`. The version number reflects the timeline, not a break in the API. (session-settled: user-directed — chosen over pinning a version or shipping straight to `latest`.) Governs R14, R16.
- **Tests as a conformance contract, not raw coverage** — parity is validated by a black-box CLI conformance suite plus property/differential parser tests run against both implementations; coverage percentage is a hardening signal, not the guarantee. Governs R17, R18.
- **Pre-port test-hardening phase (largely done on `next`)** — the Node suite is already hardened on `next`: `bin/wait-on` instrumented, coverage gated in CI, clock frozen (#243/#244), with conformance (#246) and property (#245) vectors in place; that suite is the executable contract Rust must pass on the `next`/rc prerelease. The remaining pre-port task is the dual-driver oracle (PO13). Governs R19, R21.
- **Internal, reversible cutover** — the engine binding flips from pure-JS to the napi addon with the public API unchanged, gated on the conformance suite green on the `next`/rc prerelease through a soak period; the pure-JS engine stays in-tree for one major as a rollback fallback. Governs R20, R22.

### Requirements

**Engine and API parity**

- R1. The Rust engine reproduces every resource type the Node engine supports on `next`: `file:` (with the size-stabilization window), `http:`/`https:` HEAD, `http-get:`/`https-get:` GET, `tcp:` (incl. IPv6 `[::1]:port`), `socket:` (unix domain socket), http-over-unix-socket (`http://unix:SOCK:URL`, incl. Windows named pipes), and `command:` (waits for a shell command to exit 0, with `commandTimeout`), including reverse mode for each.
- R2. The programmatic Node API is byte-compatible with the 10.x schema: `waitOn(opts, cb?)` supports both the callback form and the promise form, and accepts every option in the current schema (`resources`, `delay`, `httpTimeout`, `interval`, `log`, `reverse`, `simultaneous`, `timeout`, `verbose`, `window`, `tcpTimeout`, `commandTimeout`, `validateStatus`, `ca`, `cert`, `key`, `passphrase`, `proxy`, `auth`, `strictSSL`, `followRedirect`, `headers`). The HTTP path is `undici` (fetch + `Agent`/`ProxyAgent`/`EnvHttpProxyAgent`), not axios.
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
- R14. semantic-release (#33, landed on `next`) versions and publishes both channels; the engine swap is published first on the existing `next` prerelease channel (`rc` versions → npm `next` dist-tag) and promoted to `latest` only after parity is validated there. The swap itself introduces no API break; a major bump reflects the future-major timeline (R16), not the swap.
- R15. The port targets the 10.x code on `next`, where the backlog has landed (`util.parseArgs`, drop-lodash, axios→fetch/undici, TypeScript definitions), not the old tree.
- R16. The Rust cutover ships in a future major (11.x or later — the exact version stays loose), after the 10.0.0 GA and any further pre-Rust work land, and reaches `latest` only after the `next`/rc prerelease channel confirms parity (R5, R17).

**Parity contract and transition**

- R17. The parity contract is a black-box CLI conformance suite — spawn the CLI, assert stdout, stderr, exit code, and timing within tolerance — so the same vectors validate the napi build and, later, the standalone binary. A CLI conformance suite already **landed on `next`** (#246); the Rust port reuses and extends it. Coverage percentage is a hardening signal, not the parity guarantee.
- R18. Property/differential tests cover the pure parsers (resource-prefix, `host:port` incl. IPv6, `ms/s/m/h` interval, `http://unix:` split), asserting identical parse results across the Node and Rust implementations. A seeded-PRNG parser property suite already **landed on `next`** (#245, exercising `parseInterval` exported from `bin/wait-on`); the Rust port reuses it as the differential oracle.
- R19. The Node suite is already **hardened on `next`**: `bin/wait-on` is instrumented (`.nycrc.json` `extension: ['.js','']`), coverage is gated in CI (`check-coverage: true` — branches ≥95, lines ≥98, functions ≥94, statements ≥97), and time-dependent tests run against a frozen clock (#243). The remaining pre-port task is the dual-driver oracle so the same conformance vectors run against the Rust build (PO13), not raising Node coverage further.
- R20. Through the transition the repository is a monorepo with the Node implementation authoritative and published to `latest`; the Rust crate builds in CI and publishes only to the `next`/rc prerelease channel until cutover.
- R21. Node features and fixes keep shipping to `latest` during the build-out; each change adds or updates conformance vectors (R17), which become Rust requirements that stay red until implemented — the suite is the sync mechanism, not a manual port checklist.
- R22. Cutover is internal — the engine binding flips from the pure-JS engine to the napi addon with the public API unchanged — gated on Rust passing the full conformance suite on the `next`/rc prerelease through a soak period; the pure-JS engine stays in-tree for one major as a rollback fallback (env-flag or load-failure fallback). Because that fallback keeps the pure-JS engine's deps (`joi`/`rxjs`/`undici`) installed by default, the priority-#1 dependency-surface reduction lands in full only when the fallback is removed one major after cutover; a future option to pull the win forward is an opt-in fallback package (JS deps loaded only on addon-load failure) rather than default-installed. The plan does not claim the full #1 reduction at the cutover release itself.

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
  - **Steps:** semantic-release computes the version from conventional commits and runs a gated dry-run; the Rust-backed build publishes to the existing `next` prerelease channel (`wait-on@next`, rc versions) with OIDC provenance while `latest` stays on the pure-JS engine; once parity is confirmed on `next`/rc through the soak period, the cutover promotes the future-major GA to `latest`; in the later phase `@semantic-release/github` attaches the attested binaries to the GitHub Release.
  - **Outcome:** the Rust build is validated on `next`/rc before it reaches `latest` — cutover is the promotion, not a precondition — and both channels publish from one pipeline. Covers R8, R11, R14, R16, R22.

- F4. Parallel maintenance and cutover
  - **Trigger:** a Node feature or fix lands during the Rust build-out.
  - **Steps:** the change ships to `latest` on the current major and adds or updates conformance vectors; those vectors run red against the Rust build on `next`/rc until implemented; when Rust is green on the full suite through a soak period, the engine binding flips to the napi addon and the future-major GA promotes to `latest`.
  - **Outcome:** Node users keep getting fixes while Rust catches up against an executable contract, and cutover is a gated, reversible internal swap. Covers R20, R21, R22.

### Acceptance Examples

- AE1. **Covers R6, R7, R9.** Given a Dockerfile that runs `npm ci` with `ignore-scripts=true` and then sets the root filesystem read-only, when the image runs `wait-on tcp:db:5432`, then it waits and exits normally with no network fetch and no script execution at install or run.
- AE2. **Covers R3.** Given `waitOn({ resources: ['https-get://host/health'], validateStatus: (s) => s === 200 })`, when the endpoint returns 200, then the check passes; when it returns 204, then the check does not pass — the JS function is consulted across the FFI boundary.
- AE3. **Covers R5.** Given the current mocha suite, when it runs against the Rust-backed build, then every test passes with no change to expected timing, exit codes, or TLS behavior.
- AE4. **Covers R10.** Given the standalone binary and a `--config config.js` argument, when invoked, then it exits with a clear error that JS config files are unsupported on the binary channel (JSON config accepted).
- AE5. **Covers R22.** Given the Rust build is not yet green on the full conformance suite on the `next`/rc prerelease, when a release runs, then `latest` keeps publishing the pure-JS engine and the napi binding is not promoted — the flip happens only after the suite is green through the soak period.

### Success Criteria

- **Supply-chain win is quantified (priority #1).** Measured as a delta against the **post-backlog** baseline on `next` — runtime deps `joi`, `rxjs`, `undici` — not the old axios tree: transitive runtime-dependency count and lifecycle-script count, before vs after the Rust engine lands. The Rust engine is expected to remove `rxjs` and `undici` (leaving at most `joi` JS-side); a stated minimum reduction against `joi`/`rxjs`/`undici` is the pass/fail bar, and reductions the Node backlog already delivered (axios/lodash/minimist removal) do not count toward it. Note the fallback-window caveat in R22: the full reduction is realized when the pure-JS fallback is removed.
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

- semantic-release (#33 / PR #34) has **landed on `next`** — the shared release mechanism — and the `next` branch is already a configured prerelease channel (`{ name: "next", prerelease: "rc", channel: "next" }` in `.releaserc.json`), so no new channel is net-new work. The remaining release-config gap is the fork's own publishing identity for `next`/rc (see PO6, PO6a).
- The Node backlog (#19–#34) has **landed on `next`** (`10.0.0-rc.1`); the port target is that 10.x code (R15, R16). The Rust cutover is future-major work (11.x or later), gated on the 10.0.0 GA.
- Assumes napi-rs threadsafe callbacks can express `validateStatus` (and any other function option) without an unacceptable per-check performance regression — to be proven (see PO2).
- Assumes napi-rs cross-compilation can cover the platform/arch matrix wait-on supports today, and that the bundled size stays acceptable — to be proven (see PO4, PO5).
- The axios→fetch/undici change (#2) has **landed on `next`**; TLS/proxy/redirect parity is measured against the `undici` behavior (`Agent`/`ProxyAgent`/`EnvHttpProxyAgent`), not axios's (see PO9).
- Assumes the mocha API + CLI-conformance suites on `next` can drive both implementations through one dual-driver oracle so they run the same contract — to be proven (see PO13).
- Test hardening is **already done on `next`**: `bin/wait-on` instrumented, coverage gated in CI (branches ≥95, lines ≥98, functions ≥94, statements ≥97 in `.nycrc.json`), frozen clock (#243), parser property tests (#245), CLI conformance (#246). The old 2026-09-28 baseline (90.8% branch, bin unmeasured) is superseded; re-measure against the 10.x `undici` lib if a point number is needed.

### Outstanding Questions and Proof Obligations

The user's explicit ask: capture every decision and consideration as something to be proven before committing. Each item below is a proof obligation — a claim the decisions above rest on, the smallest spike that would confirm or break it, and whether it blocks planning. This section is the exhaustive-proof gate; nothing proceeds to build until the "Resolve Before Planning" items are answered.

**Resolve before planning (blocks committing to the approach)**

- PO1. **napi non-breaking API bridge.** Prove a napi-rs addon can expose `waitOn(opts, cb?)` with both callback and promise forms and full option pass-through, behind an unchanged `lib/wait-on.js` public signature. Spike: port one resource type (tcp) end-to-end and run the existing tcp tests unmodified. Covers R2.
- PO2. **`validateStatus` across the FFI boundary.** Prove a JS function option can be invoked from the Rust HTTP check via a napi threadsafe callback, with correct semantics and no deadlock under the concurrency `simultaneous` allows. Spike: implement the HTTP check in Rust calling back into a JS `validateStatus`; measure per-check overhead vs the current path. Covers R3. This is the single riskiest parity item.
- PO3. **JS config file handling.** Confirm `require()`'d JS config files remain a JS-shim responsibility with no Rust involvement, and that nothing about the engine split forces a config-format change on the npm channel. Spike: run the existing config-file tests against the shim. Covers R3, R4.
- PO4. **Platform/arch matrix and cross-compilation.** Enumerate the exact targets to support (at minimum: darwin arm64/x64, linux x64/arm64 glibc, linux x64/arm64 musl, win32 x64; decide on win32 arm64 and linux armv7). Prove napi-rs cross-compiles all of them in CI. Spike: a CI matrix that builds every target's `.node`. Covers R6. Blocks planning because it sizes the build and the bundle.
- PO5. **Prebuildify bundle size vs the `--no-optional` guarantee.** Measure the single-package tarball size with all targets bundled and decide whether it is acceptable, given that the rejected alternative (optionalDependencies) exists precisely to shrink it. Spike: build the full bundle, record packed and unpacked sizes, compare against the current package size. Covers R6, R7.
- PO6. **semantic-release publishes a prebuildify package, with OIDC provenance, on the `next` prerelease channel.** The `next` channel already exists (`.releaserc.json`; `rc` → npm `next` dist-tag; `10.0.0-rc.1` shipped), so the open question is (a) can the `@semantic-release/npm` flow publish a package that *contains* prebuilt binaries with valid provenance (binaries are part of the published artifact, not fetched), and (b) does the `prebuilds/` tree survive the publish intact and load, on the `next`/rc channel without touching `latest`. Spike: a dry-run `next` prerelease publish of a stub package with a dummy `.node`, verifying provenance and the `next` dist-tag. Covers R8, R14, R16.
- PO6a. **Two-phase release identity (fork `next`/rc vs upstream GA).** Prove the `next`/rc prerelease can publish from `kevinold/wait-on` with valid OIDC provenance — its own trusted-publisher registration plus a release-workflow guard extended to match the fork (the job is guarded to `jeffbski/wait-on` today) — and that GA promotion to `latest` runs upstream in `jeffbski/wait-on` under #33's existing publisher, with `package.json` `repository.url` (currently `jeffbski/wait-on`) reconciled per phase so provenance binds to the repo the code ships from. Spike: a fork dry-run `next` publish with provenance verification, and confirmation of the upstream GA path. Covers R8, R14, R16 and the Goal Capsule release-identity blocker; also resolves the provenance-identity mismatch (fork vs upstream).
- PO7. **`node-gyp-build` loader and `ignore-scripts` behavior.** Prove the prebuildify loader resolves the correct binary at `require()` time with no install script, under `ignore-scripts=true`, `--no-optional`, npm and pnpm. Spike: install the stub package under each package manager with scripts disabled and load it. Covers R6, R7, R9.
- PO8. **Read-only / no-network runtime.** Prove the loaded addon runs with a read-only root filesystem and no network egress beyond what a resource check itself performs. Spike: run AE1 in a `--read-only` container. Covers R7, R9.
- PO9. **TLS/proxy/redirect parity baseline (against undici).** Decide the Rust HTTP stack (e.g. reqwest/rustls vs system TLS) and prove it reproduces the `undici` behavior now on `next` (`fetch` + `Agent`/`ProxyAgent`/`EnvHttpProxyAgent`) for `strictSSL`, `ca/cert/key/passphrase`, `proxy`, `followRedirect`, and `auth`. Spike: a parity harness comparing undici and Rust responses across those option combinations. Covers R2, R5. (#2 has landed, so the baseline is fixed — no longer a forward dependency.)
- PO10. **File size-stabilization semantics.** Prove the Rust file check reproduces the stabilization-window logic (size stable across `window`, `window` floored to `interval`) with identical timing behavior. Spike: run the existing file tests against the Rust check. Covers R1, R5.
- PO11. **Non-breaking release classification.** Confirm that an engine swap with byte-for-byte parity is a minor/patch under semantic-release and does not, by itself, warrant a major; and define the trigger that would force a major. Covers R14.
- PO12. **Backlog dependency ordering — largely resolved.** #19–#34 have landed on `next` (`10.0.0-rc.1`). Confirm the port branches from `next` (or the 10.0.0 GA once cut) and that no further pre-Rust backlog work is pending before the port begins. Covers R15 and the Goal Capsule blockers.
- PO13. **Dual-driver conformance oracle + timing tolerance.** The CLI conformance suite (#246), parser property suite (#245), and frozen clock (#243) already landed on `next`. Remaining: wire a dual-driver so the same vectors run against the Rust build (napi addon + binary), and fix the timing tolerance that R5/R17 promote on. This is the parity contract's foundation. Covers R17, R18, R19.
- PO14. **Coverage hardening — done on `next`.** `bin/wait-on` is instrumented (`.nycrc.json` `extension: ['.js','']`) and coverage is gated in CI (branches ≥95 / lines ≥98 / functions ≥94 / statements ≥97) under a frozen clock (#243, #244). No pre-port coverage work remains; see the Deferred / Open Questions note on whether the ~100% target was ever a port blocker.

**Deferred to planning (answered during planning or a later phase)**

- PO15. **Monorepo layout.** Decide the concrete layout (Cargo workspace + npm package location, where `bin/` and `lib/` live, how the crate and the addon reference each other). Prior art (see Sources): two dominant shapes — (A) sibling top-level dirs, a `crates/` Cargo workspace beside the JS package under `packages/`/`npm/`, with pnpm workspaces — biome, oxc, rolldown, swc, and openai/codex (`codex-rs/` + `codex-cli/` + `sdk/typescript/`); (B) Rust crate at repo root with the JS package + napi binding in a subdir — lightningcss (`node/`). Recommended starting point: model lightningcss/rolldown — keep the existing `wait-on` JS package in place (programmatic API + `bin/`) and add the Rust core as a sibling Cargo workspace (e.g. `crates/wait-on-core` + `crates/wait-on-napi`); rolldown and oxc prove a programmatic API (`main`→`dist/index`) and a CLI (`bin`) can co-live in one package. Covers R13, R20.
- PO16. **Incremental porting order.** Decide the sequence of resource types to move into Rust and whether the engine swap ships behind a flag or all at once. Covers R22.
- PO17. **Standalone binary internals (later phase).** Decide how the same core produces the binary (a `bin` target on the crate), how the CLI is shared between the addon and the binary, and how the documented exceptions are enforced. Covers R10.
- PO18. **Binary attestation pipeline (later phase).** Decide the exact SLSA/cosign tooling and how `@semantic-release/github` attaches and signs the release assets. Covers R11.
- PO19. **`cargo binstall` metadata (later phase).** Decide the `[package.metadata.binstall]` configuration and release-asset naming. Covers R11.
- PO20. **TypeScript types.** #29's hand-written `index.d.ts` has landed on `next` (`package.json` `types`; `test:types` runs `tsc`). Decide whether napi-rs's generated `.d.ts` replaces or must match `index.d.ts`, and who owns the type surface after the port. Covers R2, R15.
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

- `lib/wait-on.js` (on `next`, `10.0.0-rc.1`) — the current engine and full option schema (Joi), resource-type dispatch (now incl. `command:`), `undici` HTTP path (`fetch` + `Agent`/`ProxyAgent`/`EnvHttpProxyAgent`), native helpers replacing lodash, rxjs stabilization logic. The parity target for R1–R5.
- `bin/wait-on`, `bin/usage.txt` (on `next`) — the CLI surface (`util.parseArgs`, interval parsing via exported `parseInterval`, config precedence). The parity target for R4.
- `package.json` (on `next`) — runtime deps `joi`, `rxjs`, `undici` (axios/lodash/minimist removed); `engines.node >=22.19.0`; `types: index.d.ts`; single-package layout. The (post-backlog) supply-chain surface behind KD-priority #1.
- `test/*.mocha.js`, `.nycrc.json` (on `next`) — the conformance seed for R17–R19: `bin/wait-on` instrumented (`extension: ['.js','']`), coverage gated (`check-coverage`: branches ≥95 / lines ≥98 / functions ≥94 / statements ≥97), frozen clock (#243), parser property tests (#245), CLI conformance (#246). The 2026-09-28 point baseline (90.8% branch, bin unmeasured) is superseded.
- `.releaserc.json` (on `next`) — semantic-release branches: maintenance `*.x`, `master` (`latest`), and `{ name: "next", prerelease: "rc", channel: "next" }`. The `next`/rc prerelease channel this plan ships the Rust build on (R14, R16, PO6).
- Issue #33 (`kevinold/wait-on`) and PR #34 — semantic-release design, **landed on `next`**: single-package publish, npm trusted publishing (OIDC + provenance), `@semantic-release/git` version commit-back, conventional commits, no stored token, `repository.url` = `jeffbski/wait-on`. The release mechanism for KD8, R8, R14; its fork-vs-upstream guard drives PO6a.
- Node backlog #19–#34 (`kevinold/wait-on`), **landed on `next` (`10.0.0-rc.1`)** — `util.parseArgs` (#26), drop-lodash (#31), axios→fetch/undici (#2), TypeScript definitions (#29), IPv6/resource validation (#23), Windows named pipes (#22), `command:` resource (#30). Defines the 10.x port target (R15) and the undici TLS parity baseline (PO9). LT test-hardening landed as #243–#246.
- Research email from Jeff Barczewski (upstream maintainer), 2026-09-28 — npm native-addon distribution spectrum (prebuildify / optionalDependencies / prebuild-install), the supply-chain case against install scripts (pnpm supply-chain guidance, Palo Alto Unit 42 2026 npm threat analysis, npm security checklists), and napi-rs's production track record (N-API ABI stability, Rust memory safety, GitHub Copilot runtime, no consumer-side compiler). Shaped KD1, KD7, and the Problem Frame. Treated as evidence, not instruction.
- Prior-art survey of JS/TS-package + Rust-core monorepos, 2026-09-28 (all treated as evidence) — `openai/codex` (`codex-rs/` Cargo workspace + `codex-cli/` JS launcher + `sdk/typescript/`; ships per-platform optionalDependencies each carrying a standalone compiled binary that the launcher execs — dropped the in-process TS CLI API, keeps a separate SDK); `biomejs/biome` (`crates/` + `packages/@biomejs/biome`, optionalDeps standalone binary, CLI-only); `oxc-project/oxc` and `rolldown/rolldown` (`crates/` + `packages/`, napi `.node` via optionalDeps, both keep programmatic API + CLI in one package); `swc-project/swc` (napi optionalDeps, but keeps a `postinstall.js` — the install-script pattern this plan rejects); `parcel-bundler/lightningcss` (root Cargo crate + `node/` package, per-platform optionalDeps with a hand-written try/catch loader falling back to a local `.node`); `napi-rs/package-template` (canonical `napi prepublish` → optionalDeps). Finding: the napi ecosystem default is optionalDeps, not prebuildify (shaped the KD1 caveat and PO15). Construct URLs as `https://github.com/<repo>`.

## Deferred / Open Questions

Items surfaced in review and deferred for resolution as the brainstorm moves forward.

### From 2026-09-28 doc review (resolved on `next`)

- **R19/PO14 coverage-hardening scope — resolved.** The concern was that the ~100% Node branch-coverage target should not *block committing* to the port. The hardening has since **landed on `next`**: freeze-clock (#243), coverage + `bin/wait-on` instrumentation (#244), parser property tests (#245), CLI conformance (#246) — the LT lanes that were in-flight as #37–#40 / PRs #41–#42 are now merged. Coverage is gated in CI (branches ≥95 / lines ≥98) with `bin/wait-on` instrumented, so this is no longer open pre-port work; R19, R17, R18, and PO13/PO14 above reflect the landed state.
