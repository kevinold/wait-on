# Releasing

## Node channels

`latest` from `master`, `next` (rc) from `next`, maintenance from `*.x`. Runbook: [`.github/RELEASING.md`](../../.github/RELEASING.md). The spike does not touch `.releaserc.json` or `release.yml`.

- semantic-release derives version and notes from Conventional Commits; `fix:`/`feat:`/breaking release, `ci:`/`docs:`/`chore:` do not.
- commitlint checks every PR commit and the PR-title check checks the title; both must be Conventional Commits.

## Rust test prereleases (fork)

Every push to `spike-next-rs` in `kevinold/wait-on` whose `build`, `rust`, and `package` jobs pass calls [`rs-prerelease.yml`](../../.github/workflows/rs-prerelease.yml) (details: [ci.md](ci.md#why-rs-prereleaseyml-is-reusable)).

- Tag: `rs-<package.json version>-<sha7>`, for example `rs-10.0.0-rc.1-abc1234`. Marked prerelease on GitHub.
- Assets: the multi-platform `wait-on-*.tgz` and `SHA256SUMS`.
- The tarball carries all eight prebuilds (about 15 MB packed, 36 MB unpacked; sizes in [ci.md](ci.md#cirspackage)) and installs with `--ignore-scripts`, `--omit=optional`, or pnpm; `ci:rs:package` proves each before the upload.
- Linux glibc addons need glibc 2.39 or newer (built on ubuntu-24.04: Debian 13, Ubuntu 24.04, Fedora 40+). On older glibc, `WAIT_ON_ENGINE=rust` falls back to the JS engine and `rust-strict` fails to load. musl (Alpine) is unaffected. Lowering the floor is a `napi` build change (an older glibc target), not yet done.
- Never published to npm.

Install and try (`rust-strict` fails instead of falling back to JS when the addon cannot load):

```bash
npm i --ignore-scripts <tarball-url>
WAIT_ON_ENGINE=rust-strict npx wait-on tcp:3000
```

## End-to-end prerelease verification

A maintainer's check of a fork prerelease: start from the release URL, trust only `SHA256SUMS`, and show the Rust engine running from the installed package. CI's `package` job already runs every install cell (npm, npm `--omit=optional`, pnpm) and AE1 in glibc and musl containers on the same bytes before the upload ([ci.md](ci.md#cirspackage)), so this runbook repeats one install cell and one container, and adds what CI does not do: the download, the checksum and AE2. It is run by hand; nothing in CI reruns it.

Last verified: `rs-10.0.0-rc.1-832c588` (commit `832c588`, published 2026-10-01T09:12Z), verified 2026-10-01 on darwin arm64 with docker (linux/arm64), Node 24, by lane L10 (#62).

### Runbook

Run from the repo root. `TAG` is the newest `rs-*` prerelease (`gh release list -R kevinold/wait-on -L 1`).

1. Download and check the assets.

   ```bash
   TAG=rs-10.0.0-rc.1-832c588
   BASE=https://github.com/kevinold/wait-on/releases/download/$TAG
   mkdir -p /tmp/l10/dl && cd /tmp/l10/dl
   curl -fsSLO "$BASE/wait-on-10.0.0-rc.1.tgz" && curl -fsSLO "$BASE/SHA256SUMS"
   shasum -a 256 -c SHA256SUMS        # sha256sum -c on Linux
   ```

2. Install into a fresh project with scripts disabled, and probe it. `xtask/assets/prebuild-probe.js` is the probe CI's package job uses: it waits on a local tcp port through the installed API and CLI and prints where the addon loaded from.

   ```bash
   unset WAIT_ON_NATIVE_LIBRARY_PATH HTTP_PROXY HTTPS_PROXY http_proxy https_proxy NO_PROXY no_proxy
   P=$(mktemp -d) && cd "$P" && echo '{ "name": "probe", "private": true }' > package.json
   npm install --ignore-scripts --no-audit --no-fund /tmp/l10/dl/wait-on-10.0.0-rc.1.tgz
   cp <repo>/xtask/assets/prebuild-probe.js . && WAIT_ON_ENGINE=rust-strict node prebuild-probe.js
   ```

   Pass: exit 0, `api: true`, `cli: 0`, and `realpath` starts with the project's real path (`/var` is `/private/var` on macOS) and ends with `prebuilds/<host-dir>/wait-on.node`. With `WAIT_ON_NATIVE_LIBRARY_PATH=/nonexistent/wait-on.node` the same command must fail with `failed to load the native addon` (no silent JS fallback).

3. AE1: install at image build time with `ignore-scripts=true`, then run read-only with no network.

   ```bash
   C=$(mktemp -d) && cp /tmp/l10/dl/wait-on-10.0.0-rc.1.tgz "$C/wait-on.tgz" && cp <repo>/xtask/assets/prebuild-probe.js "$C/"
   echo 'ignore-scripts=true' > "$C/.npmrc" && echo '{ "name": "ae1", "private": true }' > "$C/package.json"
   printf 'FROM node:24-trixie-slim\nWORKDIR /app\nCOPY .npmrc package.json wait-on.tgz prebuild-probe.js ./\nRUN npm install --omit=optional --no-audit --no-fund ./wait-on.tgz\n' > "$C/Dockerfile"
   docker build -q -t wait-on-ae1-glibc "$C"
   docker run --rm --read-only --network none -e WAIT_ON_ENGINE=rust-strict wait-on-ae1-glibc node /app/prebuild-probe.js
   docker rmi wait-on-ae1-glibc
   ```

   Pass: exit 0 and `realpath` `/app/node_modules/wait-on/prebuilds/linux-<arch>/wait-on.node`. The image must have glibc 2.39 or newer (`node:24-trixie-slim` has 2.41).

4. AE2: `validateStatus` is a JS function the Rust loop calls across the FFI boundary. Save the driver below as `ae2.js` in the step 2 project and run it there with the proxy variables still unset:

   ```bash
   FIXTURES=<repo>/crates/wait-on-core/tests/fixtures WAIT_ON_ENGINE=rust-strict node ae2.js
   ```

   ```js
   'use strict';
   const assert = require('assert');
   const fs = require('fs');
   const https = require('https');
   const path = require('path');

   const pem = (name) => fs.readFileSync(path.join(process.env.FIXTURES, name));
   const enginePath = require.resolve('wait-on/lib/engine', { paths: [process.cwd()] });
   const waitOn = require(path.dirname(path.dirname(enginePath)));

   let status = 200;
   const server = https.createServer({ cert: pem('server.pem'), key: pem('server-key.pem') }, (req, res) => {
     res.writeHead(status);
     res.end();
   });

   async function run(expected) {
     status = expected;
     let calls = 0;
     const validateStatus = (s) => {
       calls++;
       return s === 200;
     };
     const opts = {
       resources: [`https-get://localhost:${server.address().port}/health`],
       ca: pem('ca.pem'),
       strictSSL: true, // the schema default is false, and Rust gets TLS roots only with strictSSL
       validateStatus,
       interval: 100,
       timeout: 1500
     };
     try {
       await waitOn(opts);
       return { status: expected, resolved: true, calls };
     } catch (err) {
       return { status: expected, resolved: false, calls, error: err.message };
     }
   }

   server.listen(0, '127.0.0.1', async () => {
     try {
       const ok = await run(200);
       const no = await run(204);
       // engine-js (and with it undici and rxjs) is required lazily: absent means Rust ran the wait
       const loaded = Object.keys(require.cache).filter((k) =>
         /wait-on[\\/]lib[\\/]engine-js\.js$|node_modules[\\/](undici|rxjs)[\\/]/.test(k)
       );
       const addon = fs.realpathSync(require(enginePath).addonPath(process.env));
       console.log(JSON.stringify({ ok, no, jsEngineModulesLoaded: loaded, addon }));
       assert.equal(ok.resolved, true, '200 must pass');
       assert.ok(ok.calls >= 1, 'predicate called on 200');
       assert.equal(no.resolved, false, '204 must not pass');
       assert.match(no.error, /Timed out/);
       assert.ok(no.calls >= 1, 'predicate called on 204');
       assert.deepEqual(loaded, [], 'the Rust path ran');
       assert.ok(addon.startsWith(fs.realpathSync(process.cwd()) + path.sep), 'addon inside the installed package');
       console.log('AE2 PASS');
     } catch (err) {
       console.error(`AE2 FAIL: ${err.message}`);
       process.exitCode = 1;
     } finally {
       server.close();
     }
   });
   ```

   An env proxy for an https target routes that wait to the JS engine (see [architecture.md](architecture.md#engine-selection-and-fallback)), which is why the proxy variables must be unset; the `require.cache` assertion catches it if they are not.

### Results (`rs-10.0.0-rc.1-832c588`, 2026-10-01)

| Check | Result |
|---|---|
| Checksum | `wait-on-10.0.0-rc.1.tgz: OK` (sha256 `e770f8fb…b979602`, 15,035,917 bytes); a copy of `SHA256SUMS` with one digit flipped fails with `1 computed checksum did NOT match` |
| Install, npm `--ignore-scripts` | 12 packages; probe `{"api":true,"cli":0,"cliError":""}`, `realpath` `…/node_modules/wait-on/prebuilds/darwin-arm64/wait-on.node` inside the project |
| Strict load negative | exit 1, `WAIT_ON_ENGINE=rust-strict: failed to load the native addon at /nonexistent/wait-on.node` |
| AE1, `node:24-trixie-slim` (glibc 2.41, aarch64), `--read-only --network none` | exit 0, `{"api":true,"cli":0}`, `realpath` `/app/node_modules/wait-on/prebuilds/linux-arm64/wait-on.node` |
| AE2, `rust-strict` | 200: resolved, 1 predicate call. 204: rejected `Timed out waiting for: https-get://localhost:<port>/health`, 15 predicate calls. `engine-js`, `undici`, `rxjs` never loaded; addon inside the project. `AE2 PASS` |
| Pending list | none: `WAIT_ON_ENGINE=rust-strict npm run test:mocha` on the base commit with a host prebuild, 496 passing, 0 failing, 0 pending; `test/parser-properties.mocha.js` asserts `pending === 0` under `rust-strict`. CI push run 36839664376 green on every row. |

## Supply-chain delta

The requirements plan's priority #1 is a smaller runtime dependency tree, measured against the post-backlog `next` baseline (`joi`, `rxjs`, `undici`), not the old axios tree ([requirements plan, Success Criteria](../plans/2026-09-28-1239-feat-rust-port-plan.md)). Measured 2026-10-01T11:27Z from clean installs with `--ignore-scripts`:

| State | Runtime packages (transitive, excluding wait-on) | Declare a lifecycle hook (`preinstall`, `install`, `postinstall`, `prepare`) | Declare an install-time hook (`preinstall`, `install`, `postinstall`) |
|---|---|---|---|
| `next` baseline | 11 | 2 (`@hapi/tlds`, `undici`: `prepare`) | 0 |
| Rust engine as shipped today (`rs-10.0.0-rc.1-832c588`) | 11 | 2 | 0 |
| Projected after cutover (`joi` alone, `^18.2.9`) | 8 | 1 (`@hapi/tlds`: `prepare`) | 0 |

- Today equals the baseline: during the fallback window `joi`, `rxjs` and `undici` stay declared because the JS engine is still the default (R22). The Rust path does not load `rxjs` or `undici`, but npm still installs them.
- Cutover removes `rxjs`, `tslib` and `undici`: 11 to 8 packages, 2 to 1 `prepare` declaration. npm does not run `prepare` for registry installs, and no package declares an install-time hook in any state.
- Resolved versions: `joi@18.2.9` (`@hapi/address@5.1.1`, `@hapi/formula@3.0.2`, `@hapi/hoek@11.0.7`, `@hapi/pinpoint@2.0.1`, `@hapi/tlds@1.1.7`, `@hapi/topo@6.0.2`, `@standard-schema/spec@1.1.0`), `rxjs@7.8.2` (`tslib@2.8.1`), `undici@8.11.2`. `^` ranges move; rerun `npm ls --omit=dev --all` in a clean install to refresh.
- Pass/fail bar: not yet stated. The Success Criteria call for "a stated minimum reduction"; the operator states it and whether it gates the spike PR or only the cutover.

The Rust side that replaces `rxjs` and `undici` ships inside the addon:

| | Count | How |
|---|---|---|
| `Cargo.lock` packages | 156 | `grep -c '^name = ' Cargo.lock` |
| Third-party crates compiled into the addon, all eight targets | 134 | `cargo tree -p wait-on-napi -e normal --target all --prefix none`, deduplicated, minus `wait-on-core` and `wait-on-napi` (100 on one host: `--target all` adds the platform-gated crates such as `windows-sys`) |
| Dev, build and `xtask` crates | 20 | the rest of `Cargo.lock` |
| cargo-vet exemptions | 134 | `[[exemptions` in `supply-chain/config.toml` |
| cargo-vet imports | 7 | `[imports` in `supply-chain/config.toml`; 19 crates fully audited by them ([architecture.md](architecture.md#supply-chain)) |

## Cutover: what is still needed

The JS-to-Rust tooling inventory is [#85](https://github.com/kevinold/wait-on/issues/85). Before the Rust engine becomes the default:

- Restore the full PR CI matrix (operator PR; PRs run a trimmed matrix today, see [ci.md](ci.md#jobs)).
- State the supply-chain pass/fail bar (above).
- Evaluate Rust release tooling (knope, release-plz, git-cliff) and commit linting (crate-ci/committed) against semantic-release and commitlint; replace only if equally good (KD-S11, #85).
- npm trusted publisher registration for the fork, then publish the Rust build to `next`.
- Flip the default engine to Rust, then remove the JS fallback and with it `rxjs` and `undici` (the projected row above).
- Lower the glibc floor if older distributions matter (above).
- Optional: if this runbook should run in CI, a `cargo xtask` subcommand with Rust tests, using the runbook as its spec.

## Deferred

Not in this spike:

- npm publish from the fork (trusted publisher registration is an operator step).
- Standalone binary.
- Cutover of the default engine to Rust (and removing the JS fallback).
