---
title: A dependency-free commit-msg hook must be checked differentially against real commitlint
date: 2026-09-30
category: best-practices
module: .githooks/commit-msg
problem_type: best_practice
component: infrastructure
severity: medium
applies_when:
  - "A POSIX sh+awk hook re-implements @commitlint/config-conventional rules without node_modules"
  - "CI runs real commitlint and the local hook must never be looser than CI"
  - "Adding a tracked shell script that Windows contributors will run"
related_components: [.githooks/.gitattributes, xtask/src/hooks.rs, xtask/tests/commit_msg_hook.rs, commitlint.config.js]
tags: [commitlint, git-hooks, commit-msg, differential-testing, posix-sh, awk, windows-autocrlf]
---

# A dependency-free commit-msg hook must be checked differentially against real commitlint

## Context

Issue #82. CI lints PR commits with commitlint 21.2.3 and `@commitlint/config-conventional`, and a failure blocks the PR. Contributors only learned of a bad message after pushing. The operator rule is that tooling is Rust or POSIX sh, with no JS devDependencies or npm scripts. That rules out husky and a local commitlint install. The result is `.githooks/commit-msg` (POSIX sh + awk), enabled by `cargo xtask hooks` (`xtask/src/hooks.rs`, which sets `core.hooksPath` to `.githooks`).

The hook reimplements commitlint's rules, so it can drift from commitlint. The invariant is that it may be stricter than CI and never looser. A hook that passes a message CI rejects is worse than no hook. This is the same oracle idea as [porting JS regex parsers with a differential oracle](porting-js-regex-parsers-to-rust-with-a-differential-oracle.md), applied to a linter.

## Guidance

1. **Differential-test the reimplementation against the real linter.** Pipe each sample message through the hook and through `npx -y -p @commitlint/cli@21.2.3 -p @commitlint/config-conventional@21.2.3 commitlint`. Print `hook=X commitlint=Y` and flag `hook=0 commitlint!=0` as LOOSER. Reading the commitlint rules did not find the bugs below. This script did, and it also disproved a doc-review claim that `Merge pull request #1 from a/b` fails CI (commitlint exits 0).

2. **The header is the first non-blank line, `#` lines included.** git's commit-msg hook sees the message before git's own cleanup. `git commit -m` and `-F` use cleanup=whitespace, so `#` lines are kept and CI lints them. Skipping leading `#` lines let `git commit -m '#123 thing' -m 'fix: x'` pass locally and fail CI. `#` lines also count for body line length.

3. **Make non-ASCII subjects fail, not pass.** commitlint rejects `fix: Éclair`, and an `[A-Z]` test misses it. Under `LC_ALL=C` awk sees bytes, so a byte test cannot tell `É` from `é`. The hook rejects any non-ASCII first byte. That is stricter than CI and is marked with a `ponytail:` comment. `LC_ALL=C` also keeps lengths and the case test independent of the locale and of the awk variant (BSD awk, mawk, gawk).

4. **Copy commitlint's default ignore list exactly, not generously.** commitlint ignores `Merged <x> in|into <y>` and `Merged PR n: x`, not every `Merged...`. The hook first ignored all of them, which was looser than CI. commitlint accepts `Merge pull request #1 from a/b`, `Merge tag 'v1'` and `Reapply "feat: x"`, and rejects `Merged stuff`.

5. **Pin the line endings of a tracked sh hook.** A hook checked out with CRLF on a Windows `core.autocrlf=true` clone breaks sh. `.githooks/.gitattributes` holds `* text eol=lf`, following the precedent in `supply-chain/.gitattributes`. Verified with `git -c core.autocrlf=true clone`: the hook had 0 CR bytes, while `package.json` had 79.

6. **Drive the hook through git in tests.** `xtask/tests/commit_msg_hook.rs` runs a table of messages with `git -c core.hooksPath=.githooks hook run commit-msg -- <absolute temp file>` from the repo root, so on Windows git runs the hook with its own bundled sh. The message path is absolute because git runs the hook from the worktree root. The test skips when `git hook run --ignore-missing no-such-hook` fails (git < 2.36). Each case asserts either exit 1 with the named rules, or exit 0 with empty stderr. The PR CI rust job runs on ubuntu only. The Windows and macOS runs happen on the post-merge push to `spike-next-rs`.

7. **Ignore already-pushed bad commits by their exact full message, not their header.** Failing commits already on the protected `spike-next-rs` branch cannot be rewritten. `commitlint.config.js` matches each full message, without trimming leading whitespace, so a new commit that reuses a generic header such as `fix(review): apply review findings` is still linted. The block is marked for removal once the branch merges. Other lanes keep landing commits there: one more failing commit arrived while #88 was open. Re-run the `origin/next..origin/spike-next-rs` range lint right before merging.

## Why This Matters

A local gate that disagrees with CI in the loose direction teaches contributors to trust a green hook, and then the PR fails. Each bug above was a real "hook passes, CI fails" case, found only by running both linters on the same input. When the hook is stricter, the cost is an occasional false reject with a visible rule name and a one-line fix. When it is looser, the cost is a red PR on a protected branch. Keeping the hook in sh/awk and the tooling in Rust avoids new devDependencies.

## When to Apply

- Reimplementing any external linter or validator as a local check.
- Adding a tracked shell script that Windows contributors will run.
- Exempting history that cannot be rewritten from a linter: match the whole record, not a field the next commit can reuse.

## Examples

Differential harness shape, run per sample message:

```sh
printf '%b' "$msg" > "$f"
sh .githooks/commit-msg "$f" 2>/dev/null; h=$?
npx -y -p @commitlint/cli@21.2.3 -p @commitlint/config-conventional@21.2.3 commitlint < "$f" >/dev/null 2>&1; c=$?
echo "hook=$h commitlint=$c"; [ "$h" = 0 ] && [ "$c" != 0 ] && echo "LOOSER: $msg"
```

Samples that exposed bugs: `#123 thing\nfix: x` (leading `#`), `fix: Éclair` (non-ASCII capital), `Merged stuff` (over-broad ignore).

The user-facing rules and the strictness note are in the AGENTS.md "Commit messages" section.
