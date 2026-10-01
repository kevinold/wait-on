---
title: Local commit message validation - Plan
type: chore
date: 2026-09-30
closes: kevinold/wait-on#82
branch: chore-82-commit-msg-hook
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# Local commit message validation - Plan

## Goal Capsule

- **Objective.** A contributor who writes a commit message that CI's commitlint would reject learns that at `git commit` time, with the rule name, instead of after push; and spike PR #51 (`next..spike-next-rs`) lints green without rewriting the protected branch.
- **Means.** A tracked, dependency-free POSIX sh `commit-msg` hook that mirrors `@commitlint/config-conventional` (KTD1, KTD2), enabled by `cargo xtask hooks` (KTD4), with the five already-pushed failing spike commits added to the CI-only `ignores` in `commitlint.config.js` (KTD5).
- **Authority.** `AGENTS.md` > this plan's Resume notes (operator steers of 2026-09-30) > the rest of this plan > issue #82 text.
- **Stop conditions** (report, do not work around): a `.github/workflows/` change is needed; a `package.json` dependency, script, or lifecycle hook is needed; `git hook run` cannot drive the hook on a CI platform (A1, A2).
- **Execution profile.** `/ce-work` from this plan, strict TDD per `AGENTS.md`; one PR into `spike-next-rs` on `kevinold/wait-on`, body `Closes #82`. Every commit on the branch must pass both the new hook and CI commitlint.

## Product Contract

### Summary

Add `.githooks/commit-msg`, a POSIX sh script with no dependencies that rejects a commit message violating the Conventional Commits rules CI enforces, printing each violation with its commitlint rule name. Add `cargo xtask hooks`, which points `core.hooksPath` at `.githooks`. Add the five failing spike commits to the `ignores` in `commitlint.config.js` so CI passes on `next..spike-next-rs`. Document the commit rules in a new `AGENTS.md` "Commit messages" section, linked from the contributor guides.

### Problem Frame

Nothing checks commit messages before push, so commitlint fails in CI after the fact (issue #82 lists `subject-case`, `header-max-length`, and `body-max-line-length` failures). Five commits already on the protected `spike-next-rs` branch fail the `next..spike-next-rs` range check, so PR #51 stays red regardless of new commits. The operator rules out husky, `@commitlint/*` devDependencies, and any npm script or lifecycle hook for the local gate (Resume notes).

### Requirements

**Local hook**

- R1. `.githooks/commit-msg` is a tracked, executable (mode 100755) POSIX sh script with no dependencies beyond sh and POSIX utilities, always checked out with LF line endings, so it runs under git-for-Windows' bundled sh as well as Linux and macOS sh.
- R2. The hook reads the message file git passes as its argument, ignores everything from the scissors line (`------------------------ >8 ------------------------`) on, strips trailing `\r` from every line, and drops leading and trailing blank lines before applying rules. Lines starting with `#` are skipped for header extraction and `body-leading-blank` but still count for `body-max-line-length`, because `git commit -m`/`-F` keep them in the commit CI lints (KTD2).
- R3. The hook accepts without further checks exactly the headers `@commitlint/is-ignored` ignores by default in 21.2.3: `Merge pull request`, `Merge <x> into <y>`, `Merge branch <x>`, `Merge tag <x>`, `Merge remote-tracking branch`, `Revert `/`revert `, `Reapply `/`reapply `, `amend!`/`fixup!`/`squash!`, `Merged <x> in|into <y>`, `Merged PR <n>: <x>`, `Automatic merge`, `Auto-merged <x> into <y>`. A bare `Merged ...` header is not ignored.
- R4. The hook applies these rules to every other message, prints every violation it finds (one line each, naming the commitlint rule) and exits 1 when any fired, else exits 0 silently:
  - `type-enum`, `type-case`, `type-empty`: the header is `<type>[(<scope>)][!]: <subject>` and `<type>` is exactly one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`, lowercase.
  - `subject-empty`: the subject after `: ` is non-empty.
  - `subject-case`: the subject's first byte is neither an uppercase letter A-Z nor a non-ASCII byte (KTD2).
  - `subject-full-stop`: the subject does not end with `.`.
  - `header-max-length`: the header is at most 100 characters.
  - `header-trim`: the header has no leading or trailing whitespace.
  - `body-leading-blank`: when any line follows the header, the line right after the header is blank.
  - `body-max-line-length`: every line after the header (body and footer) is at most 100 characters.
- R5. The hook may be stricter than CI but never looser: any message the hook accepts passes CI commitlint for the rules in R4. Known stricter deltas are owned by KTD2.
- R6. The hook does not read `commitlint.config.js` and does not honor its `ignores`; those serve CI's range lint over historic commits, which are never re-hooked.

**Enabling the hook**

- R7. `cargo xtask hooks` sets `core.hooksPath` to `.githooks` for the repository it is run in and prints what it set; it is listed in `cargo xtask --help` like every other subcommand. Nothing in `package.json` changes: no dependency, script, or lifecycle hook.
- R8. `.githooks/` is excluded from the published package (`npm pack --dry-run` does not list it) and `.npmignore` names it, per the AGENTS.md hygiene convention.

**CI range lint**

- R9. `commitlint.config.js` ignores exactly the five commits that fail `commitlint --from origin/next --to origin/spike-next-rs` today (cccba80, 1649323, 9347b8b, b171574, 3688229), in a block with the same removal comment shape as `inFlightSubjects`, so that range lints green; a fresh commit that reuses one of those headers with a different body is still linted (KTD5).
- R10. The stale "CI only (no husky)" header comment in `commitlint.config.js` states that CI runs commitlint and the local gate is `.githooks/commit-msg`.

**Documentation**

- R11. `AGENTS.md` gains a "Commit messages" section stating the rules in R3-R4 with good and bad examples drawn from issue #82, the parallel-lane merge message form (`chore(merge): merge spike-next-rs into <branch>`), how to enable the hook (`cargo xtask hooks`, or `git config core.hooksPath .githooks`), and that `--no-verify` is never used; the stale Conventions bullet ("semantic-release + commitlint are proposed in #241") becomes a link to the section.
- R12. `docs/guides/contributing-dual-engine.md` links the section from its "Workflow for a change" list; `docs/guides/development.md` adds the enable step to Setup and `hooks` to the xtask subcommand list; `docs/guides/testing.md` names the hook tests in the `xtask` row.

### Key Decisions

- **Local validation is a dependency-free sh hook, not commitlint run locally.** (session-settled: user-directed — chosen over a tracked hook running npx commitlint with pinned devDependencies: operator steer; no install step, nothing downloaded, works in every worktree) Governs R1, R4, R5, R6.
- **No npm script or lifecycle hook; `cargo xtask hooks` is the enable command.** (session-settled: user-directed — chosen over an npm hooks:install script or postinstall: operator steer; new tooling is Rust or POSIX sh) Governs R7.
- **CI keeps running commitlint; historic spike commits are ignored in config, not rewritten.** (session-settled: user-directed — chosen over rewriting spike-next-rs: the branch is protected) Governs R6, R9.
- **Hook strictness is one-directional.** The hook may reject what CI accepts, never the reverse, so a local pass predicts a CI pass. Governs R5.

### Scope Boundaries

- `package.json`, `package-lock.json`, and `.github/workflows/` are untouched.
- No pre-push hook and no PR-title check; CI's pr-title workflow stays the only title gate.
- The hook does not auto-enable; a developer runs `cargo xtask hooks` once per clone.
- No JS test for `commitlint.config.js`: JS tests live only at the API/CLI front doors (operator standing rule); the config's RED/GREEN is the range lint command (U3).

## Planning Contract

- KTD1. **Hook shape.** One sh script, `#!/bin/sh`, `set -u`, no bash-isms, no external binaries beyond `sed`/`grep`/`tr`-class POSIX utilities. It normalizes the file per R2, extracts the header as the first remaining line, and runs each R4 check as an independent test that appends to a violations list; it prints the list to stderr and exits 1 when non-empty. Ignored messages (R3) exit 0 before any rule runs.
- KTD2. **Deltas from commitlint, all stricter.** `body-leading-blank` is an error here but a warning in `config-conventional`. `subject-case` rejects a first byte that is A-Z or any non-ASCII byte: commitlint rejects non-ASCII capitals (`fix: Éclair`), and a byte test cannot tell `É` from `é` without locale support, so all non-ASCII starts are rejected. `#` lines still count for `body-max-line-length`; git's own template comment lines are short, so editor commits are unaffected. Line length is `${#line}` in the shell's locale, which may count bytes for non-ASCII and so rejects earlier than CI. Each delta is marked in the script with a `ponytail:` comment naming the upgrade path. Cites R5.
- KTD3. **Tests are Rust integration tests in the xtask crate, driven through git.** (session-settled: user-directed — chosen over a mocha test: operator steer; the hook belongs to the Rust tooling, and git's `hook run` uses git's own sh on Windows) `xtask/tests/commit_msg_hook.rs` writes each table message to an absolute temp file and spawns `git -c core.hooksPath=.githooks hook run commit-msg -- <file>` with cwd at the repo root, asserting exit status and that stderr contains each expected rule name (and nothing for good messages). When `git` is missing or `git hook run` is not a git command (git < 2.36), the test prints why and returns early rather than failing. Cites R1-R6, A1-A3.
- KTD4. **`cargo xtask hooks` mechanism.** New `xtask/src/hooks.rs` with `run(&[String]) -> i32`, registered in `COMMANDS` in `xtask/src/main.rs` after `bench-startup`. It runs `git config core.hooksPath .githooks` through `host::run` with the process's current directory as cwd (A4), so git writes the config of whatever repository the developer is in and the test can target a throwaway `git init` repository instead of this checkout's config. A relative `core.hooksPath` resolves against the worktree root at hook time, so one setting serves every worktree. Cites R7.
- KTD5. **Ignore shape for the five spike commits.** All five are matched by their exact full message (header, blank line, body) compared against the trimmed message, not by header like `inFlightSubjects`: cccba80's header is itself valid and only its body fails, so a header match would let a reused header with a bad body through, which R9 forbids. (session-settled: user-directed — chosen over an exact-header ignore: a header-only ignore would silently skip every future commit with that header) Cites R9.
- KTD6. **Range verification without devDependencies.** The CI-equivalent check runs through `npx -y -p @commitlint/cli@21.2.3 -p @commitlint/config-conventional@21.2.3 commitlint` (Verification Contract); nothing is installed into the repo. The same command with a piped message proves the KTD5 negative case once by hand.

### Assumptions

- A1. Developer hosts and CI runners have git 2.36 or newer, so `git hook run` exists (local git is 2.47.0); older git makes the hook tests skip, not fail.
- A2. git-for-Windows runs hooks through its bundled sh and treats the tracked 100755 hook as executable, so the hook and its tests work in the `rust` CI job on windows without `sh` on `PATH`. Windows checkouts use `core.autocrlf`, so the hook needs `.githooks/.gitattributes` with `* text eol=lf` (precedent: `supply-chain/.gitattributes`, see `docs/solutions/best-practices/cargo-vet-store-setup-and-maintenance.md`).
- A3. `git hook run` honors `-c core.hooksPath=.githooks` and changes directory to the worktree root before running the hook, so an absolute message path is required in tests.
- A4. `cargo run` (hence `cargo xtask`) executes the binary in the invoking working directory.
- A5. The `rust` CI job runs `cargo test --workspace` (through `cargo xtask ci`). On pull requests it runs on ubuntu only; macOS and windows run on the push to `spike-next-rs` after merge. The Windows proof (A2) is therefore post-merge, and a failure there is the A2 stop condition, fixed in an immediate follow-up.

## Implementation Units

### U1. Commit-msg hook with its Rust test table

- **Goal.** `.githooks/commit-msg` rejects bad messages with rule names and accepts good ones, proven on every CI platform (windows and macOS after merge, A5).
- **Requirements.** R1-R6, R8.
- **Dependencies.** None.
- **Files.** `.githooks/commit-msg` (new, 100755), `.githooks/.gitattributes` (new, `* text eol=lf`), `xtask/tests/commit_msg_hook.rs` (new), `.npmignore`.
- **Approach.** Write the test table first (KTD3): a `const` slice of `(name, message, expected_rules)` rows plus a helper that writes the file, spawns git, and asserts. Run it and observe failures on the exit-status and rule-name assertions because the hook is absent. Then write the script per KTD1, one rule per increment, re-running the table after each. Add `.githooks` to `.npmignore`; `npm pack --dry-run` lists no `.githooks` entry before or after (the `files` allowlist already excludes it), so this line is hygiene with no RED, a recorded carve-out.
- **Execution note.** The script must stay sh-portable; no bash arrays or `[[`. Mark each KTD2 delta with a `ponytail:` comment.
- **Patterns to follow.** `xtask/tests/cli.rs` for spawning and asserting on `Output`, and its temp-file naming with the pid.
- **Test scenarios.**
  - Rejected with `subject-case`: `docs(plans): L12 xtask + Justfile lane plan` and `docs(plans): L6 command implementation plan (#58)`.
  - Rejected with `header-max-length`: the 113-character `feat(rust): verified TLS roots, client identity, explicit proxy and unix/pipe transport in the http checker (#57)`.
  - Rejected with `body-max-line-length`: the `fix(xtask): never rebuild the running xtask binary on Windows (#75)` message with a pasted log line over 100 characters, the 208-character one-line body of `fix(review): apply review findings`, and a `-m` style body whose `#82 ...` line is over 100 characters.
  - Rejected with `subject-case`: `fix: Éclair` (non-ASCII capital).
  - Rejected with `type-enum`: `improve: use parseArgs` and `Merged stuff` (not a default-ignored header); with `type-case`: `Fix: thing`; with `subject-empty`: `fix:` and `fix: `; with `subject-full-stop`: `fix: thing.`; with `header-trim`: a header with a trailing space; with `body-leading-blank`: body on the line right after the header.
  - Two violations in one message print both rule names.
  - Accepted (exit 0, empty stderr): `chore(merge): merge spike-next-rs into chore-82-commit-msg-hook`; `feat(scope)!: breaking change` with a body and a `BREAKING CHANGE:` footer under 100 characters; a subject starting with a digit; a message with short `#` comment lines and a long diff after the scissors line; the same good message with CRLF line endings; headers `Merge branch 'x' into y`, `Merge pull request #1 from a/b`, `Merge tag 'v1'`, `Revert "feat: x"`, `Reapply "feat: x"`, `fixup! feat: x`, `Automatic merge from x`.
  - Skips with a printed reason when `git hook run` is unavailable.
- **Verification.** `cargo test -p xtask` green on the host; `npm pack --dry-run` lists no `.githooks` entry.

### U2. `cargo xtask hooks`

- **Goal.** One command enables the hook in any clone or worktree.
- **Requirements.** R7.
- **Dependencies.** None.
- **Files.** `xtask/src/hooks.rs` (new), `xtask/src/main.rs`, `xtask/tests/cli.rs`.
- **Approach.** Grow `SUBCOMMANDS` in `xtask/tests/cli.rs` with `hooks` and add a test that `git init`s a temp repository, spawns xtask `hooks` with that directory as cwd, and reads back `git config --get core.hooksPath` there; run and observe the usage-list and unknown-subcommand failures. Then add the module per KTD4 and the `COMMANDS` entry. The test skips with a printed reason when `git` is absent.
- **Execution note.** Never run the subcommand against this checkout from a test; the cwd-targeted temp repository is the point of KTD4.
- **Patterns to follow.** `ci::fmt` for a one-command subcommand; `host::run` for spawning; the existing unknown-subcommand test for usage assertions.
- **Test scenarios.**
  - Usage and `--help` list `hooks` on its own line.
  - `hooks` in a fresh temp repository sets `core.hooksPath` to `.githooks` and exits 0.
  - `hooks` outside any repository exits non-zero (git's own error propagates).
- **Verification.** `cargo test -p xtask`; `cargo xtask fmt` and `cargo xtask lint` clean.

### U3. Ignore the five pushed spike commits in CI commitlint

- **Goal.** The `next..spike-next-rs` range lints green without weakening future lint.
- **Requirements.** R9, R10.
- **Dependencies.** None.
- **Files.** `commitlint.config.js`.
- **Approach.** RED is the KTD6 range command observed failing on exactly the five commits. Add the spike block per KTD5 with a removal comment naming PR #51 and the merge of `spike-next-rs` into `next` as the removal trigger, and rewrite the header comment per R10. GREEN is the same command exiting 0.
- **Execution note.** Copy the five full messages from `git log` verbatim (the `fix(review)` body is one 208-character line); the full-message match compares trimmed text. Re-run the range command right before merge: another lane may land a new failing commit on `spike-next-rs` first.
- **Patterns to follow.** The existing `inFlightSubjects` block and its KTD6 comment.
- **Test scenarios.** Test expectation: none -- CI config with no JS test per the front-door rule; proof is the range command plus one piped-message check that `fix(review): apply review findings` with a different over-long body is still rejected.
- **Verification.** The KTD6 range command exits 0; `npm test` still green (lint covers no new file).

### U4. Commit rules documentation

- **Goal.** A contributor or agent finds the rules, the examples, and the enable command in `AGENTS.md`, reachable from the guides.
- **Requirements.** R11, R12.
- **Dependencies.** U1-U3 (documents what shipped).
- **Files.** `AGENTS.md`, `docs/guides/contributing-dual-engine.md`, `docs/guides/development.md`, `docs/guides/testing.md`.
- **Approach.** Add `## Commit messages` to `AGENTS.md` after Conventions, stating R3-R4 once, a short good/bad table from issue #82, the merge-message form, enabling, and the `--no-verify` rule; replace the stale Conventions bullet with a link. Link the section from the contributing guide's workflow list; add the enable line to Setup in `development.md` and `hooks` to its xtask subcommand list; extend the `xtask` row in `testing.md` with the hook tests. Docs-only, exempt from TDD.
- **Patterns to follow.** `AGENTS.md` section tone (short bullets, cite over restate).
- **Test scenarios.** Test expectation: none -- docs only.
- **Verification.** Links resolve; `npm test` unaffected.

## Verification Contract

| Check | Command | Proves |
|---|---|---|
| Rust tooling tests | `cargo test -p xtask` | U1 hook table, U2 subcommand |
| Rust style | `cargo xtask fmt`, `cargo xtask lint` | U1, U2 |
| Node suite | `npm test` (lint + types + mocha) | nothing regressed |
| CI-equivalent range lint | `npx -y -p @commitlint/cli@21.2.3 -p @commitlint/config-conventional@21.2.3 commitlint --from origin/next --to origin/spike-next-rs` | R9 |
| Branch lint | same command with `--to HEAD` | this PR's own commits |
| Package hygiene | `npm pack --dry-run` | R8 |
| Hook end to end | `cargo xtask hooks` in a scratch clone, then a bad `git commit` is refused with the rule name | R7 observed once by hand |
| LF checkout | clone with `git -c core.autocrlf=true`; `.githooks/commit-msg` contains no `\r` | R1 on Windows |
| Post-merge platforms | the `spike-next-rs` push run's `rust` job on windows-latest and macos-latest ran the hook tests green | A2, A5 |

## Definition of Done

- All four units land with their tests in the same commits; `cargo test -p xtask` and `npm test` are green locally and in PR CI; the post-merge push run is green on windows and macOS (A5).
- The range lint exits 0 for `origin/next..origin/spike-next-rs` and for `origin/next..HEAD`.
- `cargo xtask --help` lists `hooks`; `.githooks/commit-msg` is tracked at mode 100755; `npm pack --dry-run` lists no `.githooks` entry; `package.json` is unchanged.
- `AGENTS.md` has the Commit messages section and no stale #241 bullet; the three guides link or mention it as R12 states.
- No `.github/workflows/` change, no new dependency, no `.only`/`.skip`, and no leftover experiments in the diff.

## Resume notes

- **2026-09-30, PM/operator steer** (user-directed). Supersedes issue #82 part 1 and the earlier settled decisions it rested on: the hook is no longer `npx --no -- commitlint --edit "$1"`; `@commitlint/cli` and `@commitlint/config-conventional` are not added as devDependencies; there is no `hooks:install` npm script or any lifecycle hook; the hook test is not a mocha test. Replaced by: a dependency-free POSIX sh `.githooks/commit-msg` mirroring config-conventional (R1-R6), `cargo xtask hooks` as the enable command (R7), and Rust integration tests in `xtask/tests/` driven through `git hook run` (KTD3). Parts 2 (AGENTS.md section, R11-R12) and 3 (`commitlint.config.js` ignores, R9-R10) are kept; CI keeps running commitlint and workflows are not edited.
- **2026-09-30, operator standing rule: tooling.** New dev/CI tooling is Rust (`cargo xtask` subcommand with Rust tests) or a dependency-free POSIX sh script; never new JS tooling, JS devDependencies, npm lifecycle scripts, a Justfile, or Claude Code hooks. JS tests only at the API/CLI front doors. Consequence here: U3 has no mocha test; its proof is the range lint command.
- **2026-09-30, operator standing rule: knowledge.** Anything saved to agent memory also lands in the repo in the same PR, in `docs/solutions/` (via `/ce-compound mode:non-interactive`) or these Resume notes.
- Settled here, not in the steer: the hook does not honor `commitlint.config.js` ignores (R6); the generic `fix(review)` header is ignored by full message (KTD5); `cargo xtask hooks` targets the process cwd (KTD4).
