'use strict';

// xtask/assets/prebuild-probe.js: the JS probe `cargo xtask package` runs inside install cells and
// containers to exercise the installed package's API and CLI under Node.

const childProcess = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

describe('prebuild-probe', function () {
  this.timeout(15000); // real probe subprocesses; Windows needs headroom

  const REPO = path.join(__dirname, '..');
  const PROBE = path.join(REPO, 'xtask', 'assets', 'prebuild-probe.js');
  const FAKE_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');

  function tmp() {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-pkg-'));
  }

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
