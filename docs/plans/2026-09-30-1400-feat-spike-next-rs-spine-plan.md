---
title: spike-next-rs — Rust engine buildout, side-by-side with Node (spine)
type: feat
date: 2026-09-30
topic: rust-port
spine: kevinold/wait-on#35
source_plan: docs/plans/2026-09-28-1239-feat-rust-port-plan.md
---

# spike-next-rs — Rust engine buildout (spine)

This is the controlling plan for the Rust-port spike. It turns the requirements in
`docs/plans/2026-09-28-1239-feat-rust-port-plan.md` (R1–R23, PO1–PO22) into ordered,
single-PR lanes. Lane workers read it; they never edit it. Lane notes go under the
lane's own child plan `## Resume notes`.

## Shape

- **Spike branch:** `spike-next-rs`, cut from `next` (`10.0.0-rc.1`). One long-lived draft
  PR `spike-next-rs → next` in `kevinold/wait-on` carries the whole buildout, so the work is
  visible in one place.
- **Spine:** issue `kevinold/wait-on#35`. Each lane is a native sub-issue of #35 carrying a
  YAML lane contract, driven by `multi-worker-pm --mode spine` (one lane in flight). Each
  lane is one PR into `spike-next-rs`; the PM merges it after green checks and the
  pre-merge checklist.
- **Upstream is out of scope.** Nothing is pushed to or opened against `jeffbski/wait-on`.

## Key decisions (PM, 2026-09-30)

- **KD-S1 Side by side, JS default.** The pure-JS engine stays the default and stays
  authoritative. The Rust engine is opt-in via `WAIT_ON_ENGINE=rust` (env). If the addon
  fails to load, wait-on falls back to JS (`WAIT_ON_ENGINE=rust-strict` makes a load
  failure an error, for CI). The public API, CLI, schema, and `index.d.ts` do not change.
- **KD-S2 One contract, two drivers.** The existing mocha suites (`test/*.mocha.js`,
  including CLI conformance and parser property tests) are the parity contract. CI runs
  them once per engine. A test that cannot pass on Rust yet is listed in one explicit
  pending list (`test/rust-pending.js` or equivalent) that each lane shrinks; the list
  must be empty before the spike PR leaves draft.
- **KD-S3 Port order: checks first, loop last.** Lanes L2–L6 move one resource check at a
  time into Rust behind the existing rxjs pipeline (JS still orchestrates; Rust answers
  "is this resource ready?"). L7 moves the polling/stabilization loop into Rust so the
  Rust path no longer touches `rxjs`/`undici`. This keeps every lane small and testable
  and answers PO1/PO2/PO10 early.
- **KD-S4 Layout (PO15).** Cargo workspace at repo root: `Cargo.toml` (workspace),
  `crates/wait-on-core` (pure Rust engine, no napi), `crates/wait-on-napi` (napi-rs
  binding, `cdylib`). The npm package stays where it is (`lib/`, `bin/`). Prebuilt addons
  land in `prebuilds/<platform>-<arch>[-musl]/wait-on.node`, loaded by a small hand-written
  loader in `lib/` (lightningcss style) — no new runtime dependency (`node-gyp-build` is
  not added).
- **KD-S5 Toolchain (PO21).** `rust-toolchain.toml` pins a stable toolchain; MSRV equals
  that pin. `Cargo.lock` is committed. `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test`, and `cargo deny check` gate CI (R23).
- **KD-S6 HTTP stack (PO9).** `reqwest` + `rustls` (no OpenSSL), `tokio` runtime. System
  trust store is not used; `ca` option and webpki roots mirror undici's defaults. Lane L5
  proves parity per option cell and records any deliberate difference in the guide.
- **KD-S7 CI owns no lane logic.** Spine lanes may not edit `.github/workflows/`. The
  bootstrap PR (L0) wires CI jobs that call `npm run --if-present <script>`; lanes change
  behavior by defining those scripts in `package.json`. Script hooks:
  - `ci:rs` — Rust gate on the host (fmt, clippy, test, deny, build addon for host, run
    mocha with `WAIT_ON_ENGINE=rust-strict`).
  - `build:napi` — build one target's addon into `prebuilds/` (`-- --target <triple>`).
  - `ci:rs:package` — with all targets' prebuilds downloaded: `npm pack`, install-matrix
    checks (scripts disabled, pnpm, `--no-optional`), size report.
- **KD-S8 Test releases.** Node keeps shipping through the existing channels
  (`latest` from `master`, `next`/rc from `next`, `*.x` maintenance) — the spike does not
  touch `.releaserc.json` or `release.yml`. Rust test builds ship as **GitHub prereleases
  on the fork**: every push to `spike-next-rs` whose package job succeeds publishes a
  prerelease `rs-<version>-<shortsha>` with the multi-platform `wait-on-*.tgz` attached
  and `SHA256SUMS`. Testers install with `npm i <tarball-url>` and set
  `WAIT_ON_ENGINE=rust`. npm publication from the fork (PO6a trusted publisher) is an
  operator step, not a lane.
- **KD-S9 Docs are part of done.** `docs/guides/` is the developer manual for the
  two-engine repo. L0 creates it; every lane updates the guide pages its change affects in
  the same PR. `AGENTS.md` links to it.

## Operator decisions (2026-10-01)

These supersede the 2026-09-30 key decisions where they conflict. Lessons behind them are in
`docs/solutions/best-practices/`.

- **KD-S10 Rust is primary; JS is what gets sunset.** Avoidable JavaScript moves to Rust. New
  dev/CI tooling is a `cargo xtask` subcommand with Rust tests, or a dependency-free POSIX sh
  script. No new JS tooling or JS devDependencies, no npm lifecycle scripts, no Justfile, and no
  Claude Code hooks for repo behaviour. `cargo xtask` is the single front door (L12 #75); the npm
  script names CI calls are one-line aliases to it.
- **KD-S11 Shipping-critical JS tools stay until a Rust equal is proven.** semantic-release (npm
  publish, GitHub releases) and commitlint in CI stay. At cutover, evaluate knope / release-plz /
  git-cliff for releases and crate-ci/committed for commit linting, and replace only if equally
  good. Inventory: #85.
- **KD-S12 Rust-first tests, 100% bar.** Engine behaviour is specified by Rust tests
  (`crates/wait-on-core/tests/`, `tokio::time::pause` for timing). The Rust crates are held to 100%
  line + region coverage (`cargo llvm-cov`) and 100% compatibility with the JS engine: every
  API/CLI/conformance/property test passes under `WAIT_ON_ENGINE=rust-strict` and the pending list
  is empty. JS tests remain only as front-door parity (L13 #76). L7 #59 was steered to this.
- **KD-S13 Parallel lanes, cap 3.** Independent lanes run in parallel; dependent ones wait (L5
  after L4, L7 after L5, L13 after L7, L10 last). Merges stay one at a time behind green checks and
  the pre-merge checklist, and a post-merge red halts the queue. Lane merges of the base use
  `chore(merge): merge spike-next-rs into <branch>`.
- **KD-S14 CI budget.** Pull requests run one representative row per job plus `rust` on Windows
  (#77, #83), with `fail-fast: false`; pushes run the full matrix and the fork prerelease. L10
  restores the full PR matrix before #51 leaves draft.
- **KD-S15 Commit messages are validated before push.** A POSIX sh `.githooks/commit-msg` hook
  mirrors the CI commitlint rules (#82); AGENTS.md documents them. Already-pushed spike commits
  that fail are listed in `commitlint.config.js` `ignores`.

Lanes added: L11 #68 (cargo-vet), L12 #75 (cargo xtask), L13 #76 (Rust-first tests), all before
L10.

## Lanes

Every lane: base `spike-next-rs`, kind `preview`, strict TDD per `AGENTS.md`, guides
updated, `Closes #<sub>`. Allowed paths (all lanes unless narrowed): `crates/`,
`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `lib/`, `bin/`,
`test/`, `index.d.ts`, `package.json`, `package-lock.json`, `.npmignore`, `.gitignore`,
`.nycrc.json`, `eslint.config.mjs`, `docs/guides/`, `docs/plans/`, `docs/solutions/`,
`AGENTS.md`, `README.md`.

| Lane | Title | Proves | Done when |
|---|---|---|---|
| L0 | bootstrap: CI hooks, MWPM config, guides skeleton | — | operator PR (not a spine lane; touches workflows) |
| L1 | Cargo workspace, napi addon, engine switch, dual-driver test run | PO1 (bridge), PO13 (dual driver), PO21 | `WAIT_ON_ENGINE=rust` loads the addon; addon exposes version + a no-op; every test runs under both engines with an explicit pending list; `ci:rs` green |
| L2 | `tcp:` and `socket:` checks in Rust | PO1 | tcp/socket tests (incl. IPv6, reverse, `tcpTimeout`) pass under Rust |
| L3 | `file:` check in Rust | PO10 | file tests incl. `window` stabilization and Windows delete-pending pass under Rust |
| L4 | `http(s)`/`http(s)-get` checks + `validateStatus` over threadsafe callback | PO2 | HTTP tests incl. headers, auth, `httpTimeout`, `followRedirect`, `validateStatus` pass; per-check FFI overhead measured and recorded |
| L5 | TLS, proxy, http-over-unix and named pipes | PO9 | `ca/cert/key/passphrase/strictSSL`, `proxy` + env proxies, `http://unix:` (and Windows pipes) pass; parity deltas documented |
| L6 | `command:` resource in Rust | R1 | command tests incl. `commandTimeout`, reverse pass |
| L7 | polling loop in Rust (delay, interval, window, simultaneous, timeout, reverse, log/verbose output) | R1, R5, PO16 | JS shim makes one napi call per `waitOn`; Rust path loads no `rxjs`/`undici`; pending list empty for api tests |
| L8 | parser differential, timing tolerance, startup benchmark | R17, R18, PO13, PO22 | Rust parsers exposed and fed the #245 property vectors; timing tolerance fixed and enforced; startup benchmark recorded with regression threshold |
| L9 | multi-target prebuilds, loader, install matrix, read-only run | PO4, PO5, PO7, PO8 | `build:napi` covers the PO4 matrix; `ci:rs:package` packs all targets, installs with scripts disabled under npm and pnpm and `--no-optional`, runs in a read-only container; size recorded |
| L10 | test-release readiness and supply-chain delta | R14, Success Criteria | prerelease tarball verified end to end from the fork GitHub Release; dependency and lifecycle-script delta vs `joi`/`rxjs`/`undici` recorded; pending list empty; spike PR ready to leave draft |

PO4 target matrix (L9): `darwin-arm64`, `darwin-x64`, `linux-x64-gnu`, `linux-arm64-gnu`,
`linux-x64-musl`, `linux-arm64-musl`, `win32-x64`, `win32-arm64`. `linux-armv7` is out
unless demand appears (JS fallback still covers it).

Deferred (not in this spike): standalone binary (R10–R12, PO17–PO19), npm publish from the
fork (PO6a), cutover flip of the default engine (R22), removing the JS fallback.

## Operator steps

- L0 merge (workflow edits are never a spine lane).
- Any later CI change a lane discovers it needs is filed as an operator PR, not smuggled
  into a lane.
- npm trusted-publisher registration if/when Rust builds should publish to npm.
