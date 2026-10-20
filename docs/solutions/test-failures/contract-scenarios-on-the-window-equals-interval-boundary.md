---
title: A file wait with window == interval resolves after one or two intervals, so contract timing must not sit on it
date: 2026-10-05
category: test-failures
module: features/cli-flags.feature
problem_type: test_failure
component: testing_framework
severity: medium
symptoms:
  - "\"-i sets the interval\" passed at ~1.5s on PR CI, then measured 3205/3207ms on the push run of the same tree"
  - "Elapsed time for a file wait is bimodal: about one interval or about two"
root_cause: async_timing
resolution_type: test_fix
related_components: [features/support/servers.js, features/support/steps-options.js]
tags: [contract, cucumber, timing, interval, window, flake, spike-next-rs]
---

# A file wait with window == interval resolves after one or two intervals, so contract timing must not sit on it

## Problem

`window` is raised to `interval`. A `file:` wait is ready when `now >= t + window`, where `t` is
stamped after the first async stat. At the first interval tick that holds only when that tick's
stat latency is at least the first stat's, so the wait resolves after one interval or two,
depending on stat and timer jitter. Both engines do this, and so did 9.x. A contract scenario
that measured `-i` on an existing file passed at ~1.5s on PR CI and hit 3205/3207ms on the push
run of the same tree (#108).

## Solution

Measure the interval with a resource that has no stability window and becomes ready between the
first and second poll: a TCP server that starts listening after a delay D with
`S < D < S + interval`, where S is CLI startup. 1000ms was chosen for the slowest CI rows
(step: `a TCP server that starts listening after {int}ms`).

## Prevention

Never assert elapsed time on a `window == interval` file wait. A deterministic repeat on the
same tree is a boundary bug in the scenario, not a flake to rerun.
