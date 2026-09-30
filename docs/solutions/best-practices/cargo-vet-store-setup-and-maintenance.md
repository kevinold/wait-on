---
title: Setting up and maintaining the cargo-vet store without overstating coverage
date: 2026-09-30
category: best-practices
module: supply-chain
problem_type: best_practice
component: rust-engine
severity: medium
applies_when:
  - Adding or removing an [imports.*] source in supply-chain/config.toml
  - Running cargo vet regenerate exemptions or cargo vet prune
  - Refreshing the supply-chain baseline counts in docs/guides/architecture.md
  - Changing the cargo vet invocation or its position in the ci:rs script
related_components: [package.json, docs/guides/development.md, docs/guides/architecture.md]
tags: [cargo-vet, supply-chain, rust, ci, audits, exemptions, spike-next-rs]
---

## Context

Lane L11 (#68) bootstrapped the `cargo vet` store (`supply-chain/`) for the Rust engine and gated `npm run ci:rs` on it (`package.json`: `"ci:rs": "cargo vet --locked && node scripts/ci-rs.js"`). It implements the cargo-vet leg of R23, which the rust-port plan added after a security review flagged that npm provenance attests the build, not its crate inputs (session history). The guides own the day-to-day workflow (`docs/guides/development.md`, "Vetting a new or bumped crate") and the baseline (`docs/guides/architecture.md`, "Supply chain"). This doc records the setup and maintenance steps that failed or misled on cargo-vet 0.10.2, and how each was confirmed.

## Guidance

1. **ZcashFoundation is a URL import, not a registry name.** `cargo vet import zcash-foundation` fails with `no peer named zcash-foundation found in the registry`. Use `cargo vet import zcash-foundation https://raw.githubusercontent.com/ZcashFoundation/zebra/main/supply-chain/audits.toml`. The registry's `zcash` entry is ECC's `zcash/rust-ecosystem`, a different organisation, so it does not satisfy a "ZcashFoundation" requirement.

2. **Count each source's audits before claiming coverage.** `cargo vet prune` drops imported audits the lockfile does not use, so a source can stay imported while covering nothing. After #68, ZcashFoundation's section in `supply-chain/imports.lock` is an empty `[audits.zcash-foundation.audits]` header. Count per source with `grep -o '^\[\[audits\.[a-z-]*' supply-chain/imports.lock | sort | uniq -c`. At #68: mozilla 24, bytecode-alliance 9, google 7, zcash 3, embark-studios 2, isrg 1, zcash-foundation 0. The first draft of the guide said every source removed exemptions, and code review caught it.

3. **Pick optional sources by trial.** For each candidate: import it, run `cargo vet regenerate exemptions`, count `[[exemptions` in `config.toml`, then restore `config.toml` and `imports.lock` from a backup. Keep a source only when the count drops. From 135 exemptions: isrg 134, embark-studios 133, zcash 132 (kept), ariel-os 135, fermyon 135 (dropped). The final store had 19 crates fully audited and 129 exempted.

4. **Write exemption `notes` last.** `regenerate exemptions` rewrites the whole exemptions table and drops every `notes` field, so notes written before the trials are lost. Derive each reason from `cargo vet suggest`:
   - `cargo vet diff <crate> <audited> <new>` means a bump past an audited base (63 crates at #68). Note it as "Newer than the imported audit of <audited>".
   - `cargo vet inspect <crate> <version>` means no audited base exists (66 crates).

5. **Keep the store format-stable across cargo-vet versions.** CI's `taiki-e/install-action` installed cargo-vet 0.10.0 while the store was written with 0.10.2. The two versions format multi-line `notes` that contain double quotes differently (0.10.0 escapes them as `\"`), so `cargo vet --locked` failed CI with "A file in the store is not correctly formatted" on `imports.lock`. Running `cargo vet fmt` with one version only moves the failure to the other version. The only such note came from Google's audits of `quote`. Fix (#73): `exclude = ["quote"]` under `[imports.google]`, `cargo vet` + `cargo vet prune` to drop those entries from `imports.lock`, and one hand-written `quote` exemption with a reason. Do not use `regenerate exemptions` for this, because it wipes the notes. Check both versions: `cargo vet --locked` and `cargo vet fmt` (no diff) under each. Install the older one with `cargo install cargo-vet --version 0.10.0 --locked --root <dir>` and run it with `<dir>/bin` first on `PATH`. Invoking the binary directly panics ("Cargo failed to set $CARGO"). After this, the store had 18 crates audited and 130 exempted. The store also has to be LF. The repo has no root `.gitattributes`, so the `windows-latest` checkout (autocrlf) turned every store file into CRLF, and `cargo vet --locked` rejected the whole file. `supply-chain/.gitattributes` with `* text eol=lf` fixes it. Check it by cloning with `git -c core.autocrlf=true` and counting `\r` in `supply-chain/`.

6. **Prove the gate goes red. A green run does not show it runs.**
   - Add a scratch unvetted crate, such as `leftpad = "0.2"` in `crates/wait-on-core/Cargo.toml`, and run `cargo fetch`. `cargo update -p leftpad` fails first because the lockfile does not have the crate yet.
   - `cargo vet --locked` exits 255 with `leftpad:0.2.0 missing ["safe-to-deploy"]`.
   - The old `ci:rs` printed `> cargo fmt --all --check` and never vetted. The new one exits 255 before any fmt line.
   - Revert `Cargo.toml` and `Cargo.lock`.

## Why This Matters

The gate is only as strong as the audits behind it. A source that covers zero crates, or a guide that says every source removed exemptions, gives reviewers a false picture of how much third-party code has been reviewed. Notes written too early disappear without warning, and the exemption policy in `development.md` depends on them. A gate never seen failing may not be running at all, because `npm run --if-present ci:rs` stays green whether or not the script vets.

## When to Apply

- Adding, removing, or re-evaluating an import source, or bootstrapping `cargo vet` in another workspace.
- Refreshing the baseline counts after a dependency lane lands (later lanes are expected to grow the crate count).
- Any `cargo vet regenerate exemptions` or `cargo vet prune` run: write notes afterwards.
- Changing where `cargo vet` sits in `ci:rs`.

## Examples

Trial loop, run as a script file:

```sh
#!/bin/sh
# usage: trial.sh isrg embark-studios zcash ariel-os fermyon
cp supply-chain/config.toml /tmp/cfg.bak; cp supply-chain/imports.lock /tmp/lock.bak
for src in "$@"; do
  cargo vet import "$src" >/dev/null 2>&1
  cargo vet regenerate exemptions >/dev/null 2>&1
  echo "$src $(grep -c '^\[\[exemptions' supply-chain/config.toml)"
  cp /tmp/cfg.bak supply-chain/config.toml; cp /tmp/lock.bak supply-chain/imports.lock
done
```

Checking that the gate fails:

```sh
# crates/wait-on-core/Cargo.toml: leftpad = "0.2"
cargo fetch
cargo vet --locked; echo $?   # 255, leftpad:0.2.0 missing ["safe-to-deploy"]
# then revert Cargo.toml and Cargo.lock
```
