'use strict';

// Engine selection (KD-S1): WAIT_ON_ENGINE picks js (default), rust (addon with
// silent JS fallback) or rust-strict (addon load failure is an error).
// WAIT_ON_NATIVE_LIBRARY_PATH points the loader at a specific addon file.

const childProcess = require('child_process');
const https = require('https');
const fs = require('fs');
const http = require('http');
const net = require('net');
const os = require('os');
const path = require('path');
const util = require('util');
const { describe, it, before, after, afterEach, beforeEach } = require('mocha');
const { expect } = require('chai');

const waitOn = require('../lib/wait-on');
const { resolveEngine, prebuildDir, isMusl, addonPath } = require('../lib/engine');
const counting = require('./fixtures/counting-addon');
const { routesHttpToRust } = waitOn._internal;

const { withEnv, runCLI: runCLIWith } = require('./helpers/engine-env');
const tlsFixture = require('./helpers/tls-fixture');
const stubProxy = require('./helpers/stub-proxy');

const REPO_ROOT = path.resolve(__dirname, '..');
const FIXTURE_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');
const COUNTING_ADDON = path.join(__dirname, 'fixtures', 'counting-addon.js');
const POISON = path.join(os.tmpdir(), `wait-on-no-such-addon-${process.pid}`, 'wait-on.node');
const OPTS = { resources: [__filename], timeout: 1000, interval: 100, window: 100 };

function runCLI(vars, resource = __filename) {
  return runCLIWith(vars, [resource, '-t', '1000', '-i', '100', '-w', '100']);
}

// Run fn with console.log captured (log/verbose output binds it when waitOn starts).
async function captureLog(fn) {
  const lines = [];
  const original = console.log;
  console.log = (...args) => lines.push(util.format(...args));
  try {
    await fn();
  } finally {
    console.log = original;
  }
  return lines;
}

function callbackError(vars) {
  return withEnv(vars, () => new Promise((resolve) => waitOn(OPTS, resolve)));
}

function workspaceVersion() {
  const toml = fs.readFileSync(path.join(REPO_ROOT, 'Cargo.toml'), 'utf8');
  return /^version\s*=\s*"([^"]+)"/m.exec(toml)[1];
}

describe('engine selection', function () {
  this.timeout(5000);
  let junkDir;
  let junkAddon;
  let noWaitAddon;

  before(function () {
    junkDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-junk-'));
    junkAddon = path.join(junkDir, 'wait-on.node');
    fs.writeFileSync(junkAddon, 'not a native addon');
    noWaitAddon = path.join(junkDir, 'no-wait.js'); // a stale addon: loads, but has no wait export
    fs.writeFileSync(noWaitAddon, "module.exports = { version: () => 'stale' };");
  });

  after(function () {
    fs.rmSync(junkDir, { recursive: true, force: true });
  });

  describe('js engine', function () {
    for (const value of [undefined, '', 'js']) {
      it(`should use the JS engine and never touch the addon when WAIT_ON_ENGINE is ${JSON.stringify(value)}`, async function () {
        const vars = { WAIT_ON_ENGINE: value, WAIT_ON_NATIVE_LIBRARY_PATH: POISON };
        await withEnv(vars, () => waitOn(OPTS));
        expect(resolveEngine(vars)).to.deep.equal({ engine: 'js', addon: null, loadError: null });
      });
    }

    it('should exit 0 from the CLI under js with a poison addon path', function () {
      const r = runCLI({ WAIT_ON_ENGINE: 'js', WAIT_ON_NATIVE_LIBRARY_PATH: POISON });
      expect(r.code).to.equal(0);
      expect(r.stderr).to.equal('');
    });
  });

  describe('rust engine', function () {
    it('should fall back to JS silently when the addon is missing', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust', WAIT_ON_NATIVE_LIBRARY_PATH: POISON };
      await withEnv(vars, () => waitOn(OPTS));
      const r = resolveEngine(vars);
      expect(r.engine).to.equal('js');
      expect(r.addon).to.equal(null);
      expect(r.loadError.code).to.equal('MODULE_NOT_FOUND');
    });

    it('should fall back when the addon file exists but cannot be loaded', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust', WAIT_ON_NATIVE_LIBRARY_PATH: junkAddon };
      await withEnv(vars, () => waitOn(OPTS));
      const r = resolveEngine(vars);
      expect(r.engine).to.equal('js');
      expect(r.loadError).to.be.an('error');
    });

    it('should exit 0 from the CLI with the same output as js when the addon is missing', function () {
      const rust = runCLI({ WAIT_ON_ENGINE: 'rust', WAIT_ON_NATIVE_LIBRARY_PATH: POISON });
      const js = runCLI({ WAIT_ON_ENGINE: 'js', WAIT_ON_NATIVE_LIBRARY_PATH: POISON });
      expect(rust.code).to.equal(0);
      expect(rust.stdout).to.equal(js.stdout);
      expect(rust.stderr).to.equal(js.stderr);
    });

    it('should fall back to JS with a loadError naming wait when the addon has no wait export', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust', WAIT_ON_NATIVE_LIBRARY_PATH: noWaitAddon };
      await withEnv(vars, () => waitOn(OPTS));
      const r = resolveEngine(vars);
      expect(r.engine).to.equal('js');
      expect(r.addon).to.equal(null);
      expect(r.loadError.message).to.include('wait');
    });

    it('should load a present addon under rust', function () {
      const r = resolveEngine({ WAIT_ON_ENGINE: 'rust', WAIT_ON_NATIVE_LIBRARY_PATH: FIXTURE_ADDON });
      expect(r.engine).to.equal('rust');
      expect(r.addon.version()).to.equal('fake');
    });
  });

  describe('rust-strict engine', function () {
    it('should reject when the addon is missing', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: POISON };
      let err;
      await withEnv(vars, () => waitOn(OPTS)).catch((e) => (err = e));
      expect(err).to.be.an('error');
      expect(err.message).to.include('rust-strict');
      expect(err.message).to.include(POISON);
      expect(err.message).to.include('Cannot find module');
    });

    it('should deliver the load error to the callback without throwing', async function () {
      const err = await callbackError({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: POISON });
      expect(err).to.be.an('error');
      expect(err.message).to.include(POISON);
    });

    it('should reject when the addon file cannot be loaded', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: junkAddon };
      let err;
      await withEnv(vars, () => waitOn(OPTS)).catch((e) => (err = e));
      expect(err).to.be.an('error');
      expect(err.message).to.include(junkAddon);
    });

    it('should deliver an error naming the path and wait to the callback when the addon has no wait export', async function () {
      const err = await callbackError({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: noWaitAddon });
      expect(err).to.be.an('error');
      expect(err.message).to.equal(`WAIT_ON_ENGINE=rust-strict: the native addon at ${noWaitAddon} has no wait export`);
    });

    it('should exit 1 from the CLI naming the path and wait when the addon has no wait export', function () {
      const r = runCLI({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: noWaitAddon });
      expect(r.code).to.equal(1);
      expect(r.stderr).to.include(`WAIT_ON_ENGINE=rust-strict: the native addon at ${noWaitAddon} has no wait export`);
    });

    it('should exit 1 from the CLI naming the engine and path when the addon is missing', function () {
      const r = runCLI({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: POISON });
      expect(r.code).to.equal(1);
      expect(r.stdout).to.equal('');
      expect(r.stderr).to.include('rust-strict');
      expect(r.stderr).to.include(POISON);
    });

    it('should take the addon-present branch with a fixture addon', async function () {
      const vars = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: FIXTURE_ADDON };
      await withEnv(vars, () => waitOn(OPTS));
      const first = resolveEngine(vars);
      expect(first.engine).to.equal('rust');
      expect(first.addon.version()).to.equal('fake');
      expect(resolveEngine(vars).addon).to.equal(first.addon);
    });

    it('should exit 0 from the CLI with a loadable fixture addon', function () {
      const r = runCLI({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: FIXTURE_ADDON });
      expect(r.code).to.equal(0);
    });

    it('should load the real addon and answer version() when a host prebuild exists', async function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const vars = { WAIT_ON_ENGINE: 'rust-strict' };
      await withEnv(vars, () => waitOn(OPTS));
      const { engine, addon } = resolveEngine(vars);
      expect(engine).to.equal('rust');
      expect(addon.version()).to.equal(workspaceVersion());
      expect(addon.noop()).to.equal(undefined);
    });

    it('should answer fileSize from the built addon when a host prebuild exists', async function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const addon = require(addonPath({}));
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-filesize-'));
      try {
        const file = path.join(dir, 'f');
        fs.writeFileSync(file, '12345');
        const pending = addon.fileSize(file);
        expect(pending).to.be.an.instanceof(Promise);
        expect(await pending).to.equal(5);
        expect(await addon.fileSize(path.join(dir, 'missing'))).to.equal(-1);
      } finally {
        fs.rmSync(dir, { recursive: true, force: true });
      }
    });

    it('should answer runCommand from the built addon when a host prebuild exists', async function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const addon = require(addonPath({}));
      const pending = addon.runCommand('node -e "process.exit(0)"', 0);
      expect(pending).to.be.an.instanceof(Promise);
      expect((await pending).ok).to.equal(true);
      const failed = await addon.runCommand('node -e "console.error(\'boom\'); process.exit(3)"', 0);
      expect(failed.ok).to.equal(false);
      expect(failed.error).to.match(/^Command failed: /);
      expect(failed.error).to.include('boom');
    });

    it('should keep answering a file: probe while five slow commands run under the built addon', async function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      // Five 3s commands outnumber the 4 libuv threadpool threads; fileSize runs on that
      // pool, so the file resolves only if runCommand keeps its attempts off the pool.
      const slow = [1, 2, 3, 4, 5].map((n) => `command:node -e "setTimeout(function () {}, 3000)" ${n}`);
      let err;
      await withEnv({ WAIT_ON_ENGINE: 'rust-strict' }, () =>
        waitOn({ resources: [...slow, __filename], timeout: 1500, interval: 100, window: 100 })
      ).catch((e) => (err = e));
      expect(err.message).to.match(/^Timed out waiting for/);
      expect(err.message).to.include(slow[4].slice('command:'.length));
      expect(err.message).to.not.include(__filename);
    });
  });

  describe('real addon HttpChecker', function () {
    let addon;
    const closers = [];

    before(function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      addon = require(addonPath({}));
    });

    afterEach(function () {
      while (closers.length) closers.pop()();
    });

    // Listens on an ephemeral port; counts accepted sockets and destroys them on cleanup.
    function listen(server) {
      const sockets = new Set();
      server.on('connection', (s) => sockets.add(s));
      closers.push(() => {
        for (const s of sockets) s.destroy();
        server.close();
      });
      return new Promise((resolve) =>
        server.listen(0, '127.0.0.1', () =>
          resolve({ url: `http://127.0.0.1:${server.address().port}/`, sockets })
        )
      );
    }
    const okServer = () => listen(http.createServer((req, res) => res.end()));
    const hungServer = () => listen(net.createServer(() => {}));

    function checker(url, extra = {}) {
      return new addon.HttpChecker({ url, method: 'HEAD', headers: {}, followRedirect: true, ...extra });
    }

    it('should export an HttpChecker class with check and cancel', function () {
      expect(addon.HttpChecker).to.be.a('function');
      expect(addon.HttpChecker.prototype.check).to.be.a('function');
      expect(addon.HttpChecker.prototype.cancel).to.be.a('function');
    });

    it('should resolve ok with the status when the server answers 200', async function () {
      const { url } = await okServer();
      const r = await checker(url).check();
      expect(r.ok).to.equal(true);
      expect(r.status).to.equal(200);
    });

    it('should resolve not ok when validateStatus rejects the status', async function () {
      const { url } = await okServer();
      const r = await checker(url).check((s) => s === 500);
      expect(r).to.include({ ok: false, status: 200 });
    });

    it('should resolve not ok and keep the process alive when validateStatus throws', async function () {
      const { url } = await okServer();
      const r = await checker(url).check(() => {
        throw new Error('boom');
      });
      expect(r).to.include({ ok: false, status: 200 });
    });

    it('should settle every in-flight and later check not ok after one cancel', async function () {
      const { url } = await hungServer();
      const c = checker(url);
      const inflight = [c.check(), c.check()];
      await new Promise((resolve) => setTimeout(resolve, 100));
      const t0 = Date.now();
      c.cancel();
      const results = await Promise.all(inflight);
      expect(Date.now() - t0).to.be.below(500);
      for (const r of results) expect(r).to.include({ ok: false, error: 'cancelled' });
      const t1 = Date.now();
      expect(await c.check()).to.include({ ok: false, error: 'cancelled' });
      expect(Date.now() - t1).to.be.below(100);
    });

    it('should settle not ok when timeoutMs elapses against a hung server', async function () {
      const { url } = await hungServer();
      const r = await checker(url, { timeoutMs: 50 }).check();
      expect(r.ok).to.equal(false);
      expect(r.error).to.be.a('string');
    });

    it('should reuse one connection across sequential checks', async function () {
      const { url, sockets } = await okServer();
      const c = checker(url);
      expect((await c.check()).ok).to.equal(true);
      expect((await c.check()).ok).to.equal(true);
      expect(sockets.size).to.equal(1);
    });

    it('should send the request through the proxy when proxy is set', async function () {
      const { url } = await okServer();
      const proxy = await stubProxy.start();
      closers.push(() => proxy.close());
      expect((await checker(url, { proxy: proxy.url }).check()).ok).to.equal(true);
      expect(proxy.requests.map((r) => r.line)).to.deep.equal([`HEAD ${url}`]);
    });

    it('should construct and settle not ok with an error when roots do not parse', async function () {
      const fx = tlsFixture();
      if (!fx) this.skip();
      closers.push(fx.cleanup);
      const server = https.createServer({ key: fx.key, cert: fx.cert }, (req, res) => res.end());
      const { url } = await listen(server);
      const httpsUrl = url.replace('http:', 'https:');
      expect((await checker(httpsUrl).check()).ok).to.equal(true); // reachable without roots
      const bad = '-----BEGIN CERTIFICATE-----\nnot base64!\n-----END CERTIFICATE-----\n';
      const r = await checker(httpsUrl, { roots: [bad] }).check();
      expect(r.ok).to.equal(false);
      expect(r.error).to.be.a('string');
    });

    it('should connect over the unix socket (named pipe on Windows) when socketPath is set', async function () {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-addon-sock-'));
      closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
      const sock = process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, 'sock') : path.join(dir, 'sock');
      const seen = [];
      const server = http.createServer((req, res) => {
        seen.push(req.url);
        res.end();
      });
      closers.push(() => server.close());
      await new Promise((resolve) => server.listen(sock, resolve));
      expect((await checker('http://localhost/', { socketPath: sock }).check()).ok).to.equal(true);
      expect(seen).to.deep.equal(['/']);
    });
  });

  describe('real addon wait', function () {
    let addon;

    before(function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      addon = require(addonPath({}));
    });

    const BASE = { delayMs: 0, intervalMs: 50, windowMs: 0, tcpTimeoutMs: 300, commandTimeoutMs: 0, reverse: false, verbose: false };

    it('should resolve ok after delivering the waiting line when the file is ready', async function () {
      const lines = [];
      const r = await addon.wait(
        { ...BASE, timeoutMs: 2000, resources: [{ name: __filename, kind: 'file', path: __filename }] },
        (line) => lines.push(line)
      );
      expect(lines).to.include(`waiting for 1 resources: ${__filename}`);
      expect(r).to.deep.equal({ ok: true, error: null });
    });

    describe('with servers', function () {
      const closers = [];
      afterEach(function () {
        while (closers.length) closers.pop()();
      });

      function listen(server, at) {
        closers.push(() => server.close());
        return new Promise((resolve) => server.listen(...at, () => resolve(server.address())));
      }
      const httpSpec = (port) => ({ url: `http://127.0.0.1:${port}/`, method: 'HEAD', headers: {}, followRedirect: true });

      it('should resolve ok when one ready resource of each kind is waited on', async function () {
        const { port: httpPort } = await listen(
          http.createServer((req, res) => res.end()),
          [0, '127.0.0.1']
        );
        const { port: tcpPort } = await listen(net.createServer((s) => s.destroy()), [0, '127.0.0.1']);
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-addon-wait-'));
        closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
        const sock = process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, 'sock') : path.join(dir, 'sock');
        await listen(net.createServer((s) => s.destroy()), [sock]);
        const r = await addon.wait({
          ...BASE,
          timeoutMs: 5000,
          resources: [
            { name: __filename, kind: 'file', path: __filename },
            { name: 'http', kind: 'http', http: httpSpec(httpPort) },
            { name: 'tcp', kind: 'tcp', path: `127.0.0.1:${tcpPort}`, host: '127.0.0.1', port: tcpPort },
            { name: 'socket', kind: 'socket', path: sock },
            { name: 'command', kind: 'command', command: 'exit 0' }
          ]
        });
        expect(r).to.deep.equal({ ok: true, error: null });
      });

      it('should decide http readiness with validateStatus', async function () {
        const { port } = await listen(
          http.createServer((req, res) => res.end()),
          [0, '127.0.0.1']
        );
        const spec = { ...BASE, timeoutMs: 300, resources: [{ name: 'h', kind: 'http', http: httpSpec(port) }] };
        expect(await addon.wait(spec, undefined, (s) => s === 500)).to.deep.equal({
          ok: false,
          error: 'Timed out waiting for: h'
        });
        expect(await addon.wait(spec, undefined, (s) => s === 200)).to.deep.equal({ ok: true, error: null });
      });
    });

    it('should resolve the exact timeout message when tcp never connects', async function () {
      const r = await addon.wait({
        ...BASE,
        timeoutMs: 300,
        resources: [{ name: 'tcp:127.0.0.1:1', kind: 'tcp', path: '127.0.0.1:1', host: '127.0.0.1', port: 1 }]
      });
      expect(r).to.deep.equal({ ok: false, error: 'Timed out waiting for: tcp:127.0.0.1:1' });
    });

    it('should resolve with timeoutMs, simultaneous and log all absent', async function () {
      const r = await addon.wait({ ...BASE, resources: [{ name: __filename, kind: 'file', path: __filename }] });
      expect(r).to.deep.equal({ ok: true, error: null });
    });

    it('should throw on an unknown resource kind', function () {
      expect(() => addon.wait({ ...BASE, resources: [{ name: 'x', kind: 'bogus' }] })).to.throw(
        'unknown resource kind: bogus'
      );
    });
  });

  describe('http routing (counting addon)', function () {
    const NO_PROXY_ENV = {
      HTTP_PROXY: undefined,
      http_proxy: undefined,
      HTTPS_PROXY: undefined,
      https_proxy: undefined,
      NO_PROXY: undefined,
      no_proxy: undefined
    };
    const RUST = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: COUNTING_ADDON, ...NO_PROXY_ENV };
    const FAST = { timeout: 2000, interval: 100 };
    const closers = [];
    let fx;
    let cert;
    let key;

    before(function () {
      this.timeout(30000);
      fx = tlsFixture();
      if (fx) ({ key, cert } = fx); // else https cells skip below
    });

    after(function () {
      if (fx) fx.cleanup();
    });

    beforeEach(function () {
      counting.reset();
    });

    afterEach(function () {
      while (closers.length) closers.pop()();
    });

    function listen(server, ...listenArgs) {
      closers.push(() => {
        server.closeAllConnections();
        server.close();
      });
      return new Promise((resolve) => server.listen(...listenArgs, () => resolve(server.address())));
    }
    const handler = (req, res) => res.end('ok');
    const httpPort = async () => (await listen(http.createServer(handler), 0, 'localhost')).port;
    const httpsPort = async () => (await listen(https.createServer({ key, cert }, handler), 0, 'localhost')).port;

    const waits = () => counting.calls.filter((c) => c.type === 'wait');
    // the http options of every http resource in the recorded wait specs
    const httpOpts = () => waits().flatMap((c) => c.spec.resources.filter((r) => r.kind === 'http').map((r) => r.http));

    // 'resolved' or the rejection message, so both engines' outcomes compare directly.
    function outcome(vars, opts) {
      return withEnv(vars, () => waitOn({ ...FAST, ...opts }).then(() => 'resolved', (e) => e.message));
    }

    it('should make one wait call carrying a HEAD and a GET http resource when http resources run under rust', async function () {
      const port = await httpPort();
      const resources = [`http://localhost:${port}/`, `http-get://localhost:${port}/`];
      expect(await outcome(RUST, { resources })).to.equal('resolved');
      expect(waits()).to.have.length(1);
      expect(httpOpts().map((o) => o.method)).to.deep.equal(['HEAD', 'GET']);
    });

    it('should route a default https resource to the addon when strictSSL is unset', async function () {
      if (!cert) this.skip();
      const port = await httpsPort();
      expect(await outcome(RUST, { resources: [`https://localhost:${port}/`] })).to.equal('resolved');
      expect(httpOpts()).to.have.length(1);
      expect(httpOpts()[0].url).to.equal(`https://localhost:${port}/`);
    });

    it('should pass string headers with auth folded in and a clamped timeoutMs when options are set', async function () {
      const port = await httpPort();
      const opts = {
        resources: [`http://localhost:${port}/`],
        headers: { 'X-Num': 42, Authorization: 'Bearer OLD' },
        auth: { username: 'u', password: 'p' },
        httpTimeout: 2 ** 33,
        followRedirect: false
      };
      expect(await outcome(RUST, opts)).to.equal('resolved');
      expect(httpOpts()[0]).to.deep.equal({
        url: `http://localhost:${port}/`,
        method: 'HEAD',
        headers: { 'X-Num': '42', authorization: 'Basic ' + Buffer.from('u:p').toString('base64') },
        followRedirect: false,
        timeoutMs: 2 ** 32 - 1
      });
    });

    it('should hand the addon a validateStatus wrapper returning real booleans when validateStatus is truthy, false or throws', async function () {
      const port = await httpPort();
      const resources = [`http://localhost:${port}/`];
      expect(await outcome(RUST, { resources, validateStatus: () => 1 })).to.equal('resolved');
      expect(waits()[0].validateStatus(200)).to.equal(true);
      counting.reset();
      expect(await outcome(RUST, { resources, validateStatus: (s) => s === 200 })).to.equal('resolved');
      expect(waits()[0].validateStatus(204)).to.equal(false);
      counting.reset();
      const thrower = () => {
        throw new Error('boom');
      };
      expect(await outcome(RUST, { resources, validateStatus: thrower, timeout: 300 })).to.match(/Timed out/);
      expect(waits()[0].validateStatus(200)).to.equal(false);
    });

    it('should deliver a rejected wait to the callback and log the non-timeout exit line', async function () {
      counting.constructError = new Error('client build failed');
      let err;
      const lines = await captureLog(() =>
        withEnv(RUST, () => new Promise((resolve) => waitOn({ resources: ['http://localhost:1/'], ...FAST, log: true }, resolve))).then(
          (e) => (err = e)
        )
      );
      expect(err.message).to.equal('client build failed');
      expect(waits()).to.have.length(1);
      // util.format prints the Error with its stack after the message
      expect(lines.some((l) => l.startsWith(`wait-on(${process.pid}) exiting with error Error: client build failed`))).to.equal(true);
    });

    describe('L5 cells route to the addon', function () {
      const dead = 'http://127.0.0.1:1';
      // both spellings: Windows env names are case-insensitive
      const DEAD_ENV = { HTTP_PROXY: dead, http_proxy: dead };
      const constructOpts = () => {
        expect(httpOpts()).to.have.length(1);
        return httpOpts()[0];
      };

      it('should pass socketPath, a localhost url and no proxy when the resource is http://unix: and HTTP_PROXY is set', async function () {
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-route-sock-'));
        closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
        const sock = process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, 'sock') : path.join(dir, 'sock');
        await listen(http.createServer(handler), sock);
        expect(await outcome({ ...RUST, ...DEAD_ENV }, { resources: [`http://unix:${sock}:/`] })).to.equal('resolved');
        const opts = constructOpts();
        expect(opts).to.include({ socketPath: sock, url: 'http://localhost/' });
        expect(opts).to.not.have.property('proxy');
      });

      it('should pass roots, cert and key on an http target when ca, strictSSL, cert and key are set', async function () {
        if (!cert) this.skip();
        const port = await httpPort();
        const opts = { resources: [`http://localhost:${port}/`], ca: 'unused-for-http', strictSSL: true, cert, key };
        expect(await outcome(RUST, opts)).to.equal('resolved');
        expect(constructOpts()).to.deep.include({ roots: ['unused-for-http'], cert: cert.toString() });
        expect(constructOpts().key).to.match(/^-----BEGIN PRIVATE KEY-----/);
      });

      it('should pass a decrypted PKCS#8 key when an encrypted key and its passphrase are set', async function () {
        if (!cert) this.skip();
        const port = await httpPort();
        const opts = { resources: [`http://localhost:${port}/`], cert, key: fx.encryptedKey, passphrase: fx.passphrase };
        await outcome(RUST, opts);
        expect(constructOpts().key).to.match(/^-----BEGIN PRIVATE KEY-----/);
      });

      it('should pass the key unchanged when the passphrase is wrong', async function () {
        if (!cert) this.skip();
        const port = await httpPort();
        const opts = { resources: [`http://localhost:${port}/`], cert, key: fx.encryptedKey, passphrase: 'wrong' };
        await outcome(RUST, opts);
        expect(constructOpts().key).to.equal(fx.encryptedKey.toString());
      });

      it('should pass the default roots when strictSSL is true without ca', async function () {
        if (!cert) this.skip();
        const port = await httpsPort();
        await outcome(RUST, { resources: [`https://localhost:${port}/`], strictSSL: true, timeout: 600 });
        expect(constructOpts().roots).to.have.length.above(1);
      });

      it('should pass only the ca as roots when strictSSL is true with ca', async function () {
        if (!cert) this.skip();
        const port = await httpsPort();
        await outcome(RUST, { resources: [`https://localhost:${port}/`], strictSSL: true, ca: cert, timeout: 600 });
        expect(constructOpts().roots).to.deep.equal([cert.toString()]);
      });

      it('should pass no proxy when proxy is false and HTTP_PROXY is set', async function () {
        const port = await httpPort();
        expect(await outcome({ ...RUST, ...DEAD_ENV }, { resources: [`http://localhost:${port}/`], proxy: false })).to.equal('resolved');
        expect(constructOpts()).to.not.have.property('proxy');
      });

      it('should pass the env proxy when HTTP_PROXY is set', async function () {
        const port = await httpPort();
        await outcome({ ...RUST, ...DEAD_ENV }, { resources: [`http://localhost:${port}/`], timeout: 600 });
        expect(constructOpts().proxy).to.equal(dead);
      });

      it('should pass the normalized proxy URI with percent-encoded credentials when a proxy object has auth', async function () {
        const port = await httpPort();
        const proxy = { host: '::1', port: 1, auth: { username: 'u', password: 'p@:/' } };
        await outcome(RUST, { resources: [`http://localhost:${port}/`], proxy, timeout: 600 });
        expect(constructOpts().proxy).to.equal('http://u:p%40%3A%2F@[::1]:1');
      });
    });

    describe('KTD10 carve-outs stay on the JS check', function () {
      const cells = [
        ['an https target selects HTTPS_PROXY', 'https', { HTTPS_PROXY: 'http://127.0.0.1:1', https_proxy: 'http://127.0.0.1:1' }, {}],
        ['an https target falls back to HTTP_PROXY', 'https', { HTTP_PROXY: 'http://127.0.0.1:1', http_proxy: 'http://127.0.0.1:1' }, {}],
        ['the proxy object protocol is https', 'http', {}, { proxy: { host: '127.0.0.1', port: 1, protocol: 'https' } }],
        ['the proxy object protocol is socks5', 'http', {}, { proxy: { host: '127.0.0.1', port: 1, protocol: 'socks5' } }],
        ['HTTPS_PROXY is an https URL', 'http', { HTTPS_PROXY: 'https://127.0.0.1:1', https_proxy: 'https://127.0.0.1:1' }, {}]
      ];
      for (const [label, scheme, env, extra] of cells) {
        it(`should construct no checker when ${label}`, async function () {
          const port = scheme === 'https' ? (cert ? await httpsPort() : this.skip()) : await httpPort();
          await outcome({ ...RUST, ...env }, { resources: [`${scheme}://localhost:${port}/`], timeout: 600, ...extra });
          expect(counting.calls).to.have.length(0);
        });
      }

      it('should deliver a malformed env proxy construction error to the callback on both engines even when NO_PROXY exempts the target', async function () {
        const vars = { HTTP_PROXY: 'proxy.corp:3128', http_proxy: 'proxy.corp:3128', NO_PROXY: 'localhost', no_proxy: 'localhost' };
        const opts = { resources: ['https://localhost:1/'], timeout: 600 };
        const js = await outcome({ WAIT_ON_ENGINE: 'js', ...NO_PROXY_ENV, ...vars }, opts);
        expect(js).to.match(/Invalid URL protocol/);
        expect(await outcome({ ...RUST, ...vars }, opts)).to.equal(js);
        expect(counting.calls).to.have.length(0);
      });
    });

    it('should time out like JS without constructing a checker when the url has userinfo', async function () {
      const port = await httpPort();
      const opts = { resources: [`http://u:p@localhost:${port}/`], timeout: 600 };
      const js = await outcome({ WAIT_ON_ENGINE: 'js', ...NO_PROXY_ENV }, opts);
      expect(js).to.match(/Timed out/);
      expect(await outcome(RUST, opts)).to.equal(js);
      expect(waits()).to.have.length(0);
    });

    describe('routesHttpToRust', function () {
      const base = { addon: {}, validatedOpts: { strictSSL: false }, socketPath: undefined, env: {}, url: 'http://localhost:1/' };
      const https = { url: 'https://localhost:1/' };
      const proxyObj = (extra) => ({ validatedOpts: { strictSSL: false, proxy: { host: 'h', port: 1, ...extra } } });
      const toJs = [
        ['no addon is loaded', { addon: null }],
        ['the url has userinfo', { url: 'http://u:p@localhost:1/' }],
        ['an https target selects HTTPS_PROXY', { ...https, env: { HTTPS_PROXY: 'http://p:1' } }],
        ['an https target selects https_proxy', { ...https, env: { https_proxy: 'http://p:1' } }],
        ['an https target falls back to HTTP_PROXY', { ...https, env: { HTTP_PROXY: 'http://p:1' } }],
        ['the proxy object protocol is https', proxyObj({ protocol: 'https:' })],
        ['the proxy object protocol is socks5', proxyObj({ protocol: 'socks5' })],
        ['the proxy object host cannot form a URL', proxyObj({ host: 'bad host' })],
        ['HTTPS_PROXY is an https URL on an http target', { env: { HTTPS_PROXY: 'https://p:1' } }],
        ['HTTP_PROXY is malformed and NO_PROXY exempts the target', { env: { HTTP_PROXY: 'proxy.corp:3128', NO_PROXY: 'localhost' } }]
      ];
      for (const [label, override] of toJs) {
        it(`should route to JS when ${label}`, function () {
          expect(routesHttpToRust({ ...base, ...override })).to.equal(false);
        });
      }

      const toRust = [
        ['no L5 condition holds', {}],
        ['a unix socketPath is set with a malformed HTTP_PROXY', { socketPath: '/tmp/sock', env: { HTTP_PROXY: 'nope' } }],
        ['TLS options and strictSSL are set', { validatedOpts: { strictSSL: true, ca: 'x', cert: 'x', key: 'x', passphrase: 'x' } }],
        ['proxy is false with an https HTTPS_PROXY', { validatedOpts: { strictSSL: false, proxy: false }, env: { HTTPS_PROXY: 'https://p:1' } }],
        ['the proxy object is http', proxyObj({ protocol: 'http:' })],
        ['an https target has a proxy object', { ...https, ...proxyObj({}) }],
        ['an http target has HTTP_PROXY', { env: { HTTP_PROXY: 'http://p:1' } }],
        ['an https target is exempted by NO_PROXY', { ...https, env: { HTTPS_PROXY: 'http://p:1', NO_PROXY: 'localhost' } }]
      ];
      for (const [label, override] of toRust) {
        it(`should route to Rust when ${label}`, function () {
          expect(routesHttpToRust({ ...base, ...override })).to.equal(true);
        });
      }
    });
  });

  // AE-L4-4 / R-L4-9: an API caller's process exits once waitOn settles, even with a
  // request in flight to a server that never answers. Real clock, subprocess.
  // L7 U6: front-door output of the Rust loop under the real addon. Real clock.
  describe('real addon loop at the front door', function () {
    this.timeout(6000);
    const hasAddon = () => fs.existsSync(addonPath({}));
    const STRICT_REAL = { WAIT_ON_ENGINE: 'rust-strict' };
    const message = (vars, opts) => withEnv(vars, () => waitOn(opts).then(() => 'resolved', (e) => e.message));

    it('should reject with the identical timeout message on js and rust-strict for a missing file', async function () {
      const file = path.join(os.tmpdir(), `wait-on-missing-${process.pid}`);
      const opts = { resources: [file], timeout: 300, interval: 50 };
      const js = await message({ WAIT_ON_ENGINE: 'js' }, opts);
      expect(js).to.equal(`Timed out waiting for: ${file}`);
      if (hasAddon()) expect(await message(STRICT_REAL, opts)).to.equal(js);
    });

    describe('verbose lines', function () {
      const closers = [];
      before(function () {
        if (!hasAddon()) this.skip();
      });
      afterEach(function () {
        while (closers.length) closers.pop()();
      });

      function listen(at) {
        const server = net.createServer((s) => s.destroy());
        closers.push(() => server.close());
        return new Promise((resolve) => server.listen(...at, () => resolve(server.address())));
      }
      const verboseLines = (opts) => captureLog(() => message(STRICT_REAL, { verbose: true, interval: 50, ...opts }));

      it('should log a successful TCP connection for a listening tcp port', async function () {
        const { port } = await listen([0, '127.0.0.1']);
        const lines = await verboseLines({ resources: [`tcp:127.0.0.1:${port}`], timeout: 2000 });
        expect(lines).to.include(`  TCP connection successful to host:127.0.0.1 port:${port}`);
      });

      it('should log a socket connection for a listening socket', async function () {
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-loop-sock-'));
        closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
        const sock = process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, 'sock') : path.join(dir, 'sock');
        await listen([sock]);
        const lines = await verboseLines({ resources: [`socket:${sock}`], timeout: 2000 });
        expect(lines).to.include(`  connected to socket:${sock}`);
      });

      it('should log a timed-out connect, or the OS error, for a black-holed tcp host', async function () {
        const lines = await verboseLines({ resources: ['tcp:10.255.255.1:9'], tcpTimeout: 200, timeout: 600 });
        expect(lines).to.include(`wait-on(${process.pid}) Timed out waiting for: tcp:10.255.255.1:9; exiting with error`);
        // a routed host drops the SYN until tcpTimeout; a host without a route fails at once
        const reasons = lines.filter((l) => l.startsWith('  timed out connecting to TCP') || l.includes('(os error'));
        expect(reasons).to.not.be.empty;
        if (!reasons[0].includes('(os error')) {
          expect(reasons[0]).to.equal('  timed out connecting to TCP host:10.255.255.1 port:9 tcpTimeout:200ms');
        }
      });
    });
  });

  describe('process lifetime (hung http server)', function () {
    this.timeout(10000);
    const HUNG_API = path.join(__dirname, 'fixtures', 'hung-http-api.js');

    function runHung(engine) {
      const env = { ...process.env, WAIT_ON_ENGINE: engine };
      delete env.WAIT_ON_NATIVE_LIBRARY_PATH;
      // 5 s budget: spawnSync kills the child at the timeout, leaving status null.
      return childProcess.spawnSync(process.execPath, [HUNG_API], { env, encoding: 'utf8', timeout: 5000 });
    }

    it('should exit after waitOn rejects under js', function () {
      const r = runHung('js');
      expect(r.status, r.stderr).to.equal(0);
      expect(r.stdout).to.include('settled');
    });

    it('should exit after waitOn rejects under rust-strict with a request in flight', function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const r = runHung('rust-strict');
      expect(r.status, r.stderr).to.equal(0);
      expect(r.stdout).to.include('settled');
    });
  });

  // KTD1: strictSSL without ca trusts what Node trusts by default, NODE_EXTRA_CA_CERTS
  // included (read at process start, so a subprocess). Real clock.
  describe('NODE_EXTRA_CA_CERTS with strictSSL and no ca', function () {
    this.timeout(30000);
    const EXTRA_CA_API = path.join(__dirname, 'fixtures', 'extra-ca-api.js');
    let fx;

    before(function () {
      fx = tlsFixture();
      if (!fx) this.skip();
    });

    after(function () {
      if (fx) fx.cleanup();
    });

    function runExtraCa(vars) {
      const env = { ...process.env, NODE_EXTRA_CA_CERTS: path.join(fx.dir, 'cert.pem') };
      delete env.WAIT_ON_NATIVE_LIBRARY_PATH;
      Object.assign(env, vars);
      const r = childProcess.spawnSync(process.execPath, [EXTRA_CA_API, fx.dir], { env, encoding: 'utf8', timeout: 10000 });
      expect(r.status, r.stderr).to.equal(0);
      return r.stdout.trim().split(' ');
    }

    it('should resolve under js', function () {
      expect(runExtraCa({ WAIT_ON_ENGINE: 'js' })).to.deep.equal(['resolved', '0']);
    });

    it('should resolve under rust-strict with one wait call to the addon', function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const [outcome, waits] = runExtraCa({ WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: COUNTING_ADDON });
      expect(outcome).to.equal('resolved');
      expect(Number(waits)).to.equal(1);
    });
  });

  describe('invalid value', function () {
    const vars = { WAIT_ON_ENGINE: 'nope', WAIT_ON_NATIVE_LIBRARY_PATH: POISON };

    it('should reject naming the value and the allowed values', async function () {
      let err;
      await withEnv(vars, () => waitOn(OPTS)).catch((e) => (err = e));
      expect(err.message).to.include('"nope"');
      expect(err.message).to.include('js, rust, rust-strict');
    });

    it('should deliver the error to the callback without throwing', async function () {
      const err = await callbackError(vars);
      expect(err.message).to.include('"nope"');
    });

    it('should exit 1 from the CLI with the message on stderr', function () {
      const r = runCLI(vars);
      expect(r.code).to.equal(1);
      expect(r.stderr).to.include('js, rust, rust-strict');
    });
  });

  // KTD6/KTD7: a loaded addon runs the whole wait in one wait call. fake-addon.js answers
  // ready (or the Rust timeout result) and records the spec, so no build is needed.
  describe('Rust shim (fake addon)', function () {
    const fake = require(FIXTURE_ADDON);
    const STRICT = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: FIXTURE_ADDON };
    const sock = path.join(os.tmpdir(), 'wait-on-fake.sock');
    const resources = [__filename, 'tcp:127.0.0.1:1', `socket:${sock}`, 'http-get://localhost:1/x', 'command:exit 0'];

    beforeEach(function () {
      fake.calls.length = 0;
    });

    it('should make one wait call with a spec for a file, tcp, socket, http and command resource', async function () {
      const opts = { resources, delay: 5, interval: 100, window: 5e9, timeout: 3e9, simultaneous: 5e9, verbose: true };
      await captureLog(() => withEnv(STRICT, () => waitOn(opts)));
      expect(fake.calls).to.deep.equal([
        {
          delayMs: 5,
          intervalMs: 100,
          windowMs: 2 ** 32 - 1,
          tcpTimeoutMs: 300,
          commandTimeoutMs: 0,
          simultaneous: 2 ** 32 - 1,
          timeoutMs: 1,
          reverse: false,
          verbose: true,
          resources: [
            { name: __filename, kind: 'file', path: __filename },
            { name: 'tcp:127.0.0.1:1', kind: 'tcp', path: '127.0.0.1:1', host: '127.0.0.1', port: 1 },
            { name: `socket:${sock}`, kind: 'socket', path: sock },
            {
              name: 'http-get://localhost:1/x',
              kind: 'http',
              http: { url: 'http://localhost:1/x', method: 'GET', headers: {}, followRedirect: true }
            },
            { name: 'command:exit 0', kind: 'command', command: 'exit 0' }
          ]
        }
      ]);
    });

    it('should reject with the Rust timeout message when the addon answers timed out', async function () {
      let err;
      await withEnv({ ...STRICT, WAIT_ON_FAKE_ADDON_ANSWER: 'timeout' }, () => waitOn({ resources })).catch((e) => (err = e));
      expect(err.message).to.equal(`Timed out waiting for: ${resources.join(', ')}`);
      expect(fake.calls).to.have.length(1);
      expect(fake.calls[0]).to.not.have.any.keys('timeoutMs', 'simultaneous');
    });

    it('should exit 1 from the CLI with the timeout message and record one wait call', function () {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-fake-log-'));
      try {
        const log = path.join(dir, 'calls.jsonl');
        const r = runCLIWith({ ...STRICT, WAIT_ON_FAKE_ADDON_ANSWER: 'timeout', WAIT_ON_FAKE_ADDON_LOG: log }, ['tcp:127.0.0.1:1']);
        expect(r.code).to.equal(1);
        expect(r.stderr).to.include('Timed out waiting for: tcp:127.0.0.1:1');
        const lines = fs.readFileSync(log, 'utf8').trim().split('\n');
        expect(lines).to.have.length(1);
        expect(JSON.parse(lines[0]).resources[0]).to.include({ kind: 'tcp', port: 1 });
      } finally {
        fs.rmSync(dir, { recursive: true, force: true });
      }
    });
  });

  // AE-L7-3 / KTD5: the engine is required lazily, so loading wait-on pulls in neither
  // rxjs nor undici until a JS-engine wait runs. Subprocess for a clean require.cache.
  describe('module graph', function () {
    this.timeout(10000);
    const LOADED =
      "const loaded = () => { const keys = Object.keys(require.cache).map((k) => k.split(require('path').sep).join('/'));" +
      " return { rxjs: keys.some((k) => k.includes('node_modules/rxjs/')), undici: keys.some((k) => k.includes('node_modules/undici/')) }; };";

    function runGraph(program, engine = 'js') {
      const env = { ...process.env, WAIT_ON_ENGINE: engine };
      delete env.WAIT_ON_NATIVE_LIBRARY_PATH;
      const r = childProcess.spawnSync(process.execPath, ['-e', LOADED + program], { cwd: REPO_ROOT, env, encoding: 'utf8', timeout: 8000 });
      expect(r.status, r.stderr).to.equal(0);
      return JSON.parse(r.stdout.trim().split('\n').pop());
    }

    it('should load neither rxjs nor undici on a bare require', function () {
      expect(runGraph("require('./lib/wait-on'); console.log(JSON.stringify(loaded()));")).to.deep.equal({ rxjs: false, undici: false });
    });

    it('should load both once a JS-engine wait runs', function () {
      const program = `require('./lib/wait-on')({ resources: [${JSON.stringify(__filename)}], window: 0, interval: 10 }).then(() => console.log(JSON.stringify(loaded())));`;
      expect(runGraph(program)).to.deep.equal({ rxjs: true, undici: true });
    });

    it('should load neither once a Rust-engine wait runs', function () {
      if (!fs.existsSync(addonPath({}))) this.skip();
      const program = `require('./lib/wait-on')({ resources: [${JSON.stringify(__filename)}], window: 0, interval: 10 }).then(() => console.log(JSON.stringify(loaded())));`;
      expect(runGraph(program, 'rust-strict')).to.deep.equal({ rxjs: false, undici: false });
    });
  });

  describe('prebuild resolution', function () {
    const cells = [
      [{ platform: 'darwin', arch: 'arm64', musl: false }, 'darwin-arm64'],
      [{ platform: 'darwin', arch: 'x64', musl: false }, 'darwin-x64'],
      [{ platform: 'linux', arch: 'x64', musl: false }, 'linux-x64'],
      [{ platform: 'linux', arch: 'arm64', musl: false }, 'linux-arm64'],
      [{ platform: 'linux', arch: 'x64', musl: true }, 'linux-x64-musl'],
      [{ platform: 'linux', arch: 'arm64', musl: true }, 'linux-arm64-musl'],
      [{ platform: 'win32', arch: 'x64', musl: false }, 'win32-x64'],
      [{ platform: 'win32', arch: 'arm64', musl: false }, 'win32-arm64']
    ];
    for (const [host, dir] of cells) {
      it(`resolves ${dir}`, function () {
        expect(prebuildDir(host)).to.equal(dir);
      });
    }

    it('detects musl only on linux without a glibc runtime', function () {
      expect(isMusl({ platform: 'linux', report: () => ({ header: {} }) })).to.equal(true);
      expect(isMusl({ platform: 'linux', report: () => ({ header: { glibcVersionRuntime: '2.39' } }) })).to.equal(false);
      expect(isMusl({ platform: 'darwin', report: () => ({ header: {} }) })).to.equal(false);
    });

    it('resolves the host addon under prebuilds/ and honors the override', function () {
      const p = addonPath({});
      expect(p.startsWith(path.join(REPO_ROOT, 'prebuilds') + path.sep)).to.equal(true);
      expect(p).to.include(`${process.platform}-${process.arch}`);
      expect(path.basename(p)).to.equal('wait-on.node');
      expect(addonPath({ WAIT_ON_NATIVE_LIBRARY_PATH: FIXTURE_ADDON })).to.equal(FIXTURE_ADDON);
    });
  });
});
