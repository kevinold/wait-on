# Contributing to the two-engine repo

## Workflow for a change

1. Decide which engine(s) it touches. Behavior visible through `waitOn` or the CLI must end up identical on both; JS stays authoritative.
2. Tests first, per [AGENTS.md](../../AGENTS.md#test-driven-development-mandatory). The mocha suites are the shared contract; don't fork tests per engine.
3. Run the suites under both drivers (see [testing.md](testing.md#running-under-each-engine)).
4. Shrink the Rust pending list for anything the change makes pass on Rust; never grow it without a reason in the PR.
5. `npm test` green; `ci:rs` green once it exists.
6. Update the guides (checklist below) in the same PR.
7. Commit messages follow [AGENTS.md › Commit messages](../../AGENTS.md#commit-messages); `cargo xtask hooks` enables the local check.

## Spine lanes vs operator PRs

- **Spine lane (L1–L10):** one PR into `spike-next-rs`, one sub-issue of `kevinold/wait-on#35`, strict TDD, `Closes #<sub>`. Allowed paths are listed in the [spine plan](../plans/2026-09-30-1400-feat-spike-next-rs-spine-plan.md#lanes); `.github/workflows/` is never one of them.
- **Operator PR:** anything touching `.github/workflows/`, repo settings, or release configuration. A CI change a lane needs is filed as an operator PR, not folded into the lane. See [ci.md](ci.md#why-lanes-cannot-edit-workflows).
- Changing CI behavior from a lane means defining a hook script in `package.json` (`ci:rs`, `build:napi`, `ci:rs:package`; contract in [ci.md](ci.md#npm-script-hook-contract)). A lane that adds `ci:rs` must add `Cargo.toml` and `rust-toolchain.toml` in the same PR, since the toolchain steps are gated on those files.

## Docs-as-done checklist

A lane is not done until the pages it affects are current and every planned-status marker naming that lane is replaced:

- [ ] [architecture.md](architecture.md): layout, resource checks moved to Rust, loader, engine selection.
- [ ] [development.md](development.md): commands table rows, prerequisites, building and running each engine.
- [ ] [testing.md](testing.md): dual-engine runs, pending list, new suites.
- [ ] [ci.md](ci.md): hook scripts now defined, what they produce.
- [ ] [releasing.md](releasing.md): prerelease verification, install steps.
- [ ] [README.md](README.md): the model, if it changed.
- [ ] Any deliberate JS vs Rust behavior difference recorded where the behavior is described.
- [ ] Every page still states only what is true on merge.
