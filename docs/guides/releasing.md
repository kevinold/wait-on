# Releasing

## Node channels

`latest` from `master`, `next` (rc) from `next`, maintenance from `*.x`. Runbook: [`.github/RELEASING.md`](../../.github/RELEASING.md). The spike does not touch `.releaserc.json` or `release.yml`.

- semantic-release derives version and notes from Conventional Commits; `fix:`/`feat:`/breaking release, `ci:`/`docs:`/`chore:` do not.
- commitlint checks every PR commit and the PR-title check checks the title; both must be Conventional Commits.

## Rust test prereleases (fork)

Every push to `spike-next-rs` in `kevinold/wait-on` whose `build`, `rust`, and `package` jobs pass calls [`rs-prerelease.yml`](../../.github/workflows/rs-prerelease.yml) (details: [ci.md](ci.md#why-rs-prereleaseyml-is-reusable)).

- Tag: `rs-<package.json version>-<sha7>`, for example `rs-10.0.0-rc.1-abc1234`. Marked prerelease on GitHub.
- Assets: the multi-platform `wait-on-*.tgz` and `SHA256SUMS`.
- The tarball carries all eight prebuilds (about 3.4 MB packed; sizes in [ci.md](ci.md#cirspackage)) and installs with `--ignore-scripts`, `--omit=optional`, or pnpm; `ci:rs:package` proves each before the upload.
- Linux glibc addons need glibc 2.39 or newer (built on ubuntu-24.04: Debian 13, Ubuntu 24.04, Fedora 40+). On older glibc, `WAIT_ON_ENGINE=rust` falls back to the JS engine and `rust-strict` fails to load. musl (Alpine) is unaffected. Lowering the floor is a `napi` build change (an older glibc target), not yet done.
- Never published to npm.

Install and try:

```bash
npm i --ignore-scripts <tarball-url>
WAIT_ON_ENGINE=rust npx wait-on tcp:3000
```

## End-to-end prerelease verification

Verify a prerelease tarball from the fork GitHub Release end to end, including checksums and the Rust engine actually loading.

Status: planned (lane L10)

## Deferred

Not in this spike:

- npm publish from the fork (trusted publisher registration is an operator step).
- Standalone binary.
- Cutover of the default engine to Rust (and removing the JS fallback).
