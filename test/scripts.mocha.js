'use strict';

// Pure planning functions behind the build:napi and ci:rs npm scripts.

const childProcess = require('child_process');
const os = require('os');
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
      [process.execPath, mocha, '--exit', 'test/**/*.mocha.js'],
      [process.execPath, path.join(repoRoot, 'scripts', 'bench-startup.js')]
    ]);
    expect(steps[5].env.WAIT_ON_ENGINE).to.equal('rust-strict');
  });

  it('runs node steps with process.execPath, not npm or a shell', function () {
    for (const step of ciRs.steps({ repoRoot })) {
      if (step.cmd !== 'cargo') expect(step.cmd).to.equal(process.execPath);
    }
  });
});

describe('bench:startup', function () {
  const bench = require('../scripts/bench-startup');
  const threshold = { relative: 0.25, floorMs: 50 };

  it('median of an odd, an even and an unsorted list', function () {
    expect(bench.median([3, 1, 2])).to.equal(2);
    expect(bench.median([4, 1, 3, 2])).to.equal(2.5);
    expect(bench.median([7])).to.equal(7);
  });

  it('verdict passes when overhead is under the relative allowance', function () {
    const v = bench.verdict({ jsMs: 200, rustMs: 240, threshold });
    expect(v).to.include({ ok: true, allowedMs: 50, overheadMs: 40 });
  });

  it('verdict passes under the absolute floor when the relative allowance is smaller', function () {
    const v = bench.verdict({ jsMs: 100, rustMs: 145, threshold });
    expect(v).to.include({ ok: true, allowedMs: 50 });
  });

  it('verdict fails past both with a message naming the numbers', function () {
    const v = bench.verdict({ jsMs: 100, rustMs: 160, threshold });
    expect(v.ok).to.equal(false);
    for (const n of ['60', '50', '100', '160']) expect(v.message).to.include(n);
  });

  it('withRecording replaces only the host key and keeps others', function () {
    const linux = { jsMs: 1, rustMs: 2, overheadMs: 1, date: '2026-09-01' };
    const darwin = { jsMs: 3, rustMs: 4, overheadMs: 1, date: '2026-09-30' };
    const baseline = { threshold, runs: 20, recorded: { 'linux-x64': linux } };
    const next = bench.withRecording(baseline, 'darwin-arm64', darwin);
    expect(next).to.deep.equal({ threshold, runs: 20, recorded: { 'linux-x64': linux, 'darwin-arm64': darwin } });
    expect(baseline.recorded).to.have.keys('linux-x64');
  });

  it('exits non-zero naming the addon when the Rust engine cannot load', function () {
    this.timeout(10000);
    const missing = path.join(os.tmpdir(), `wait-on-missing-${process.pid}.node`);
    const r = childProcess.spawnSync(
      process.execPath,
      [path.join(__dirname, '..', 'scripts', 'bench-startup.js'), '--runs', '1'],
      {
        env: { ...process.env, WAIT_ON_NATIVE_LIBRARY_PATH: missing },
        encoding: 'utf8',
        timeout: 9000
      }
    );
    expect(r.status).to.not.equal(0);
    const out = r.stdout + r.stderr;
    expect(out).to.include('rust-strict');
    expect(out).to.include(missing);
  });
});
