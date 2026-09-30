'use strict';

// L2 (#54): with an addon that exports tcpCheck/socketCheck, tcp: and socket: checks
// go to the addon; reverse and verbose lines stay in JS; an addon lacking an export
// falls back to the JS check. The fixture addon is the network-edge stub.

const fs = require('fs');
const net = require('net');
const path = require('path');
const { describe, it, beforeEach } = require('mocha');
const { expect } = require('chai');

const waitOn = require('../lib/wait-on');
const { withEnv, runCLI } = require('./helpers/engine-env');
const { getFreePort, socketPathIn, tempDir, listening } = require('./helpers/cli-conformance');

const CHECKS_ADDON = path.join(__dirname, 'fixtures', 'fake-addon-checks.js');
const PLAIN_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');
const fixture = require(CHECKS_ADDON);
const FAST = { timeout: 1000, interval: 100 };
const STRICT = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: CHECKS_ADDON };

function lastCall() {
  return fixture.calls[fixture.calls.length - 1];
}

// Run waitOn with verbose on and return every console.log line it printed.
async function verboseLines(vars, opts) {
  const lines = [];
  const origLog = console.log;
  console.log = (...args) => lines.push(args.join(' '));
  try {
    await withEnv(vars, () => waitOn({ ...FAST, ...opts, verbose: true }));
  } finally {
    console.log = origLog;
  }
  return lines;
}

// Real tcp (127.0.0.1) and socket listeners for the JS-check cases; closed after fn.
async function withListeners(fn) {
  const socketPath = socketPathIn(tempDir());
  const tcp = await listening(net.createServer(), 0, '127.0.0.1');
  const socket = await listening(net.createServer(), socketPath);
  try {
    return await fn([`tcp:127.0.0.1:${tcp.address().port}`, `socket:${socketPath}`]);
  } finally {
    tcp.close();
    socket.close();
  }
}

describe('addon checks (fixture)', function () {
  this.timeout(5000);

  beforeEach(function () {
    fixture.calls.length = 0;
  });

  it('should dispatch a tcp check to the addon with host, port and tcpTimeout', async function () {
    const port = await getFreePort();
    await withEnv(STRICT, () => waitOn({ ...FAST, resources: [`tcp:127.0.0.1:${port}`], tcpTimeout: 450 }));
    expect(lastCall()).to.deep.equal({ fn: 'tcpCheck', host: '127.0.0.1', port, timeoutMs: 450 });
  });

  it('should default a bare port to localhost', async function () {
    const port = await getFreePort();
    await withEnv(STRICT, () => waitOn({ ...FAST, resources: [`tcp:${port}`] }));
    expect(lastCall()).to.include({ fn: 'tcpCheck', host: 'localhost', port });
  });

  it('should pass an IPv6 literal without brackets', async function () {
    const port = await getFreePort();
    await withEnv(STRICT, () => waitOn({ ...FAST, resources: [`tcp:[::1]:${port}`] }));
    expect(lastCall()).to.include({ fn: 'tcpCheck', host: '::1', port });
  });

  it('should dispatch a socket check to the addon with the path', async function () {
    const socketPath = socketPathIn(tempDir());
    await withEnv(STRICT, () => waitOn({ ...FAST, resources: [`socket:${socketPath}`] }));
    expect(lastCall()).to.deep.equal({ fn: 'socketCheck', path: socketPath });
  });

  it('should dispatch from the CLI and record the call in the log file', async function () {
    const port = await getFreePort();
    const socketPath = socketPathIn(tempDir());
    const cells = [
      [`tcp:127.0.0.1:${port}`, { fn: 'tcpCheck', host: '127.0.0.1', port, timeoutMs: 300 }],
      [`socket:${socketPath}`, { fn: 'socketCheck', path: socketPath }]
    ];
    for (const [resource, expected] of cells) {
      const log = path.join(tempDir(), 'calls.jsonl');
      const r = runCLI({ ...STRICT, WAIT_ON_FAKE_ADDON_LOG: log }, [resource, '-t', '1000', '-i', '100']);
      expect(r.code, r.stderr).to.equal(0);
      const lines = fs.readFileSync(log, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
      expect(lines).to.deep.equal([expected]);
    }
  });

  it('should resolve reverse tcp and reverse socket when the addon answers not-ready', async function () {
    const port = await getFreePort();
    const resources = [`tcp:127.0.0.1:${port}`, `socket:${socketPathIn(tempDir())}`];
    await withEnv({ ...STRICT, WAIT_ON_FAKE_ADDON_ANSWER: 'refused' }, () =>
      waitOn({ ...FAST, resources, reverse: true })
    );
    expect(fixture.calls.map((c) => c.fn)).to.include.members(['tcpCheck', 'socketCheck']);
  });

  it('should time out reverse tcp and reverse socket when the addon answers ready', async function () {
    const port = await getFreePort();
    for (const resource of [`tcp:127.0.0.1:${port}`, `socket:${socketPathIn(tempDir())}`]) {
      let err;
      await withEnv(STRICT, () => waitOn({ ...FAST, resources: [resource], reverse: true, timeout: 300 })).catch(
        (e) => (err = e)
      );
      expect(err, resource).to.be.an('error');
      expect(err.message).to.include('Timed out');
    }
    expect(fixture.calls.map((c) => c.fn)).to.include.members(['tcpCheck', 'socketCheck']);
  });

  it('should print the existing verbose lines from the addon result', async function () {
    const port = await getFreePort();
    const socketPath = socketPathIn(tempDir());
    const tcp = `tcp:127.0.0.1:${port}`;
    const socket = `socket:${socketPath}`;

    const ready = await verboseLines(STRICT, { resources: [tcp, socket] });
    expect(ready).to.include(`  TCP connection successful to host:127.0.0.1 port:${port}`);
    expect(ready).to.include(`  connected to socket:${socketPath}`);

    const timedOut = await verboseLines(
      { ...STRICT, WAIT_ON_FAKE_ADDON_ANSWER: 'timeout' },
      { resources: [tcp], tcpTimeout: 450, reverse: true }
    );
    expect(timedOut).to.include(`  timed out connecting to TCP host:127.0.0.1 port:${port} tcpTimeout:450ms`);

    const refused = await verboseLines(
      { ...STRICT, WAIT_ON_FAKE_ADDON_ANSWER: 'refused' },
      { resources: [tcp, socket], reverse: true }
    );
    expect(refused).to.include(`  error connecting to TCP host:127.0.0.1 port:${port} fake refused`);
    expect(refused).to.include(`  error connecting to socket socket:${socketPath} fake refused`);
  });

  for (const engine of ['rust', 'rust-strict']) {
    it(`should fall back to the JS checks when the addon lacks tcpCheck and socketCheck under ${engine}`, async function () {
      await withListeners((resources) =>
        withEnv({ WAIT_ON_ENGINE: engine, WAIT_ON_NATIVE_LIBRARY_PATH: PLAIN_ADDON }, () =>
          waitOn({ ...FAST, resources })
        )
      );
    });
  }

  it('should never call the addon under js', async function () {
    await withListeners((resources) =>
      withEnv({ WAIT_ON_ENGINE: 'js', WAIT_ON_NATIVE_LIBRARY_PATH: CHECKS_ADDON }, () => waitOn({ ...FAST, resources }))
    );
    expect(fixture.calls).to.deep.equal([]);
  });
});
