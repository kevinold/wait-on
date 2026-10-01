'use strict';

// tcp checks under the built host addon (npm run build:napi): the Rust loop's error reason
// and its overall timeout while a connect is pending. Skips only when no prebuild exists;
// ci:rs always builds one first, so this never skips there.

const fs = require('fs');
const { describe, it, before } = require('mocha');
const { expect } = require('chai');

const waitOn = require('../lib/wait-on');
const { addonPath } = require('../lib/engine');
const { withEnv, runCLI } = require('./helpers/engine-env');
const { getFreePort } = require('./helpers/cli-conformance');

describe('addon checks (real addon)', function () {
  this.timeout(10000);
  const REAL = { WAIT_ON_ENGINE: 'rust-strict' };

  before(function () {
    if (!fs.existsSync(addonPath({}))) this.skip();
  });

  it('should report a Rust reason for a port nothing listens on (CLI)', async function () {
    const port = await getFreePort();
    const r = runCLI(REAL, ['--verbose', '--tcpTimeout', '5000', '-t', '4000', `tcp:127.0.0.1:${port}`]);
    expect(r.code).to.not.equal(0);
    expect(r.stdout).to.include(`error connecting to TCP host:127.0.0.1 port:${port}`);
    expect(r.stdout).to.include('(os error');
  });

  it('should deliver a spec the addon throws on (port above 65535) to the callback without throwing', async function () {
    const err = await withEnv(REAL, () => new Promise((resolve) => waitOn({ resources: ['tcp:localhost:99999'], timeout: 500 }, resolve)));
    expect(err).to.be.an('error');
    expect(err.message).to.include('port');
  });

  it('should fire the overall timeout while a Rust connect is pending', async function () {
    const start = Date.now();
    let err;
    await withEnv(REAL, () => waitOn({ resources: ['tcp:10.255.255.1:9'], tcpTimeout: 30000, timeout: 500 })).catch((e) => (err = e));
    expect(err).to.be.an('error');
    expect(err.message).to.include('Timed out');
    expect(Date.now() - start).to.be.below(2000);
  });
});
