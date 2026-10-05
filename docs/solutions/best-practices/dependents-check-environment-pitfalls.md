---
title: Running the dependents check - proxy asymmetry, npm release-age gates, and shell-free npm
date: 2026-10-05
category: best-practices
module: scripts/dependents.js
problem_type: best_practice
component: testing_framework
severity: medium
applies_when:
  - "Running npm run dependents before an rc or GA release"
  - "Adding a proxy variable, a control version, or a manifest entry to the dependents check"
  - "Porting the check to another runtime or CI host"
related_components: [test/dependents/dependents.json, .github/RELEASING.md]
tags: [dependents, npm, proxy, all-proxy, min-release-age, release-check]
---

# Running the dependents check - proxy asymmetry, npm release-age gates, and shell-free npm

## Context

`npm run dependents` (jeffbski/wait-on#264) runs published dependents' own commands
twice: on the wait-on version they pin (baseline), then on the packed tarball. A row that
fails on the tarball but passed on baseline is a `regression` and exits 1. A row that fails
on both is `pre-existing` and exits 0. So anything that fails the baseline for an
environment reason can hide a real tarball regression. The first recorded run, and the
review of it, turned up two environment traps the code alone does not explain.

## Guidance

- **Scrub every proxy source either side honors, not only `HTTP(S)_PROXY` and `NO_PROXY`.**
  The baseline and the tarball do not read the same proxy variables. start-server-and-test
  v3.0.12 pins wait-on 9.1.0, which probes with axios, and axios resolves proxies through
  proxy-from-env, which also honors `ALL_PROXY` and the `npm_config_*proxy` variables that
  `npm run` exports. The demos' `curl` honors `ALL_PROXY` too. The 10.x tarball probes
  through undici's `EnvHttpProxyAgent`, which reads only `HTTP(S)_PROXY` and `NO_PROXY`. With
  one of the extra variables set, the baseline can fail through the proxy while the tarball
  goes direct. If the tarball also regresses that command, the row reads `pre-existing` and
  the run exits 0. The run environment drops all of them (`PROXIES` in
  `scripts/dependents.js`). The #263 consumer-contract review reached the same rule for its
  9.5.1 control (session history). An npm that re-reads a proxy from `.npmrc` inside the
  dependent's own `npm run` can still set it again, which the scrub cannot prevent.
- **The 9.5.1 control needs the control version to be older than npm's release-age gate.**
  With `min-release-age=7` in `~/.npmrc`, `npm install --no-save --ignore-scripts wait-on@9.5.1` failed with
  `ETARGET No matching version found for wait-on@9.5.1 with a date before ...`, because
  9.5.1 was published on 2026-09-29. `npm config get` did not show the setting; reading
  `~/.npmrc` did. A failed swap is a harness failure, so it aborts the entry before the
  verdict table prints, and the baseline and tarball results are lost too. Run without
  `--control` until the control version clears the window. Do not override the gate from
  the script: it is the maintainer's supply-chain setting.
- **Spawn npm as `node <npm_execpath> ...`, never `npm` or `shell: true`.** `npm` is a
  `.cmd` shim on Windows, and a shell brings back quoting. `npm run` sets `npm_execpath`. The
  script refuses to start without it, so run it through `npm run dependents`
  (`main` in `scripts/dependents.js`).
- **Judge `npm ls` by its parsed versions only.** After `npm install --no-save <tgz>` breaks
  the dependent's exact pin, `npm ls wait-on --all --json` exits 1 (`ELSPROBLEMS`), so its
  exit code means nothing here (`lsVerdict` in `scripts/dependents.js`).
- **Set `npm_config_ignore_scripts=true` for the dependent's commands**
  (`runEnv` in `scripts/dependents.js`). start-server-and-test's `demo3` runs `npm run test`, whose
  `pretest` runs `prettier --write` on the clone. Explicit `npm run <script>` still runs with
  the flag set; only pre and post hooks are skipped.
- **The run needs `git` and `curl`.** `demo-multiple`, the #260 bad-port control, ends in
  `curl`. On a host without `curl` it fails on both sides and reads `pre-existing`, so the
  run cannot judge the regression it was built to catch.

## Why This Matters

The check exists to catch regressions no repo test sees, such as #260 (Fetch refuses ports
6000 and 6010). Each trap above turns that signal into a quiet exit 0 or an aborted run.
The code carries the scrub list and the guards, but not why `ALL_PROXY` belongs on the list,
or why a control install can fail on one maintainer's machine and not another's.

## When to Apply

- Before each rc or GA approval (`.github/RELEASING.md`, Dependents check).
- When a run shows `pre-existing` rows that fail on a probe of localhost: check the proxy
  variables in the shell first.
- When `--control` fails with `ETARGET` and a "date before" message: check `~/.npmrc` for
  `min-release-age` or `before`.
- When adding a dependent whose pinned wait-on uses a different HTTP client from the tarball.

## Examples

A control run blocked by the release-age gate (the swap fails before the table):

```text
npm error notarget No matching version found for wait-on@9.5.1 with a date before 9/28/2026
start-server-and-test: npm install --no-save --ignore-scripts wait-on@9.5.1 failed (kept /tmp/...)
```

The same check without `--control` prints the table and exits on the verdict:

```sh
npm pack --pack-destination /tmp/pack
npm run dependents -- --tgz /tmp/pack/wait-on-10.0.0-rc.1.tgz
```

## Related

- `.github/RELEASING.md` (Dependents check): when and how to run it.
- `test/dependents/dependents.json`: the manifest of dependents and their commands.
- The Rust spike's `cargo xtask dependents` has a sibling learning on the `spike-next-rs`
  branch (`docs/solutions/best-practices/dependents-harness-npm-and-candidate-list.md`), not
  present on `next`.
