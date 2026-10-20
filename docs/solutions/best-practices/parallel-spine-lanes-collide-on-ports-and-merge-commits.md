---
title: Parallel spine lanes collide on fixed test ports and on default merge-commit messages
date: 2026-10-01
category: best-practices
module: test, spine workflow
problem_type: best_practice
component: process
severity: medium
applies_when:
  - More than one lane worktree runs `npm test` / `cargo xtask ci` on the same machine at once
  - A lane merges `origin/spike-next-rs` into its branch to clear a conflict
retire_when: "Every mocha suite listens on ephemeral ports (listen(0)) and commitlint/the pre-merge checklist accept default merge subjects"
tags: [spine, parallel-lanes, mocha, ports, commitlint, merge-commit, spike-next-rs]
---

# Parallel spine lanes collide on fixed test ports and merge-commit messages

## Symptom

- Two lane workers running the mocha suite at the same time fail with `EADDRINUSE` on 3000/3001/3011, then pass when re-run alone.
- A green lane PR is refused by the spine pre-merge checklist (`subject-prefix`) and fails commitlint because one commit is git's default `Merge remote-tracking branch 'origin/spike-next-rs' into ...`.
- After one lane merges, sibling PRs go `DIRTY` and must merge the base again (L8 #72 needed it twice).

## Root cause

- Several suites listen on fixed ports, so concurrent runs in different worktrees share them.
- The default merge subject has no conventional type, so both the checklist and commitlint reject it.

## Fix

- Run lanes in parallel only when they touch disjoint code; hold dependent lanes (L5 after L4, L7 after L5, L13 after L7).
- On a port collision, wait and re-run (or run only the failing file); never "fix" it in `lib/`.
- Merge the base only when GitHub reports a conflict, and always with a conventional message:
  `git merge origin/spike-next-rs -m "chore(merge): merge spike-next-rs into <branch>"`. Never rebase or force-push a lane branch.
- Merge lanes one at a time and expect siblings to need a re-merge afterwards.

Related: [[cargo-xtask-alias-compiles-deps-before-the-vet-gate]], AGENTS.md "Commit messages".
