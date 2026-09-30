# Releasing

## Node channels

`latest` from `master`, `next` (rc) from `next`, maintenance from `*.x`. Runbook: [`.github/RELEASING.md`](../../.github/RELEASING.md). The spike does not touch `.releaserc.json` or `release.yml`.

- semantic-release derives version and notes from Conventional Commits; `fix:`/`feat:`/breaking release, `ci:`/`docs:`/`chore:` do not.
- commitlint checks every PR commit and the PR-title check checks the title; both must be Conventional Commits.

## Rust test prereleases (fork)

Every push to `spike-next-rs` in `kevinold/wait-on` whose `build`, `rust`, and `package` jobs pass calls [`rs-prerelease.yml`](../../.github/workflows/rs-prerelease.yml) (details: [ci.md](ci.md#why-rs-prereleaseyml-is-reusable)).

- Tag: `rs-<package.json version>-<sha7>`, for example `rs-10.0.0-rc.1-abc1234`. Marked prerelease on GitHub.
- Assets: the multi-platform `wait-on-*.tgz` and `SHA256SUMS`.
- Skipped when no tarball exists, which is every run until `ci:rs:package` is defined (lane L9).
- Never published to npm.

Install and try:

```bash
npm i <tarball-url>
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
