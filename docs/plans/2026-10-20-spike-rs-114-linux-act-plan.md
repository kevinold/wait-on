---
title: Run linux-only checks and ubuntu CI jobs locally with docker and act - Plan
type: build
date: 2026-10-20
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
issue: 114
---

# Run linux-only checks and ubuntu CI jobs locally with docker and act - Plan

kevinold/wait-on#114 on the spike (`spike-next-rs`). Two `cargo xtask` subcommands so a
macOS developer can run what today only runs in GitHub Actions: the Linux-only dependent
(jest-dev-server) and the ubuntu rows of `.github/workflows/*.yml`.

## Goal Capsule

- **Objective:** a failure that today surfaces only after a push (jest-dev-server under
  `--include-optional`, a RustSec advisory in the ubuntu `rust` job) shows up on a
  developer's Mac before the push.
- **Means:** `cargo xtask linux <cmd...>` runs a command in a pinned Linux Node + Rust
  container over the checkout (KTD1-KTD3); `cargo xtask act` wraps nektos/act with a pinned
  runner image and fixed architecture (KTD4). Both skip with a message when no Docker
  engine is reachable (KTD5).
- **Authority:** kevinold/wait-on#114.
- **Stop conditions:** an edit under `.github/workflows/`; new JS tooling, npm script, or
  Claude hook; a `lib/`, `bin/`, `crates/` or `index.d.ts` change.
- **Allowed paths:** `xtask/`, `mise.toml`, `.npmignore`, `AGENTS.md`, `docs/guides/`,
  `docs/plans/`, `docs/solutions/`, `CONCEPTS.md`.

## Product Contract

### Problem Frame

jest-dev-server is an optional, `os: ["linux"]` dependent, so `cargo xtask dependents
--include-optional` on macOS silently skips it; it has never been run. Ubuntu CI jobs run
only in GHA, and lanes have hit push-only failures (L17 timing, RUSTSEC-2026-0346/0347 on
rustls 0.23.45 failing `cargo deny check`, fixed by #113) after green local runs. OrbStack
(Docker) and act are already on the dev machine.

### Requirements

- R1. `cargo xtask linux <cmd> [args...]` runs `<cmd>` inside a pinned Linux image, after
  `npm ci --engine-strict` and `npm run build:napi` (so `npm run dependents` packs a tarball
  carrying the Linux prebuild). Exit code is the command's.
- R2. The checkout is mounted **read-only** at `/src` and copied into the container's own
  `/work` (excluding `.git`, `.claude`, `node_modules`, `target`, `prebuilds`), so Linux
  `node_modules`, build output and prebuilds never land in the host tree. Cargo, rustup,
  `target` and the npm cache live in named volumes.
- R3. `cargo xtask linux npm run dependents -- --include-optional` on macOS runs
  start-server-and-test's rows (the acceptance demo: a dependent that actually runs) and
  reaches jest-dev-server and prints its row. That row is `pre-existing` (FAIL on baseline
  and candidate): the jest-puppeteer root jest config uses `projects:
  ["<rootDir>/packages/*"]` and `packages/jest-dev-server` has no tests ("No tests found",
  exit 1 at main, v10.1.4 and v9.0.2). The manifest is **not** changed in this lane;
  recorded as a follow-up.
- R3a. `scripts: true` is honored: npm is pinned in the image to 11.16.0 (the recorded-run
  version, before npm 11.19's `allowScripts` gate blocked @swc/core, nx and puppeteer
  install scripts).
- R3b. localhost/IPv6: start-server-and-test `demo-json-server` and `demo-ip6` failed on
  both published wait-on and the candidate in a trial container (servers on `::1`, probe
  dials `127.0.0.1`). Cause (verified 2026-10-20, node:24-trixie amd64 on OrbStack): on
  the default bridge network the container has no non-loopback IPv6 address, so
  `getaddrinfo` with `AI_ADDRCONFIG` (Node's http/undici lookup) drops `::1` and
  `localhost` resolves to `127.0.0.1` only. `--network host` (the OrbStack VM's stack,
  which has IPv6) returns `::1, 127.0.0.1`. The wrapper runs with `--network host`
  (KTD3b); if a run still shows those rows failing on baseline, the testing guide marks
  them expected `pre-existing` in containers.
- R4. `cargo xtask act [--event pull_request|push] [--job J] [--workflow W] [-- act args]`
  runs the ubuntu jobs of a workflow (default `node.js.yml`, `pull_request`) with a pinned
  runner image for `ubuntu-latest`/`ubuntu-24.04` and `--container-architecture
  linux/amd64`.
- R5. `cargo xtask act --job rust -- -C <scratch>` (this branch's xtask, act's
  `--directory` pointed at a scratch worktree of the pre-#113 commit, whose own `xtask/`
  has no `act` subcommand) fails at `cargo deny check` on rustls 0.23.45 the way GHA run
  39067121354 did. Fallback when act cannot read a worktree: a plain `git clone` at that
  commit. The same job on this branch passes its deny step.
- R6. No reachable Docker engine: both subcommands print a skip message naming the reason
  and exit 0. `act` missing from PATH: `cargo xtask act` fails with a hint to `mise install`.
- R7. act pinned in `mise.toml` as `"aqua:nektos/act"`; AGENTS.md Commands entries;
  `docs/guides/testing.md` section on when to run each before pushing and what act cannot
  cover; any new top-level file in `.npmignore`.

### Scope Boundaries

- No `.github/workflows/` edits; act reads them as is.
- The v10 part of #114 (jeffbski/wait-on `next`, npm-script tooling) is a separate upstream
  PR after jeffbski/wait-on#266: follow-up only, recorded in the PR body and testing guide.
- Not in act's reach (documented, not worked around): `windows-latest`, `macos-latest`,
  `windows-11-arm`, `ubuntu-24.04-arm` matrix rows; the `prerelease`/`release` jobs (OIDC,
  `contents: write`, repo secrets); `package` (push-only, needs every target's artifact).
- No consumer-visible change, so no `features/*.feature` scenario.
- jest-dev-server manifest entry fix (it can never pass as pinned): follow-up, not here.

### Assumptions

- Containers (act and `linux`) run as root, unlike GHA's `runner` user. Root never takes
  the denied-stat path that `crates/wait-on-core/src/lib.rs`
  `file_size_is_minus_one_when_stat_is_denied` tolerates, so the 100% region gate in
  `ci:rs` can fail under act and not on GHA: a known local false failure, documented.

- `catthehacker/ubuntu:rust-24.04` (amd64 + arm64 published) carries node and rustup, so the
  `rust` job's `rustup toolchain install` works without the 20 GB `full` image.
- Running containers as `linux/amd64` under OrbStack's Rosetta is fast enough for a full
  `ci:rs` and matches GHA's `ubuntu-latest` arch; Puppeteer ships no linux-arm64 Chrome, so
  amd64 is also what jest-dev-server's install needs.

## Planning Contract

### Key Technical Decisions

- KTD1. **One generic `linux` subcommand, dependents as its first caller**, not a
  `dependents --container` flag: the issue asks for "a given command"; the dependents
  harness stays unchanged. (session-settled: user-directed — chosen over new JS tooling or
  npm scripts: spike Rust-first tooling rule.)
- KTD2. **Image built from an inline Dockerfile** (`FROM node:24-trixie` + rustup with
  `--default-toolchain none`), tagged `wait-on-linux`, the pattern `package.rs` already uses
  for AE1 cells. trixie because the gnu addon needs GLIBC 2.39+ (same note as
  `package::container_cells`). `rust-toolchain.toml` picks the toolchain at first `cargo`,
  cached in the `RUSTUP_HOME` volume.
- KTD3. **Setup then exec without a shell-quoting layer:** `sh -c '<copy> && npm ci
  --engine-strict && npm run build:napi && exec "$@"' sh <cmd...>` so arguments pass
  verbatim. Copy over a read-only mount (not volumes shadowing host paths) per operator
  input: nothing the container writes can reach the host tree.
- KTD3a. **Pin npm rather than manage `allowScripts`:** one `npm i -g npm@11.16.0` line in
  the image; managing per-dependent allowScripts would edit clones' config. Upgrade when
  the harness learns allowScripts.
- KTD3b. **`--network host`** over an IPv6-enabled user network: one flag, no network to
  create or clean up, and it keeps clone/npm network access. Ceiling: a port bound inside
  the container is bound on the engine VM too.
- KTD4. **act wrapper is argument construction only:** `act <event> -W
  .github/workflows/<workflow> -P ubuntu-latest=<img> -P ubuntu-24.04=<img>
  --container-architecture linux/amd64 --artifact-server-path <tmp> [-j job]
  [passthrough...]`. The artifact server is always on because mapping `ubuntu-24.04` runs
  the PR `napi` row, whose `upload-artifact` step needs it. No `.actrc` (no new top-level
  file); act itself skips rows whose `runs-on` has no `-P` mapping. `--event push` without
  `--job` also runs `package`, which fails at `download-artifact` (it needs every target's
  prebuild): documented, use `--job`.
- KTD5. **Engine probe is `docker info`** (exit 0 = reachable), not `docker --version`
  (which only proves the CLI). Skip is exit 0: these are local-only tools; CI never calls
  them, unlike `package`'s AE1 cells.
- KTD6. act pinned via mise (session-settled: user-directed — chosen over brew/unpinned act:
  reproducible version).

## Implementation Units

### U1. `linux` subcommand (test-first)

Files: `xtask/src/linux.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`.
Tests first, in `linux.rs` `#[cfg(test)]`:
- `dockerfile()` starts `FROM node:24-trixie`, installs rustup with
  `--default-toolchain none`, sets `CARGO_HOME`/`RUSTUP_HOME`/`PATH`.
- `build_args()` is `build --platform linux/amd64 -t wait-on-linux -` (Dockerfile on stdin).
- `dockerfile()` also pins `npm@11.16.0` (R3a). `CARGO_HOME` stays in the image (its
  `bin/` holds the rustup proxies); only `CARGO_HOME/registry` and `RUSTUP_HOME` are volumes,
  so an image rebuild is never shadowed by a stale volume copy.
- `run_args(root, cmd)` carries `--rm`, `--platform linux/amd64`, `-v <root>:/src:ro`, named
  volumes for `/work/target`, the cargo and rustup homes and the npm cache, `--network host` (R3b), then the image, `sh -c <setup>` and the command words verbatim (one
  with a space survives as one arg). `<setup>` copies `/src` to `/work` with the R2
  excludes using `tar -C /src --exclude=... -cf - . | tar -C /work -xf -` (no rsync in the
  image), then `npm ci --engine-strict && npm run build:napi && exec "$@"`.
- `run([])` (no command) is a usage error.
- `skip_reason(false)` names docker; `skip_reason(true)` is `None`.
CLI test: `xtask linux true` with `PATH` set to an empty dir exits 0 and prints the skip
message (no Docker needed). Add `linux` to `SUBCOMMANDS`.

### U2. `act` subcommand (test-first)

Files: `xtask/src/act.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`.
First a smoke step: `act -l` from this worktree (act reads the repo with go-git; if a
worktree `.git` file breaks it, the docs name a plain clone as the workaround).
Tests first: `parse_args` defaults (`pull_request`, `node.js.yml`, no job), `--event push`,
`--job rust`, `--workflow commitlint.yml`, passthrough after `--`, unknown flag and bad
event errors; `act_args` order, both `-P` mappings, `--container-architecture
linux/amd64` and `--artifact-server-path`; CLI test: empty-PATH run exits 0 with the docker skip message. Add `act` to
`SUBCOMMANDS`.

### U3. Pin, hygiene, docs

`mise.toml` `"aqua:nektos/act" = "0.2.89"`; `.npmignore` (`xtask/` already excluded by
`files`; add nothing unless a top-level file appears); AGENTS.md Commands entries;
`docs/guides/testing.md` "Linux and act runs" section: when to run each, act limits, the
v10 follow-up; update the Dependents harness "not yet run" note with the recorded run.

## Verification Contract

- `cargo test -p xtask`, `cargo xtask fmt`, `cargo xtask lint`, `npm test`.
- Manual (OrbStack): R3 `cargo xtask linux npm run dependents -- --include-optional` shows
  start-server-and-test rows `ok` three ways (R3b rows fixed or documented) and a
  jest-dev-server row; host `git status` clean afterwards (R2); R5 `cargo xtask act --job rust` on a scratch worktree of the pre-#113
  commit fails at `cargo deny check` with RUSTSEC-2026-0346/0347 (R5 route); on this branch
  `cargo xtask act --job rust` passes its deny step and `cargo xtask act --job build`
  passes. Evidence goes in the PR body and testing guide.

## Definition of Done

U1-U3 committed with tests, both acceptance runs recorded, PR open against
`kevinold/wait-on` `spike-next-rs`, CI green.
