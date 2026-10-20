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
start-server-and-test's own `demo-multiple` script (defined in its package.json, not wait-on's) waits on ports 6000/6010
and failed under `js` only.
No repo test used a bad-list port.

## Guidance

- **`npm ls` exits non-zero after the swap.** `npm install --no-save <tgz>` breaks the dependent's
  exact pin, so `npm ls wait-on --all --json` exits 1 (ELSPROBLEMS). Parse the JSON and check
  every version; ignore the exit code.
- **Turn off lifecycle scripts.** Set `npm_config_ignore_scripts=true`. start-server-and-test's
  own demo scripts fall back to its `npm test`, whose `pretest` (in its package.json, not
  wait-on's) runs `prettier --write` on the clone.
- **The candidate list is short.** Only start-server-and-test (about 2.4M weekly downloads) and
  jest-dev-server (about 340k; `@mozillasecurity/jest-dev-server` is a republish) are viable
  published dependents. nx, cypress, playwright and storybook do not depend on wait-on.
- **jest-dev-server: run the suites that call it, not its own package.** In
  `argos-ci/jest-puppeteer`, `packages/jest-dev-server` has no tests, so jest on it prints
  "No tests found" and a green row proves nothing. Run
  `npm test -- packages/jest-environment-puppeteer packages/expect-puppeteer` from the clone root
  (through npm, so `node_modules/.bin` is on `PATH`). Their global setup calls jest-dev-server
  `setup`, which calls `waitOn({ resources: ["tcp:0.0.0.0:4444"], timeout })` for the root
  `jest-puppeteer.config.js` server. jest-dev-server detects a timeout by the message prefix
  "Timed out waiting for", so both engines must keep that text; these suites only exercise the
  happy path.
- **Install Puppeteer's Chrome as a build step, with scripts off.** npm 11.16 blocks puppeteer's
  postinstall, but npm 11.13 runs it, and under emulation it left a half-extracted Chrome that
  `install.mjs` then refused ("browser folder exists but the executable is missing"). With
  `scripts: false` the entry builds with `node node_modules/puppeteer/install.mjs` before
  `npm run build`, the one Chrome download on every npm.
- **The `overrides` swap needs npm 11.16+.** After `npm pkg set overrides.wait-on=file:<tgz>`,
  npm 11.13's `npm install` reports "up to date" and keeps the locked nested `wait-on@8.0.1`;
  npm 11.16 swaps it. The harness's `npm ls` check fails the run loudly, so no wrong result
  slips through, but the run is wasted.
- **Linux on Apple Silicon.** Chrome for Testing has no linux-arm64 build, and in a
  `linux/amd64` container under Rosetta its zip extraction stopped part way twice. Run a native
  arm64 container with Debian's `chromium` and `PUPPETEER_SKIP_DOWNLOAD=true`,
  `PUPPETEER_EXECUTABLE_PATH=/usr/bin/chromium`; the harness passes the parent env through.
- **Mind port 4444.** The suites' config uses `usedPortAction: "kill"`, so a run kills whatever
  already listens on 4444. jest-dev-server's `launchTimeout` is 4000 ms; a cold, loaded host can
  hit "Server has taken more than 4000ms to start". Rerun once before calling it a regression.
- **Prove the swap and the route.** A rust-strict run counts only when some process dlopened the
  swapped addon and none loaded `lib/engine-js.js` (the proof preload), judged over all of that
  run's records.
