---
title: A workflow that lives only on a non-default branch never fires on workflow_run or workflow_dispatch
date: 2026-09-30
category: best-practices
module: .github/workflows
problem_type: best_practice
component: infrastructure
severity: high
applies_when:
  - A workflow file exists only on a long-lived non-default branch (e.g. spike-next-rs) and must react to CI on that branch
  - A job downloads an artifact that an earlier job may not have produced yet
retire_when: "GitHub documents that workflow_run / workflow_dispatch fire for workflow files on non-default branches (events-that-trigger-workflows page)"
tags: [github-actions, workflow-run, workflow-dispatch, workflow-call, reusable-workflow, download-artifact, spike-next-rs]
---

# A workflow that lives only on a non-default branch never fires on workflow_run or workflow_dispatch

## Context

Issue #52 asked for `.github/workflows/rs-prerelease.yml` triggered `on: workflow_run` of CI for pushes to `spike-next-rs`. GitHub fires `workflow_run` and `workflow_dispatch` only for workflow files present on the repository's default branch (`master`). A file that exists only on `spike-next-rs` never runs, and the failure is silent: no error, just no run.

## Guidance

Make the workflow reusable and call it from a job inside the CI run that already fires on that branch:

```yaml
# node.js.yml (runs on push to spike-next-rs)
prerelease:
  needs: [build, rust, package]
  if: github.event_name == 'push' && github.ref == 'refs/heads/spike-next-rs' && github.repository == 'kevinold/wait-on'
  permissions:
    contents: write          # a called workflow cannot exceed its caller's grant
  uses: ./.github/workflows/rs-prerelease.yml

# rs-prerelease.yml
on:
  workflow_call:
  workflow_dispatch:         # inert until this file is on the default branch
```

Because the called workflow runs inside the caller's run (same `run_id`), `actions/download-artifact` reads the caller's artifacts with no `run-id` or extra token.

Download with `pattern:`, not `name:`, when the artifact may be absent. Uploading with `if-no-files-found: ignore` creates no artifact at all. A download by `name: package` then errors ("Artifact not found") and turns CI red. A download by `pattern: package` with `merge-multiple: true` succeeds on an empty set, and a `hashFiles('dist/wait-on-*.tgz') != ''` step gate skips the release.

`hashFiles()` works only in step-level `if`, not job-level. So "no-op until `Cargo.toml` exists" means per-step guards, and the jobs still consume runner minutes.

## Why This Matters

Built as the issue specified, the prerelease would never have run, and nothing would have said so. The name-based download would have turned every push to `spike-next-rs` red until lane L9 produced a tarball, blocking every lane gated on that branch. The doc review caught both before implementation (feasibility and adversarial reviewers plus a codex cross-model pass).

## When to Apply

- Any automation for a long-lived non-default branch: spikes, `next`, maintenance branches.
- Any job that consumes an artifact produced by an optional or not-yet-enabled step.

Limitation: bare `workflow_dispatch` on the reusable file stays a no-op even after promotion to `master`. A dispatch run has no `package` artifact in its own run, so enabling it needs a `run-id` input plus `actions: read`.

## Examples

Verified with `actionlint` in this repo. Removing the reusable file while the caller references it fails with `could not read reusable workflow file for "./.github/workflows/rs-prerelease.yml"`. See `.github/workflows/node.js.yml` (the `prerelease` job) and `.github/workflows/rs-prerelease.yml`.

Related repo quirk: the multi-worker-pm config loader reads `.multi-worker-pm.json` only from the primary checkout, never from a git worktree copy. Validate a new config from a worktree with `run.mjs spine config --digest --primary <scratch dir containing .git and the file>`.

## Related

- kevinold/wait-on#52 (L0 bootstrap), spine kevinold/wait-on#35
- `docs/plans/2026-09-30-1142-chore-spike-l0-bootstrap-plan.md` (KTD1)
- `docs/guides/ci.md` ("Why rs-prerelease.yml is reusable")
