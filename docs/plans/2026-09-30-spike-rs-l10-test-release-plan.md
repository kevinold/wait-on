---
title: "[L10] test-release readiness and supply-chain delta - Plan"
type: docs
date: 2026-10-01
deepened: 2026-10-01
topic: rust-port
lane: L10
kind: preview
branch: rs-62-test-release
closes: kevinold/wait-on#62
spine: kevinold/wait-on#35
spine_plan: docs/plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md
requirements_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
artifact_contract: ce-unified-plan/v1
product_contract_source: lane-requirements
execution: code
---

# [L10] test-release readiness and supply-chain delta - Plan

Deepened lane plan for sub-issue #62 (base `832c588`, verified 2026-10-01 against `xtask/src/package.rs`, `xtask/assets/prebuild-probe.js`, `lib/engine.js`, `lib/engine-rust.js`, `package.json`, `Cargo.lock`, `supply-chain/config.toml`, `docs/guides/*.md`, the fork's GitHub releases and the spine plan's operator decisions KD-S10..KD-S15). The spine plan is controlling and is never edited by this lane. L10 is the last lane: it runs after L11, L12 and L13 have merged.

Product Contract preservation: R-L10-1..R-L10-11 and T-L10-1..T-L10-4 keep their IDs and meaning. The original risk section's conditional sentence ("if the lane ends up docs-only ...") is resolved: KTD1 makes this a docs-only lane. Each T-L10-* is met by recorded command output in `docs/guides/releasing.md` and the PR body. The carve-out for each risk is recorded under Planning Contract.

---

## Goal Capsule

**Objective.** When this lane merges, a maintainer can show from the repo alone that:
- the latest fork GitHub prerelease tarball installs from the release URL with scripts disabled and runs the Rust engine end to end (AE1, AE2);
- the supply-chain win against `joi`/`rxjs`/`undici` and the Rust-side crate and cargo-vet counts are written down as numbers;
- the Rust pending list is empty;
- `docs/guides/` explains how to build, test and ship both engines with `cargo xtask`.

The spike PR #51 can then leave draft once the operator restores the full PR CI matrix.

**Means.** A docs-only lane (KTD1). Verification is one recorded run of hand-executed steps that the runbook documents. The run reuses the CI package job's own assets (the install cells' shape and `xtask/assets/prebuild-probe.js`, read but not edited) against the downloaded release tarball. It adds one throwaway AE2 driver, whose full text goes into the runbook (KTD2). Numbers are measured with `npm ls`, `cargo tree` and the vet store (KTD3, KTD4) and written into `docs/guides/releasing.md`. No executable code lands in the repo.

**Direction (operator, 2026-10-01).** Rust is primary and JS is what gets sunset (KD-S10). New automation would be a `cargo xtask` subcommand with Rust tests or a dependency-free POSIX sh script. This lane needs none. Rust crates stay at 100% line and region coverage (KD-S12). This lane adds no Rust code, so the gate is unchanged.

**Authority.** `AGENTS.md` > spine plan (KD-S1..KD-S15) > this plan.

**Stop conditions** (report to the PM, do not work around):
- a `.github/workflows/` change is needed. Restoring the full PR matrix (#77 trim) is an operator PR and is listed in the lane PR body as the remaining step, not done here;
- a public API, CLI, `WAIT_ON_SCHEMA` or `index.d.ts` change is needed;
- a new npm runtime dependency is needed;
- the fork prerelease fails AE1 or AE2 because of the engine itself. That is an engine-lane bug: report it with the reproduction, and do not patch outside this lane's scope;
- the verification cannot be completed without new executable code, for example because the release assets are missing or the probe asset no longer runs against an installed package. KTD1 then needs a PM decision between a `cargo xtask verify-release` subcommand (an `xtask/` allowed-paths question) and POSIX sh under `scripts/`.

**Execution profile.** `/lfg` → `/ce-work` from this plan. Strict TDD per `AGENTS.md` does not apply because only docs change (the docs-only carve-out). One PR to `kevinold/wait-on`, base `spike-next-rs`, body `Closes #62`. The lane never merges. Allowed paths: `crates/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `lib/`, `bin/`, `test/`, `index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`, `.gitignore`, `.nycrc.json`, `eslint.config.mjs`, `docs/guides/`, `docs/plans/`, `docs/solutions/`, `AGENTS.md`, `README.md`, `benchmarks/`, `scripts/`. `xtask/` is not in the list, and this plan touches only `docs/guides/`, `docs/plans/` and `docs/solutions/`. Files under `docs/` are never deleted or discarded. Every new or modified `docs/**` file (this plan, the guides, any `docs/solutions/` entry) is staged in the lane's commits.

---

## Product Contract

### Summary

Verify the newest `rs-<version>-<sha7>` prerelease on `kevinold/wait-on` end to end. Download it from the release URL, check `SHA256SUMS`, install it with scripts disabled, and run AE1 and AE2 with `WAIT_ON_ENGINE=rust-strict`. Record the supply-chain delta and the Rust-side crate and cargo-vet exemption counts. Confirm the pending list is empty. Finish `docs/guides/releasing.md` and the guides index so a new developer can build, test and ship both engines with `cargo xtask`.

### Problem Frame

Every spine lane has proven its slice in CI. The `package` job already installs the exact tarball bytes with scripts disabled and runs AE1 in read-only, no-network containers before the prerelease is uploaded.

What nobody has done yet is the maintainer's check. It starts from the release URL and trusts nothing but `SHA256SUMS`. It then shows the Rust engine running from the installed package, including AE2, which the package job does not cover. AE2 is a JS predicate called across the FFI boundary.

The supply-chain win is the requirements plan's priority #1, but it has never been written down as numbers against the agreed baseline. `docs/guides/releasing.md` still carries a `Status: planned (lane L10)` marker, and `contributing-dual-engine.md` says the marker must be gone before the lane is done.

### Starting state (verified 2026-10-01, base `832c588`)

- Latest fork prerelease: `rs-10.0.0-rc.1-832c588`, the base commit, published 2026-10-01T09:12Z. Its assets are `wait-on-10.0.0-rc.1.tgz` and `SHA256SUMS`. `rs-prerelease.yml` created it after the push's `build`, `rust` and `package` jobs passed.
- `cargo xtask package` (`xtask/src/package.rs`) always packs the checkout's own tarball and cannot take an external tarball path.
  - Install cells: `npm install --ignore-scripts`, the same with `--omit=optional`, and `pnpm add --ignore-scripts` (pnpm `10.34.6` via `npm exec`). Each runs in a fresh temp project with `WAIT_ON_NATIVE_LIBRARY_PATH` removed.
  - Refused lifecycle scripts: `preinstall`, `install`, `postinstall`, `prepare`.
  - Container cells: `node:24-trixie-slim` (glibc 2.41) and `node:24-alpine`, built with `.npmrc` `ignore-scripts=true` and run with `--read-only --network none -e WAIT_ON_ENGINE=rust-strict`.
- `xtask/assets/prebuild-probe.js` runs from any project that has `wait-on` installed.
  - It calls `resolveEngine` under the current `WAIT_ON_ENGINE`.
  - It waits on a local tcp listener through the installed API and the installed CLI.
  - It prints one JSON line with `addonPath`, `realpath`, `pkgDir`, `api`, `cli` and `cliError`.
  - `--no-listener --timeout <ms>` makes the wait time out.
- `lib/engine.js`: `WAIT_ON_ENGINE` accepts `js`, `rust` and `rust-strict`. `rust-strict` throws when the addon fails to load or has no `wait` export.
- `lib/engine-rust.js`: `validateStatus` stays in JS and is called from the Rust loop. `strictSSL` with `ca` becomes `roots: [ca]` for Rust.
- Committed TLS fixtures: `crates/wait-on-core/tests/fixtures/{ca,server,server-key}.pem`. `server.pem` is `CN=localhost` with SAN `localhost`, `127.0.0.1` and `::1`.
- `package.json`:
  - runtime `dependencies`: `joi ^18.2.9`, `rxjs ^7.8.2`, `undici ^8.10.2`;
  - no lifecycle scripts;
  - `files` ships `bin/`, `lib/`, `prebuilds/`, `exampleConfig.js` and `index.d.ts`.
- `Cargo.lock`: 156 `name =` entries, covering all crates including dev and `xtask`. `supply-chain/config.toml`: 134 `[[exemptions` entries and 7 `[imports` entries. As of L11/L12, architecture.md records 19 crates fully audited by imports and 134 exempted.
- The pending list was retired in L13. `test/parser-properties.mocha.js` asserts `report.stats.pending === 0` under `rust-strict`. CI's `rust` job runs mocha under `rust-strict` on ubuntu and windows for PRs, and on macOS for pushes.
- Guides and learnings:
  - The "End-to-end prerelease verification" section of `docs/guides/releasing.md` says `Status: planned (lane L10)`.
  - `docs/guides/README.md` describes the releasing row as "Node channels (pointer), Rust test prereleases on the fork, deferred items".
  - `docs/guides/` has no stale Justfile or `scripts/*.js` references.
  - `docs/solutions/best-practices/trimmed-pr-matrix-lets-platform-bugs-escape-to-the-base.md` has `retire_when: "The PR matrix is restored to the full push matrix (L10 #62)"`.
- Issue #85 (open) is the JS-to-Rust tooling inventory (KD-S11). The PM's comment on issue #62 asks for an operator PR that restores the full PR matrix in `.github/workflows/node.js.yml` (build 2×3, rust 3 OS, napi 8 targets, package on PRs) before #51 leaves draft. L10's readiness check confirms it.
- Dev host: darwin arm64 with docker (linux/arm64 engine), `gh`, `pnpm`, `cargo` and `node`. The glibc container cell therefore exercises the `linux-arm64` prebuild.

### Requirements

**Release verification**

- R-L10-1 Release-URL verification. The latest fork prerelease tarball is fetched from its GitHub Release URL, its SHA-256 matches `SHA256SUMS`, and it installs into a clean temp project with scripts disabled (`--ignore-scripts`). The verified tag, sha and date are recorded.
- R-L10-2 AE1 from the release. The installed tarball runs `wait-on tcp:<port>` in a read-only, no-network container under `WAIT_ON_ENGINE=rust-strict`, so a silent JS fallback fails. It waits and exits 0, and no script runs at install time.
- R-L10-3 AE2 from the release. Against the installed tarball, under `WAIT_ON_ENGINE=rust-strict`, `waitOn({ resources: ['https-get://…'], validateStatus: s => s === 200 })` passes on a 200 and does not pass on a 204 (it times out or rejects). The test proves the JS predicate was called (the path ran), not only the outcome.
- R-L10-4 Engine actually loaded. Each verification asserts that the Rust addon loaded from inside the installed package, not from the repo checkout and not through the JS fallback.

**Numbers and confirmations**

- R-L10-5 Supply-chain delta recorded. Record the transitive runtime-dependency count and the lifecycle-script count for three states, measured against the Success Criteria in the requirements plan. Gains from the earlier axios, lodash and minimist removal do not count.
  - the `next` baseline (`joi`, `rxjs`, `undici`);
  - the Rust engine path as shipped today (fallback window, all three still declared);
  - the projected post-cutover state (`joi` at most).
- R-L10-6 Rust-side counts recorded. Record the Rust crate count in `Cargo.lock`, with the runtime crates of `wait-on-core` and `wait-on-napi` counted separately from dev and xtask crates. Also record the cargo-vet exemption and import counts in `supply-chain/config.toml`.
- R-L10-7 Pending list empty. Confirm and record that every API, CLI, conformance and property test passes under `WAIT_ON_ENGINE=rust-strict` with no pending or skip list (KD-S12).

**Guides**

- R-L10-8 Releasing guide finished. `docs/guides/releasing.md` replaces "Status: planned" with a working runbook containing:
  - the commands for R-L10-1..R-L10-4;
  - the recorded results;
  - the supply-chain delta table;
  - a link to #85 (the JS-to-Rust tooling inventory);
  - a list of what the cutover still needs (KD-S11 release tooling evaluation, JS fallback removal, default engine flip, npm trusted publisher).
- R-L10-9 Guides index finished. `docs/guides/README.md` and the pages it lists let a new developer build, test and ship both engines with `cargo xtask` as the front door. No stale references to removed JS scripts or a Justfile remain. The index's releasing row reflects the new content.

**Readiness and knowledge**

- R-L10-10 Draft-exit readiness. The PR body lists the readiness checklist for #51: the items above, plus the remaining operator step to restore the full PR matrix (build 2×3, rust 3 OS, napi 8 targets, package on PRs) in `.github/workflows/node.js.yml`.
- R-L10-11 Knowledge lands in repo. Non-obvious learnings go to `docs/solutions/` (via `/ce-compound mode:non-interactive`) or to the Resume notes below, in this PR.

### Named risks → tests (AGENTS.md: a prose check does not count)

- T-L10-1 Silent JS fallback hides a broken addon. If automation is added, its Rust test asserts that it sets `WAIT_ON_ENGINE=rust-strict` and checks the addon load path. Manual runs use `rust-strict`.
- T-L10-2 Checksum mismatch accepted. If automation verifies `SHA256SUMS`, a Rust unit test feeds it a tampered digest and expects a failure.
- T-L10-3 AE2 passes through a fallback. The AE2 check counts predicate calls (at least one with 200 and at least one with 204) and asserts that the 204 run does not succeed.
- T-L10-4 Delta numbers drift from reality. If automation computes the counts, a Rust test checks the counting against a small fixture lockfile with concrete expected numbers.

KTD1 adds no automation, so each risk is met by a recorded observation instead. The carve-outs under Planning Contract › Risks name the observation for each risk.

### Scope Boundaries

- Not here: workflow edits, npm publish, the default-engine flip, JS fallback removal, the standalone binary, and release-tool replacement (KD-S11).
- Non-goals (least code that works, KD-S10):
  - No `cargo xtask verify-release` subcommand. The package job already proves the same install cells and AE1 on the identical bytes, a one-off URL check does not justify a subcommand, and `xtask/` is outside allowed paths.
  - No POSIX sh wrapper under `scripts/`. It would be a second front door beside `cargo xtask`, with no test harness of its own.
  - No JS or mocha test of the installed tarball. JS tests stay at the API and CLI front doors of the checkout, and AE2's behaviour is already tested there under `rust-strict`.
  - No container cell for an older glibc. The glibc floor is a `napi` build question, and `releasing.md` records it.

---

## Planning Contract

- KTD1. **Docs-only verification, run by hand from the runbook.** The open area is resolved as option (a).
  - **How it runs:**
    - The steps run once by hand on the dev host, in the order the runbook lists them. Their output is recorded in `docs/guides/releasing.md` and the PR body.
    - The install reuses the first cell that `xtask/src/package.rs` already proves in CI (npm `--ignore-scripts`). The other cells are not repeated, because CI already proves them on the same bytes.
    - The addon-path and AE1 proof reuse `xtask/assets/prebuild-probe.js` as a read-only asset. It is copied into the temp project and the docker context, never edited.
    - The container cells reuse the images, `.npmrc` and `docker run` flags from `container_cells`.
  - **Why not (b), a `cargo xtask verify-release <tag>` subcommand:** the CI `package` job already ran these cells and AE1 on the same bytes before upload. This lane adds only three things: starting from the URL, checking the checksum, and AE2. That is a single manual run, not a tool. A subcommand would also need an `xtask/` allowed-paths decision and Rust tests for a tool that runs once per prerelease.
  - **Why not (c), POSIX sh under `scripts/`:** it would add a second front door beside `cargo xtask` (KD-S10) with no test seam.
  - **Later:** if the operator wants this repeatable in CI, the runbook becomes the spec for a future `xtask` lane. `releasing.md` records it as a cutover follow-up. This lane does not file it.
- KTD2. **The AE2 driver is a throwaway script, and its text goes into the runbook.** AE2 needs a local https server that returns 200 and then 204, and a caller that passes `validateStatus` through the installed `waitOn`.
  - **How it runs:**
    - The driver is a short Node script written in the scratchpad.
    - It runs from the temp project under `WAIT_ON_ENGINE=rust-strict`.
    - Its text is copied verbatim into `releasing.md`, so a maintainer can paste and rerun it.
  - **TLS:** it uses the committed `crates/wait-on-core/tests/fixtures/{server,server-key,ca}.pem` (the SAN covers `localhost` and `127.0.0.1`) with `ca` and an explicit `strictSSL: true`. The schema default is `false` (`lib/wait-on.js`), and `rustTlsOptions` in `lib/engine-rust.js` passes `roots` only when `strictSSL` is true, so without it the real TLS path would not run.
  - **Routing:** the driver runs with `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, `https_proxy` and `NO_PROXY` unset. `lib/wait-on.js` sends a wait to `lib/engine-js.js` whenever `rust.routable()` is false (for example, an https target behind an env proxy), and `rust-strict` only governs whether the addon loads.
  - **What it asserts:**
    - After both runs, no `require.cache` key ends with `wait-on/lib/engine-js.js` and none sits under `node_modules/undici` or `node_modules/rxjs`. `engine-js` is required lazily, so its absence proves the Rust path ran the wait.
    - It counts predicate calls with a closure.
    - It asserts at least one call on 200 and at least one on 204.
    - It asserts that the 204 run rejects with the timeout error.
    - It prints the installed addon's realpath via `require('wait-on/lib/engine').addonPath(process.env)` and `fs.realpathSync`, resolved from the temp project.
  - **Why not certs generated with `openssl`** (the JS suite's approach): the PEM fixtures already exist and need no tool.
  - **Why not a mocha test in `test/`:** the subject is the installed tarball, not the checkout, and JS tests belong at the checkout's front doors.
- KTD3. **The supply-chain delta is measured with `npm ls` on clean installs.** Each of the three states is counted in a fresh temp project.
  - **Baseline and shipped-today rows:** these are the same install, because during the fallback window the Rust path declares the same three deps (R22). The install is the release tarball with `--ignore-scripts`. The count is the distinct packages under `node_modules` minus `wait-on` itself (`npm ls --omit=dev --all --parseable`).
  - **Post-cutover row:** install `joi` alone, at the version `package.json` pins.
  - **Lifecycle-script count:** the number of installed packages whose `package.json` declares any of `preinstall`, `install`, `postinstall` or `prepare`. These are the same four that `xtask/src/package.rs` refuses.
  - **Recorded with the table:** the registry resolution date and the exact versions resolved, because `^` ranges move.
  - **Why not read `package-lock.json`:** it includes dev deps, which would need filtering by hand.
  - **Why not inspect the `npm pack` tarball:** it does not resolve transitive deps.
- KTD4. **Rust counts come from `cargo tree` and a grep of the vet store.**
  - **Runtime crates:** the deduplicated package set from `cargo tree -p wait-on-napi -e normal --target all --prefix none`. `--target all` matters: the addon ships for eight targets, and the host-only default drops platform-gated crates such as `windows-sys` (103 vs 138 crates on the darwin dev host). `releasing.md` states that the count spans all targets. This includes `wait-on-core` and its normal deps. The two workspace crates are subtracted, and the subtraction is stated.
  - **Dev, build and `xtask` crates:** everything else in `Cargo.lock`, that is, the total `name =` entries minus the runtime set.
  - **Vet counts:** the number of `[[exemptions` and `[imports` occurrences in `supply-chain/config.toml`, cross-checked against the audited/exempted sentence in `docs/guides/architecture.md`. That sentence is updated only if the numbers moved.
  - **Why not `cargo vet` summaries:** they need network access, and their output differs across the 0.10.x versions already noted in the guides.
- KTD5. **The pending-list confirmation is an observation, not a new test.** `test/parser-properties.mocha.js` already fails under `rust-strict` if `report.stats.pending` is not 0. The L13 inventory in `testing.md` records that the pending list is retired. The lane runs the mocha suite under `rust-strict` on the dev host, records the summary line (passing count, `pending: 0`, no skips), and cites the base commit's green `rust` job rows. The assertion already exists, so no new one is added elsewhere.
- KTD6. **No workflow edits.** The full PR matrix restore is listed in the PR body as an operator step. (session-settled: user-directed — chosen over editing node.js.yml in this lane: lanes never edit workflows)
- KTD7. **Any automation would be a cargo xtask subcommand with Rust tests, or dependency-free POSIX sh.** KTD1 needs neither. (session-settled: user-directed — chosen over JS scripts, JS devDeps, npm lifecycle scripts, Justfile, Claude hooks: Rust primary, JS sunset, KD-S10)
- KTD8. **The Rust crates stay at 100% line+region llvm-cov.** This lane adds no Rust code. (session-settled: user-directed — chosen over lowering the gate for new code: operator bar, KD-S12)
- KTD9. **The delta also records the Rust crate count and the cargo-vet exemption count, and releasing.md links #85 and lists what the cutover still needs.** Governs R-L10-6 and R-L10-8. (session-settled: user-directed — chosen over a joi/rxjs/undici-only delta: PM asked for both)

### Assumptions

- This is a headless run with no user confirmation. The plan makes two unconfirmed guesses:
  - the operator accepts a docs-only lane rather than a verification tool (KTD1);
  - reading `xtask/assets/prebuild-probe.js` and copying it into a temp project counts as using it, not editing `xtask/`, so it stays inside allowed paths.
- The newest prerelease at execution time is the one verified. If a push to `spike-next-rs` has produced an `rs-10.0.0-rc.1-<sha7>` newer than `832c588`, verify that one and record it. The runbook works for any tag.
- docker, `gh`, `pnpm` (via `npm exec`) and registry access are available on the dev host. The container cells need the linux prebuilds for the host arch, and every prerelease tarball carries them.
- The glibc floor (2.39) holds. `node:24-trixie-slim` is the glibc image, as in `container_cells`. A failure on an older image is a known failure mode, not a lane finding.
- The evidence is the run on the dev host. CI does not rerun the runbook. The lane PR's Windows row runs only the unchanged suites.

### Open Questions (deferred, non-blocking)

- Supply-chain pass/fail bar. The requirements plan's Success Criteria make "a stated minimum reduction against `joi`/`rxjs`/`undici`" the pass/fail bar, but no minimum is stated anywhere. The shipped-today row equals the baseline (R22), so only the post-cutover projection shows a win. This lane records the numbers. The operator states the minimum and whether it gates #51's draft exit or only the cutover. The PR body and `releasing.md` mark the bar as "not yet stated" rather than guess it.

### Risks

- T-L10-1 carve-out. No automation lands, so the evidence is the recorded probe output. The install cell and the container cell each print a JSON line whose `realpath` passes the `assert_installed_addon` rule in `xtask/src/package.rs`: it starts with the realpath of the project root (the temp project or `/app`) and ends with `prebuilds/<dir>/wait-on.node`. Each run has `WAIT_ON_ENGINE=rust-strict` set, and the container `run` line and host env are recorded verbatim. A negative is recorded too: the same probe with `WAIT_ON_ENGINE=rust-strict` and `WAIT_ON_NATIVE_LIBRARY_PATH` pointing at a missing file fails to load instead of falling back.
- T-L10-2 carve-out. The evidence is the output of the system `shasum -a 256 -c SHA256SUMS` on the downloaded assets. A negative is recorded too: flip one hex digit in a copy of `SHA256SUMS` and observe the check fail. Both outputs are recorded.
- T-L10-3. The KTD2 driver's own assertions cover this: at least one call on each run, the 204 run rejects with the timeout message, and the realpath is inside the installed package. Its output is recorded, and the driver text is in the runbook so it can be rerun.
- T-L10-4 carve-out. The numbers are measured, not computed by code. The runbook records the exact commands, the resolved versions and the date next to each number, so drift is visible and reproducible. The grep counts are cross-checked against the existing vet sentence in `architecture.md`.
- The release lacks an asset, or the probe no longer runs from an installed package. This is a stop condition, and KTD1 reopens.
- Registry resolution changes the `joi` transitive count between the run and the PR review. The table records versions and the date, so a reviewer who reruns it sees a newer dated row rather than a contradiction.

---

## Implementation Units

### U1. Release verification run: URL, checksum, install cells, AE1 in containers

**Goal.** Produce the recorded evidence for R-L10-1, R-L10-2 and R-L10-4, including the T-L10-1 and T-L10-2 negatives.

**Requirements.** R-L10-1, R-L10-2, R-L10-4; T-L10-1 and T-L10-2 (carve-outs); KTD1.

**Dependencies.** None; this is the first unit. Needs docker, `gh` and registry access on the dev host.

**Files.** None in the repo. Outputs are captured to the scratchpad for U4 and U5:
- the release tag, sha and publish date;
- the checksum output and its tampered negative;
- the probe JSON line from the npm install cell;
- the output of the glibc container `ready` run;
- the strict-mode load-failure negative.

**Approach.**
1. Find the newest `rs-*` prerelease with the GitHub CLI. Record its tag, sha7 and `publishedAt`.
2. Download both assets into a scratch directory and check the digest against `SHA256SUMS` with the system SHA-256 tool. Then copy `SHA256SUMS`, flip one digit, and observe the check fail.
3. Run one install cell, npm `--ignore-scripts`, the first cell in `xtask/src/package.rs`. The `--omit=optional` and pnpm cells, the musl image, and the container timeout runs are already proven on the same bytes by the CI `package` job; the runbook says so and does not repeat them. For the cell:
   - create a fresh temp project with a stub `package.json`;
   - install the downloaded tarball, and confirm from a clean install log that no lifecycle scripts ran;
   - copy `xtask/assets/prebuild-probe.js` into the project and run it with `WAIT_ON_ENGINE=rust-strict`;
   - keep the JSON line, and check that `realpath` starts with the realpath of the temp project root (macOS resolves `/var` to `/private/var`) and ends with `prebuilds/darwin-arm64/wait-on.node`.
4. Write the same docker context that `write_docker_context` writes: the Dockerfile from `container_cells`, `.npmrc` with `ignore-scripts=true`, a stub `package.json`, the downloaded tarball as `wait-on.tgz`, and the probe. Build the glibc image.
5. Run it with `--read-only --network none -e WAIT_ON_ENGINE=rust-strict` in `ready` form: exit 0, `api: true`, `cli: 0`, and a realpath that starts with `/app/` and ends with `prebuilds/linux-arm64/wait-on.node`.
6. Run the T-L10-1 negative on the host: the probe under `rust-strict`, with `WAIT_ON_NATIVE_LIBRARY_PATH` set to a missing path, exits non-zero with the `failed to load the native addon` message.

If any cell fails because of the engine itself, stop (Goal Capsule).

**Test expectation:** none -- no executable code lands, and the recorded outputs are the evidence (T-L10-1 and T-L10-2 carve-outs).

**Verification.** Every captured output exists and matches the pass shapes above. The verified tag is the newest prerelease at run time.

### U2. AE2 from the release: the JS predicate across the FFI boundary

**Goal.** Produce the recorded evidence for R-L10-3 and R-L10-4, with the driver itself asserting T-L10-3.

**Requirements.** R-L10-3, R-L10-4; T-L10-3; KTD2.

**Dependencies.** U1 (an installed temp project from the npm cell).

**Files.** None in the repo. The driver script lives in the scratchpad. Its full text and its output are captured for U4.

**Approach.**
1. Write the KTD2 driver. It starts a Node `https` server on an ephemeral port using `crates/wait-on-core/tests/fixtures/server.pem` and `server-key.pem`. The server answers with the configured status and an empty body.
2. The driver resolves the installed `wait-on` from the temp project. It calls `waitOn` with `https-get://localhost:<port>/health`, `ca` set to `ca.pem`, `strictSSL: true`, `validateStatus` wrapped in a call counter, and a short `timeout` and `interval`.
3. Run it twice:
   - with 200: it must resolve, with at least one call;
   - then with 204: it must reject with the `Timed out` message, with at least one call from this run alone.
4. Print the counts, plus `addonPath` from the installed `wait-on/lib/engine` and its realpath. Exit non-zero if any assertion fails.
5. Run it from the temp project with `WAIT_ON_ENGINE=rust-strict` and the proxy variables unset (KTD2), and assert the `require.cache` check after both runs.

**Test expectation:** none -- the driver is throwaway and its text is copied into the runbook. Its own assertions cover T-L10-3.

**Verification.** The driver exits 0 under `rust-strict`. The recorded output shows at least one call on each run, the 204 rejection text, no `engine-js`, `undici` or `rxjs` module loaded, and a realpath that starts with the temp project root's realpath and ends with `prebuilds/darwin-arm64/wait-on.node`.

### U3. Numbers: supply-chain delta, Rust counts, pending-list confirmation

**Goal.** Measure and record R-L10-5, R-L10-6 and R-L10-7.

**Requirements.** R-L10-5, R-L10-6, R-L10-7; T-L10-4 (carve-out); KTD3, KTD4, KTD5, KTD9.

**Dependencies.** U1. The installed npm-cell temp project is the measured install for the baseline and shipped-today rows.

**Files.** None in the repo; the results flow into U4. Reads `package.json`, `Cargo.lock`, `supply-chain/config.toml` and `docs/guides/architecture.md`.

**Approach.**
1. Per KTD3, in the U1 npm-cell project:
   - list the installed runtime packages and count them minus `wait-on`;
   - count the packages that declare any of the four install scripts.
2. Still per KTD3, create a second temp project and install `joi` alone at the `package.json` range with scripts disabled. Count it the same way. This is the post-cutover projection.
3. State two things explicitly:
   - the shipped-today row equals the baseline row because of the fallback window (R22);
   - the axios, lodash and minimist gains are excluded by construction, because the baseline is the `next` tree.
4. Per KTD4:
   - compute the runtime crate set for `wait-on-napi` with `cargo tree` (normal edges, all targets, deduplicated) and subtract the two workspace crates;
   - compare with the `Cargo.lock` total to get the dev, build and xtask remainder;
   - grep the vet store for the exemption and import counts;
   - check whether the "19 fully audited, 134 exempted" sentence in `architecture.md` still holds, and note the result.
5. Per KTD5, run the mocha suite under `WAIT_ON_ENGINE=rust-strict` on the dev host with a host prebuild. Record the summary (passing, failing 0, pending 0), and cite the base commit's `rust` job rows as the CI confirmation.

Every number carries its command, date and resolved versions.

**Test expectation:** none -- the numbers are measurements recorded with their commands (T-L10-4 carve-out), and the pending-list assertion already exists in `test/parser-properties.mocha.js`.

**Verification.** Three outputs exist, each with its command and date:
- a delta table with three rows and two columns (transitive runtime deps, lifecycle scripts);
- a Rust table (runtime crates, other crates, exemptions, imports);
- the mocha summary line.

### U4. `docs/guides/releasing.md` runbook and the guides pass

**Goal.** Replace the `Status: planned (lane L10)` marker with the working runbook and the recorded results, and bring the guides index and pages up to date (R-L10-8, R-L10-9).

**Requirements.** R-L10-8, R-L10-9; KTD9.

**Dependencies.** U1, U2, U3.

**Files.**
- `docs/guides/releasing.md`;
- `docs/guides/README.md`;
- `docs/guides/architecture.md`, only if the vet counts moved;
- `docs/guides/contributing-dual-engine.md`, only if its releasing checklist row needs the new anchor.

**Approach.**
1. Rewrite the "End-to-end prerelease verification" section of `releasing.md` to contain, in order:
   - the verified tag, sha and date;
   - the ordered runbook:
     - find the newest prerelease;
     - download both assets;
     - check `SHA256SUMS`;
     - install into a fresh temp project with scripts disabled, in each of the three cells;
     - run the probe under `rust-strict`;
     - run the container cells with their exact `docker run` flags;
     - run the AE2 driver (its full text and how to run it);
     - run the two negatives;
   - the recorded outputs, condensed to the JSON lines and summary lines;
   - the supply-chain delta table and the Rust counts table, each with its commands and dates, framed against the requirements plan's Success Criteria (post-backlog baseline; fallback-window caveat R22);
   - the pending-list confirmation;
   - a link to #85;
   - a "What the cutover still needs" list: the KD-S11 release tooling evaluation, JS fallback removal, the default engine flip, npm trusted publisher registration, the optional future `xtask` verify subcommand (with the runbook as its spec), and the glibc floor note that is already present.
2. Keep the existing "Rust test prereleases (fork)" section. If its `Install and try` snippet would duplicate the runbook, replace it with a pointer to the runbook.
3. In `README.md` (the guides index), rewrite the releasing row to name the runbook, the delta and the cutover list.
4. Re-read every listed page for statements that will be untrue once this merges: planned markers, L-numbered "exists" cells, and `scripts/*.js` or Justfile mentions (none known). Fix only what is stale.
5. Honor the rule in `contributing-dual-engine.md`: no planned-status marker naming L10 may remain anywhere under `docs/guides/`.

**Test expectation:** none -- docs-only edits are exempt (AGENTS.md).

**Verification.**
- A search of `docs/guides/` for `planned (lane L10)` finds nothing.
- The index row matches the page.
- Every command in the runbook was run in U1..U3, and its recorded output appears beside it.

### U5. PR readiness checklist, learnings, and the plan's Resume notes

**Goal.** Make the lane PR the draft-exit readiness record for #51 (R-L10-10), and land the learnings in the repo (R-L10-11).

**Requirements.** R-L10-10, R-L10-11; KTD6.

**Dependencies.** U4.

**Files.**
- `docs/plans/2026-09-30-spike-rs-l10-test-release-plan.md` (Resume notes, append-only);
- `docs/solutions/best-practices/`, only if `/ce-compound mode:non-interactive` finds a non-obvious learning. Candidates: the pattern of verifying from the release URL by reusing the package job's assets, and anything AE2 revealed about TLS roots through the installed package;
- the PR body (not a repo file).

**Approach.**
1. The PR body carries `Closes #62` and a checklist:
   - R-L10-1..R-L10-9, each with a one-line result and a link to its `releasing.md` anchor;
   - T-L10-1..T-L10-4, each with its recorded observation;
   - the pending-list line;
   - the remaining operator step, marked as not done here: restore the full PR matrix in `.github/workflows/node.js.yml` (build 2×3, rust 3 OS, napi 8 targets, package on PRs; per #77/#83 and KD-S14) before #51 leaves draft. Note that `docs/solutions/best-practices/trimmed-pr-matrix-lets-platform-bugs-escape-to-the-base.md` retires with that operator PR, not with this lane.
2. Run `/ce-compound mode:non-interactive`. If it writes nothing, say so in the Resume notes.
3. Append the Resume notes: the verified tag, the dated numbers, and any deviation from this plan.

All `docs/**` changes (this plan, the guides, any solution) are staged in the lane's commits. Nothing under `docs/` is deleted.

**Test expectation:** none -- docs and PR body only.

**Verification.**
- The PR body checklist has every R and T line, each with a result.
- The Resume notes are appended.
- `docs/solutions/` either has the new entry, or the Resume notes record the decision not to write one.

---

## Verification Contract

| Check | Command / observation | Proves |
|---|---|---|
| Release verified from the URL | recorded tag, sha and date; checksum pass and tampered-digest failure (U1) | R-L10-1, T-L10-2 |
| AE1 from the release | npm install-cell probe line and glibc container `ready` output under `rust-strict`, realpath inside the installed package; strict load-failure negative (U1) | R-L10-2, R-L10-4, T-L10-1 |
| AE2 from the release | KTD2 driver output: at least one call on 200 and on 204, 204 rejects, realpath inside the installed package (U2) | R-L10-3, R-L10-4, T-L10-3 |
| Supply-chain delta and Rust counts | the two tables, with commands, versions and dates (U3) | R-L10-5, R-L10-6, T-L10-4 |
| Pending list empty | `WAIT_ON_ENGINE=rust-strict npm run test:mocha` summary on the dev host; the base commit's green `rust` rows | R-L10-7 |
| Repo behaviour unchanged | `npm test` and `npm run ci:rs` green on the lane PR's rows (no `lib/`, `bin/`, `test/` or `crates/` edits) | lane is docs-only |
| Guides | no `planned (lane L10)` under `docs/guides/`; index row matches the page | R-L10-8, R-L10-9 |
| Readiness | PR body checklist, including the operator matrix-restore step; Resume notes appended | R-L10-10, R-L10-11 |

---

## Definition of Done

- R-L10-1..R-L10-11 are met. T-L10-1..T-L10-4 each have their recorded observation in `releasing.md` and the PR body (carve-outs recorded, no new executable code).
- `docs/guides/releasing.md` has the runbook, the results, both tables, the #85 link and the cutover list. No `Status: planned (lane L10)` remains under `docs/guides/`, and the index row is current.
- The verified prerelease is the newest fork prerelease at run time, named by tag, sha and date.
- Cleanup:
  - the diff contains no scratch assets, downloaded tarballs, `SHA256SUMS`, temp projects, docker contexts, or changes to `prebuilds/`, `target/` or `node_modules`;
  - the throwaway driver exists only as text inside the runbook;
  - the docker images built for the run are removed from the dev host.
- `npm test` and `npm run ci:rs` are green on the PR rows. `Cargo.lock`, `package.json` and `package-lock.json` are unchanged.
- `/ce-compound mode:non-interactive` has run and the Resume notes are appended. Every `docs/**` file created or changed is committed in the lane PR, and nothing under `docs/` is deleted.
- The PR is open against `spike-next-rs` with `Closes #62`. Its body carries the #51 readiness checklist, with the operator matrix-restore step as the one remaining item.

---

## Resume notes

(append-only; lane notes go here)

- 2026-10-01: verified `rs-10.0.0-rc.1-832c588` (newest at run time; base `832c588`). Checksum OK, tampered copy fails. npm `--ignore-scripts` install and probe under `rust-strict` pass, and the strict-load negative fails as expected. AE1 passes in `node:24-trixie-slim` (glibc 2.41, aarch64) with `--read-only --network none`. AE2 passes: 1 call on 200, 15 on 204 (timed out), `engine-js`/`undici`/`rxjs` never loaded. Mocha under `rust-strict`: 496 passing, 0 pending. Results and runbook are in `docs/guides/releasing.md`.
- Numbers (2026-10-01T11:27Z): 11 runtime packages today (= baseline, R22), 8 projected after cutover. `prepare` declared by 2 packages (`@hapi/tlds`, `undici`) today and 1 after; 0 install-time hooks in every state. Rust: 156 `Cargo.lock` packages, 134 third-party runtime crates across all targets (100 host-only), 20 dev/build/xtask, 134 vet exemptions, 7 imports (architecture.md's 19 audited / 134 exempted still holds).
- Deviation: the guides pass (U4) also found the `ci.md` size table stale. The tarball grew from 3.4 MB to 15.0 MB packed after HTTP/TLS moved into the addon (L4/L5), so the table was refreshed from the release. `ci.md` described only the push matrix, so a note on the trimmed PR matrix was added. Lane range L1–L10 became L1–L13 in the index and contributing pages.
- Residual: the base commit's `pull_request` CI run 36839668677 failed `rust (windows-latest)` on `waiter::tests::reverse_file_is_ready_once_removed_with_no_window` (log count 3 vs 4 under coverage, the Windows delete-pending pattern). The push run on the same commit was green. Not fixed here (engine-lane test, outside this docs lane); listed in the PR body.
