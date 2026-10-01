'use strict';

// Pure planning functions behind the build:napi and ci:rs npm scripts.

const childProcess = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

const buildNapi = require('../scripts/build-napi');
const ciRs = require('../scripts/ci-rs');
const ciRsPackage = require('../scripts/ci-rs-package');
const { prebuildDir, isMusl } = require('../lib/engine');

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

describe('ci:rs:package', function () {
  this.timeout(15000); // real npm pack and probe subprocesses; Windows needs headroom

  const PO4 = [
    'darwin-arm64',
    'darwin-x64',
    'linux-x64',
    'linux-arm64',
    'linux-x64-musl',
    'linux-arm64-musl',
    'win32-x64',
    'win32-arm64'
  ];
  const hostDir = prebuildDir({
    platform: process.platform,
    arch: process.arch,
    musl: isMusl({ platform: process.platform, report: process.report.getReport.bind(process.report) })
  });

  function tmp() {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-pkg-'));
  }

  function touch(root, rel, body = 'x') {
    const file = path.join(root, rel);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, body);
  }

  it('should list exactly the eight PO4 prebuild dirs from the build target table', function () {
    expect(Object.keys(buildNapi.TARGETS)).to.have.length(8);
    expect(ciRsPackage.expectedPrebuildDirs()).to.deep.equal(PO4);
  });

  it('should name every missing prebuild dir', function () {
    const root = tmp();
    for (const dir of ['darwin-x64', 'linux-x64-musl', 'win32-arm64']) touch(root, `${dir}/wait-on.node`);
    const missing = ciRsPackage.missingPrebuilds({ prebuildsRoot: root, dirs: PO4 });
    expect(missing).to.deep.equal(['darwin-arm64', 'linux-x64', 'linux-arm64', 'linux-arm64-musl', 'win32-x64']);
    const message = ciRsPackage.formatMissing(missing);
    for (const dir of missing) expect(message).to.include(`prebuilds/${dir}/wait-on.node`);
  });

  it('should pass with all eight prebuilds present', function () {
    const root = tmp();
    for (const dir of PO4) touch(root, `${dir}/wait-on.node`);
    expect(ciRsPackage.missingPrebuilds({ prebuildsRoot: root, dirs: PO4 })).to.deep.equal([]);
  });

  it('should require only the host dir under host-only', function () {
    const root = tmp();
    touch(root, `${hostDir}/wait-on.node`);
    const hostOnly = ciRsPackage.requiredDirs({ hostOnly: true });
    expect(hostOnly).to.have.length(1);
    expect(PO4.concat(`${process.platform}-${process.arch}`)).to.include(hostOnly[0]);
    expect(ciRsPackage.missingPrebuilds({ prebuildsRoot: root, dirs: hostOnly })).to.deep.equal([]);
    const all = ciRsPackage.requiredDirs({ hostOnly: false });
    expect(ciRsPackage.missingPrebuilds({ prebuildsRoot: root, dirs: all })).to.not.be.empty;
  });

  it('should pack every prebuild and no build intermediates', function () {
    if (!process.env.npm_execpath) this.skip(); // needs npm; set when run through npm run
    const root = tmp();
    fs.copyFileSync(path.join(__dirname, '..', 'package.json'), path.join(root, 'package.json'));
    for (const rel of ['lib/wait-on.js', 'bin/wait-on', 'exampleConfig.js', 'index.d.ts']) touch(root, rel);
    for (const dir of PO4) touch(root, `prebuilds/${dir}/wait-on.node`);
    for (const rel of ['target/x', 'crates/x', 'scripts/prebuild-probe.js', 'docs/x', 'test/x', 'benchmarks/x', 'Cargo.toml']) {
      touch(root, rel);
    }
    const out = childProcess.execFileSync(process.execPath, [process.env.npm_execpath, 'pack', '--dry-run', '--json'], {
      cwd: root,
      encoding: 'utf8'
    });
    const packJson = JSON.parse(out)[0];
    expect(ciRsPackage.checkPack(packJson, PO4)).to.deep.equal([]);
    const paths = packJson.files.map((f) => f.path);
    for (const dir of PO4) expect(paths).to.include(`prebuilds/${dir}/wait-on.node`);
  });

  it('should fail the pack check when a prebuild is missing or an intermediate ships', function () {
    const files = PO4.filter((d) => d !== 'win32-x64')
      .map((d) => ({ path: `prebuilds/${d}/wait-on.node`, size: 1 }))
      .concat({ path: 'scripts/prebuild-probe.js', size: 1 });
    const problems = ciRsPackage.checkPack({ files }, PO4).join('\n');
    expect(problems).to.include('prebuilds/win32-x64/wait-on.node');
    expect(problems).to.include('scripts/prebuild-probe.js');
  });

  it("should reject a manifest with lifecycle scripts or optionalDependencies and accept the repo's", function () {
    expect(ciRsPackage.checkManifest({ scripts: { postinstall: 'x' } }).join('\n')).to.include('postinstall');
    expect(ciRsPackage.checkManifest({ scripts: { prepare: 'x' } }).join('\n')).to.include('prepare');
    expect(ciRsPackage.checkManifest({ optionalDependencies: {} }).join('\n')).to.include('optionalDependencies');
    expect(ciRsPackage.checkManifest(require('../package.json'))).to.deep.equal([]);
  });

  it('should report packed, unpacked, js-only and per-target sizes', function () {
    const files = PO4.map((d) => ({ path: `prebuilds/${d}/wait-on.node`, size: 600000 })).concat(
      { path: 'lib/wait-on.js', size: 150000 },
      { path: 'package.json', size: 50000 }
    );
    expect(ciRsPackage.sizeReport({ size: 700000, unpackedSize: 5000000, files })).to.deep.equal({
      packed: 700000,
      unpacked: 5000000,
      jsOnlyUnpacked: 200000,
      targets: PO4.map((dir) => ({ dir, size: 600000 }))
    });
  });

  it('should write a sha256sum-compatible line', function () {
    const file = path.join(tmp(), 'wait-on-1.0.0.tgz');
    fs.writeFileSync(file, 'abc');
    expect(ciRsPackage.sha256sumsLine(file)).to.equal(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  wait-on-1.0.0.tgz\n'
    );
  });

  it('should parse --host-only', function () {
    expect(ciRsPackage.parseArgs(['--host-only'])).to.deep.equal({ hostOnly: true });
    expect(ciRsPackage.parseArgs([])).to.deep.equal({ hostOnly: false });
  });

  it('should plan npm, npm omit-optional and pnpm cells with scripts disabled and no shell', function () {
    const env = { PATH: 'p', WAIT_ON_NATIVE_LIBRARY_PATH: '/elsewhere/wait-on.node' };
    const cells = ciRsPackage.installCells({ tgz: '/t/wait-on.tgz', npmExecPath: '/npm-cli.js', env });
    expect(cells.map((c) => c.name)).to.deep.equal(['npm', 'npm-omit-optional', 'pnpm']);
    for (const cell of cells) {
      expect(cell.cmd).to.equal(process.execPath);
      expect(cell.args[0]).to.equal('/npm-cli.js');
      expect(cell.args).to.include('--ignore-scripts');
      expect(cell.args).to.include('/t/wait-on.tgz');
      expect(cell.env).to.not.have.property('WAIT_ON_NATIVE_LIBRARY_PATH');
      expect(cell.env.PATH).to.equal('p');
    }
    expect(cells[0].args).to.not.include('--omit=optional');
    expect(cells[1].args).to.include('--omit=optional');
    expect(cells[2].args).to.include.members(['exec', '--package', `pnpm@${ciRsPackage.PNPM_VERSION}`, 'pnpm', 'add']);
    expect(env).to.have.property('WAIT_ON_NATIVE_LIBRARY_PATH'); // caller's env untouched
  });

  it('should accept an addon realpath inside the project and reject one outside', function () {
    const project = fs.realpathSync(tmp());
    const inside = path.join(project, 'node_modules', 'wait-on', 'prebuilds', hostDir, 'wait-on.node');
    expect(() => ciRsPackage.assertInstalledAddon({ realpath: inside, projectRoot: project, dir: hostDir })).to.not.throw();
    const repoAddon = path.join(__dirname, '..', 'prebuilds', hostDir, 'wait-on.node');
    expect(() => ciRsPackage.assertInstalledAddon({ realpath: repoAddon, projectRoot: project, dir: hostDir })).to.throw(
      repoAddon
    );
    const wrongDir = path.join(project, 'node_modules', 'wait-on', 'prebuilds', 'other', 'wait-on.node');
    expect(() => ciRsPackage.assertInstalledAddon({ realpath: wrongDir, projectRoot: project, dir: hostDir })).to.throw(
      hostDir
    );
  });

  it('should plan four container cells over glibc and musl with read-only and no network', function () {
    const cells = ciRsPackage.containerCells({ arch: 'x64' });
    expect(cells.map((c) => c.name)).to.deep.equal(['glibc-ready', 'glibc-timeout', 'musl-ready', 'musl-timeout']);
    expect(cells.map((c) => c.image)).to.deep.equal([
      'node:24-trixie-slim',
      'node:24-trixie-slim',
      'node:24-alpine',
      'node:24-alpine'
    ]);
    expect(cells.map((c) => c.expectedDir)).to.deep.equal(['linux-x64', 'linux-x64', 'linux-x64-musl', 'linux-x64-musl']);
    for (const cell of cells) {
      const run = cell.runArgs.join(' ');
      expect(run).to.include('--read-only');
      expect(run).to.include('--network none');
      expect(run).to.include('-e WAIT_ON_ENGINE=rust-strict');
      expect(run).to.include(`${cell.tag} node /app/prebuild-probe.js`);
      expect(cell.npmrc).to.equal('ignore-scripts=true\n');
      expect(cell.dockerfile).to.include(`FROM ${cell.image}`);
      expect(cell.dockerfile).to.include('prebuild-probe.js');
      expect(cell.dockerfile).to.include('--omit=optional');
    }
    expect(cells.filter((c) => c.expectReady).map((c) => c.name)).to.deep.equal(['glibc-ready', 'musl-ready']);
    for (const cell of cells.filter((c) => !c.expectReady)) {
      expect(cell.runArgs.slice(-3)).to.deep.equal(['--no-listener', '--timeout', '1000']);
    }
  });

  it('should derive the container dirs from the runner arch', function () {
    const dirs = ciRsPackage.containerCells({ arch: 'arm64' }).map((c) => c.expectedDir);
    expect([...new Set(dirs)]).to.deep.equal(['linux-arm64', 'linux-arm64-musl']);
  });

  it('should skip without docker locally, fail without docker under CI, and run when present', function () {
    const skip = ciRsPackage.dockerDecision({ dockerFound: false, ci: false });
    expect(skip.action).to.equal('skip');
    expect(skip.reason).to.include('docker');
    expect(skip.reason).to.not.include('\n');
    expect(ciRsPackage.dockerDecision({ dockerFound: false, ci: true }).action).to.equal('fail');
    expect(ciRsPackage.dockerDecision({ dockerFound: true, ci: true }).action).to.equal('run');
    expect(ciRsPackage.dockerDecision({ dockerFound: true, ci: false }).action).to.equal('run');
  });

  it('should pass a ready probe only when the API and CLI both succeed from the expected dir', function () {
    const addon = '/app/node_modules/wait-on/prebuilds/linux-x64-musl/wait-on.node';
    const ready = { realpath: addon, api: true, cli: 0, cliError: '' };
    const expect0 = { expectReady: true, expectedDir: 'linux-x64-musl' };
    expect(ciRsPackage.probeVerdict(ready, expect0)).to.equal(null);
    expect(ciRsPackage.probeVerdict({ ...ready, api: 'boom' }, expect0)).to.include('api');
    expect(ciRsPackage.probeVerdict({ ...ready, cli: 1 }, expect0)).to.include('cli');
    expect(ciRsPackage.probeVerdict({ ...ready, cli: null }, expect0)).to.include('cli');
    expect(ciRsPackage.probeVerdict(ready, { expectReady: true, expectedDir: 'linux-x64' })).to.include('linux-x64');
  });

  it('should pass a timeout probe only when the API and the CLI each time out', function () {
    const addon = '/app/node_modules/wait-on/prebuilds/linux-x64/wait-on.node';
    const timedOut = {
      realpath: addon,
      api: 'Timed out waiting for: tcp:127.0.0.1:1',
      cli: 1,
      cliError: 'Timed out waiting for: tcp:127.0.0.1:1'
    };
    const expectTimeout = { expectReady: false, expectedDir: 'linux-x64' };
    expect(ciRsPackage.probeVerdict(timedOut, expectTimeout)).to.equal(null);
    expect(ciRsPackage.probeVerdict({ ...timedOut, cliError: 'Error: Cannot find module' }, expectTimeout)).to.include('cli');
    expect(ciRsPackage.probeVerdict({ ...timedOut, api: true }, expectTimeout)).to.include('api');
    expect(ciRsPackage.probeVerdict({ ...timedOut, cli: 0 }, expectTimeout)).to.include('cli');
  });

  describe('prebuild-probe', function () {
    const REPO = path.join(__dirname, '..');
    const PROBE = path.join(REPO, 'scripts', 'prebuild-probe.js');
    const FAKE_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');

    // A project whose node_modules/wait-on links to this checkout, like an installed copy.
    function linkedProject() {
      const project = tmp();
      fs.mkdirSync(path.join(project, 'node_modules'));
      fs.symlinkSync(REPO, path.join(project, 'node_modules', 'wait-on'), 'junction');
      return project;
    }

    function probe(vars, args = []) {
      const env = { ...process.env, WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: FAKE_ADDON, ...vars };
      const r = childProcess.spawnSync(process.execPath, [PROBE, ...args], { cwd: linkedProject(), env, encoding: 'utf8' });
      return { code: r.status, stdout: r.stdout, stderr: r.stderr };
    }

    it('should print the loaded addon path and pass API and CLI checks against the fixture addon', function () {
      const { code, stdout, stderr } = probe({});
      expect(code, stderr).to.equal(0);
      const line = JSON.parse(stdout.trim());
      expect(line.addonPath).to.equal(FAKE_ADDON);
      expect(line.realpath).to.equal(fs.realpathSync(FAKE_ADDON));
      expect(fs.realpathSync(line.pkgDir)).to.equal(fs.realpathSync(REPO));
      expect(line.api).to.equal(true);
      expect(line.cli).to.equal(0);
    });

    it('should exit non-zero with the timeout message when nothing listens', function () {
      const { code, stdout, stderr } = probe({ WAIT_ON_FAKE_ADDON_ANSWER: 'timeout' }, ['--no-listener', '--timeout', '300']);
      expect(code).to.not.equal(0);
      expect(stderr).to.include('Timed out waiting for');
      const line = JSON.parse(stdout.trim());
      expect(line.api).to.include('Timed out waiting for');
      expect(line.cli).to.not.equal(0);
      expect(line.cliError).to.include('Timed out waiting for');
    });

    it('should fail under rust-strict when the addon cannot load', function () {
      const { code, stderr } = probe({ WAIT_ON_NATIVE_LIBRARY_PATH: path.join(tmp(), 'missing.node') });
      expect(code).to.not.equal(0);
      expect(stderr).to.include('WAIT_ON_ENGINE=rust-strict');
    });
  });
});
