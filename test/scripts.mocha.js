'use strict';

// Pure planning functions behind the build:napi and ci:rs npm scripts.

const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

const buildNapi = require('../scripts/build-napi');
const ciRs = require('../scripts/ci-rs');

describe('build:napi', function () {
  const triples = {
    'aarch64-apple-darwin': 'darwin-arm64',
    'x86_64-apple-darwin': 'darwin-x64',
    'x86_64-unknown-linux-gnu': 'linux-x64',
    'aarch64-unknown-linux-gnu': 'linux-arm64',
    'x86_64-unknown-linux-musl': 'linux-x64-musl',
    'aarch64-unknown-linux-musl': 'linux-arm64-musl',
    'x86_64-pc-windows-msvc': 'win32-x64',
    'aarch64-pc-windows-msvc': 'win32-arm64'
  };

  for (const [triple, dir] of Object.entries(triples)) {
    it(`maps ${triple} to prebuilds/${dir}`, function () {
      const plan = buildNapi.planBuild({ target: triple, extraArgs: [], repoRoot: '/r' });
      expect(plan.prebuildPath).to.equal(path.join('/r', 'prebuilds', dir, 'wait-on.node'));
    });
  }

  it('rejects an unknown triple with the supported list', function () {
    expect(() => buildNapi.planBuild({ target: 'riscv64gc-unknown-linux-gnu', extraArgs: [], repoRoot: '/r' })).to.throw(
      /riscv64gc-unknown-linux-gnu.*x86_64-unknown-linux-gnu/
    );
  });

  it('parses the host triple from rustc -vV output', function () {
    const out = 'rustc 1.98.1 (48a229cea 2026-09-01)\nbinary: rustc\nhost: aarch64-apple-darwin\nrelease: 1.98.1\n';
    expect(buildNapi.hostTriple(out)).to.equal('aarch64-apple-darwin');
  });

  it('builds into target/napi and never the repo root', function () {
    const plan = buildNapi.planBuild({ target: 'x86_64-unknown-linux-gnu', extraArgs: [], repoRoot: '/r' });
    const outputDir = path.join('/r', 'target', 'napi', 'linux-x64');
    expect(plan.outputDir).to.equal(outputDir);
    expect(plan.napiArgs).to.deep.equal([
      'build',
      '--release',
      '--manifest-path',
      path.join('/r', 'crates', 'wait-on-napi', 'Cargo.toml'),
      '--output-dir',
      outputDir,
      '--target',
      'x86_64-unknown-linux-gnu'
    ]);
  });

  it('forwards extra args (-x) to napi', function () {
    const plan = buildNapi.planBuild({ target: 'x86_64-unknown-linux-musl', extraArgs: ['-x'], repoRoot: '/r' });
    expect(plan.napiArgs[plan.napiArgs.length - 1]).to.equal('-x');
  });

  it('splits --target from the CI argument shape', function () {
    expect(buildNapi.parseArgs(['--target', 'x86_64-unknown-linux-musl', '-x'])).to.deep.equal({
      target: 'x86_64-unknown-linux-musl',
      extraArgs: ['-x']
    });
    expect(buildNapi.parseArgs([])).to.deep.equal({ target: undefined, extraArgs: [] });
  });
});

describe('ci:rs', function () {
  const repoRoot = '/r';
  const mocha = require.resolve('mocha/bin/mocha.js');

  it('runs fmt, clippy, test, deny, the host build, then mocha under rust-strict, in that order', function () {
    const steps = ciRs.steps({ repoRoot });
    expect(steps.map(({ cmd, args }) => [cmd, ...args])).to.deep.equal([
      ['cargo', 'fmt', '--all', '--check'],
      ['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'],
      ['cargo', 'test', '--workspace'],
      ['cargo', 'deny', 'check'],
      [process.execPath, path.join(repoRoot, 'scripts', 'build-napi.js')],
      [process.execPath, mocha, '--exit', 'test/**/*.mocha.js']
    ]);
    expect(steps[5].env.WAIT_ON_ENGINE).to.equal('rust-strict');
  });

  it('runs node steps with process.execPath, not npm or a shell', function () {
    for (const step of ciRs.steps({ repoRoot })) {
      if (step.cmd !== 'cargo') expect(step.cmd).to.equal(process.execPath);
    }
  });
});
