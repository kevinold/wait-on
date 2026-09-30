'use strict';

// The Rust pending list (KD-S2): under WAIT_ON_ENGINE=rust*, listed tests report
// pending and a listed title no suite registers fails the run; under js the list
// has no effect. Each case runs a fixture spec in a mocha subprocess.

const childProcess = require('child_process');
const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

const MOCHA = require.resolve('mocha/bin/mocha.js');
const FIXTURES = path.join(__dirname, 'fixtures', 'rust-pending');

function runFixture(hooks, engine) {
  const env = { ...process.env };
  if (engine === undefined) delete env.WAIT_ON_ENGINE;
  else env.WAIT_ON_ENGINE = engine;
  const args = [
    MOCHA,
    '--no-config',
    '--reporter',
    'json',
    '--require',
    path.join(FIXTURES, hooks),
    path.join(FIXTURES, 'spec.js')
  ];
  const r = childProcess.spawnSync(process.execPath, args, { env, encoding: 'utf8' });
  return { code: r.status, report: JSON.parse(r.stdout) };
}

describe('rust pending list', function () {
  this.timeout(10000);

  for (const engine of ['rust', 'rust-strict']) {
    it(`should report a listed test as pending under ${engine}`, function () {
      const { code, report } = runFixture('hooks-listed.js', engine);
      expect(code).to.equal(0);
      expect(report.stats.pending).to.equal(1);
      expect(report.stats.passes).to.equal(1);
      expect(report.pending[0].fullTitle).to.equal('rust-pending fixture listed test');
    });
  }

  for (const engine of [undefined, 'js']) {
    it(`should run every test when WAIT_ON_ENGINE is ${JSON.stringify(engine)} even when listed`, function () {
      const { code, report } = runFixture('hooks-listed.js', engine);
      expect(code).to.equal(0);
      expect(report.stats.passes).to.equal(2);
      expect(report.stats.pending).to.equal(0);
    });
  }

  it('should fail the run under rust when a listed test is not registered', function () {
    const { code, report } = runFixture('hooks-stale.js', 'rust');
    expect(code).to.not.equal(0);
    expect(report.failures[0].err.message).to.include('no such test');
    expect(report.failures[0].err.message).to.include('test/rust-pending.js');
  });

  it('should ignore a stale entry under js', function () {
    const { code, report } = runFixture('hooks-stale.js', 'js');
    expect(code).to.equal(0);
    expect(report.stats.passes).to.equal(2);
  });

  it('keeps the committed list empty and composed into the root hooks', function () {
    const rustPending = require('./rust-pending');
    const { mochaHooks } = require('./frozen-clock');
    expect(rustPending.pending).to.deep.equal([]);
    expect(mochaHooks.beforeAll).to.be.a('function');
    expect(mochaHooks.beforeEach).to.be.a('function');
    expect(mochaHooks.beforeAll).to.equal(rustPending.mochaHooks.beforeAll);
    expect(mochaHooks.beforeEach).to.equal(rustPending.mochaHooks.beforeEach);
  });
});
