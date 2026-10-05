---
title: Running dependents' suites on a swapped wait-on - npm ls exit codes, lifecycle scripts, and which dependents exist
date: 2026-10-05
category: best-practices
module: xtask/src/dependents.rs
problem_type: best_practice
component: testing_framework
severity: medium
applies_when:
  - "Running or extending cargo xtask dependents"
  - "Proving a dependent really runs a swapped-in tarball"
related_components: [xtask/assets/dependents.json, docs/guides/testing.md]
tags: [dependents, npm, start-server-and-test, regression, spike-next-rs]
---

# Running dependents' suites on a swapped wait-on - npm ls exit codes, lifecycle scripts, and which dependents exist

## Context

`cargo xtask dependents` found the Fetch bad-port regression (#104, upstream #260):
start-server-and-test's `demo-multiple` waits on ports 6000/6010 and failed under `js` only.
No repo test used a bad-list port.

## Guidance

- **`npm ls` exits non-zero after the swap.** `npm install --no-save <tgz>` breaks the dependent's
  exact pin, so `npm ls wait-on --all --json` exits 1 (ELSPROBLEMS). Parse the JSON and check
  every version; ignore the exit code.
- **Turn off lifecycle scripts.** Set `npm_config_ignore_scripts=true`. start-server-and-test's
  demos fall back to `npm test`, whose `pretest` runs `prettier --write` on the clone.
- **The candidate list is short.** Only start-server-and-test (about 2.4M weekly downloads) and
  jest-dev-server (about 340k; `@mozillasecurity/jest-dev-server` is a republish) are viable
  published dependents. nx, cypress, playwright and storybook do not depend on wait-on.
- **Prove the swap and the route.** A rust-strict run counts only when some process dlopened the
  swapped addon and none loaded `lib/engine-js.js` (the proof preload), judged over all of that
  run's records.
