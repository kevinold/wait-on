---
title: Spike CI flakes - rerun once, and treat a deterministic repeat as a regression
date: 2026-10-05
category: workflow-issues
module: .github/workflows
problem_type: workflow_issue
component: development_workflow
severity: medium
symptoms:
  - "mlugg/setup-zig post-step fails with `fetch failed` on the x86_64-linux-musl napi row, which skips the package job and its size budget"
  - "Windows test/https-proxy.mocha.js closeServers after-each hook times out"
applies_when:
  - "A spike-next-rs PR or push run fails on a step unrelated to the diff"
tags: [ci, flake, setup-zig, windows, mwpm, spike-next-rs]
---

# Spike CI flakes - rerun once, and treat a deterministic repeat as a regression

## Context

While the MWPM orchestrator drove spine lanes on `spike-next-rs`, two CI failures cleared on a
single rerun:

- `mlugg/setup-zig` post-step `fetch failed` on the x86_64-linux-musl `napi` row. The failed row
  skips the `package` job, so the size budget never runs. A green run without `package` is not
  a size-budget pass.
- A Windows `test/https-proxy.mocha.js` `closeServers` after-each timeout.

## Guidance

Rerun a failed job once. A pass on rerun is a flake; note it in the PR. A failure that repeats on
the same tree is a regression to diagnose, not to loosen (no wider timeouts or retries in
`lib/`). The interval scenario's 3205/3207ms repeat (#108) looked like a flake and was a real
boundary bug; see
`docs/solutions/test-failures/contract-scenarios-on-the-window-equals-interval-boundary.md`.
