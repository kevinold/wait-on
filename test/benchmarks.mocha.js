'use strict';

// benchmarks/http-ffi.js: the pure summarize helper and a subprocess smoke per engine.

const childProcess = require('child_process');
const fs = require('fs');
const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

const bench = require('../benchmarks/http-ffi');
const { addonPath } = require('../lib/engine');

const SCRIPT = path.join(__dirname, '..', 'benchmarks', 'http-ffi.js');

function runBench(engines) {
  const env = { ...process.env };
  delete env.WAIT_ON_ENGINE;
  return childProcess.spawnSync(process.execPath, [SCRIPT, '--iterations', '3', '--engines', engines], {
    encoding: 'utf8',
    env,
    timeout: 30000
  });
}

describe('benchmarks/http-ffi', function () {
  this.timeout(40000);

  it('should summarize samples as median and p95', function () {
    expect(bench.summarize([5, 1, 3, 2, 4])).to.deep.equal({ median: 3, p95: 5 });
  });

  it('should print a js row and exit 0', function () {
    const r = runBench('js');
    expect(r.status, r.stderr).to.equal(0);
    expect(r.stdout).to.match(/^js\s/m);
  });

  it('should print a rust-strict row and exit 0 when a prebuild exists', function () {
    if (!fs.existsSync(addonPath({}))) this.skip();
    const r = runBench('rust-strict');
    expect(r.status, r.stderr).to.equal(0);
    expect(r.stdout).to.match(/^rust-strict\s/m);
  });
});
