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
const { describe, it, before, after, afterEach, beforeEach } = require('mocha');
const { expect } = require('chai');

const waitOn = require('../lib/wait-on');
const { resolveEngine, prebuildDir, isMusl, addonPath } = require('../lib/engine');
const counting = require('./fixtures/counting-addon');
const { routesHttpToRust } = waitOn._internal;

const CLI_PATH = path.resolve(__dirname, '../bin/wait-on');
const REPO_ROOT = path.resolve(__dirname, '..');
const FIXTURE_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');
const COUNTING_ADDON = path.join(__dirname, 'fixtures', 'counting-addon.js');
const POISON = path.join(os.tmpdir(), `wait-on-no-such-addon-${process.pid}`, 'wait-on.node');
const ENGINE_VARS = ['WAIT_ON_ENGINE', 'WAIT_ON_NATIVE_LIBRARY_PATH'];
const OPTS = { resources: [__filename], timeout: 1000, interval: 100, window: 100 };

// Run fn with the engine env vars (plus any other keys in vars) set exactly to vars
// (unset when absent or undefined), then restore.
async function withEnv(vars, fn) {
  const keys = [...new Set([...ENGINE_VARS, ...Object.keys(vars)])];
  const saved = {};
  for (const k of keys) {
    saved[k] = process.env[k];
    if (vars[k] === undefined) delete process.env[k];
    else process.env[k] = vars[k];
  }
  try {
    return await fn();
  } finally {
    for (const k of keys) {
      if (saved[k] === undefined) delete process.env[k];
      else process.env[k] = saved[k];
    }
  }
}

function runCLI(vars) {
  const env = { ...process.env };
  for (const k of ENGINE_VARS) {
    if (vars[k] === undefined) delete env[k];
    else env[k] = vars[k];
  }
  const args = [CLI_PATH, __filename, '-t', '1000', '-i', '100', '-w', '100'];
  const r = childProcess.spawnSync(process.execPath, args, { env, encoding: 'utf8' });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
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

  before(function () {
    junkDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-junk-'));
    junkAddon = path.join(junkDir, 'wait-on.node');
    fs.writeFileSync(junkAddon, 'not a native addon');
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
  });

  describe('http routing (counting addon)', function () {
    const NO_PROXY_ENV = { HTTP_PROXY: undefined, http_proxy: undefined, HTTPS_PROXY: undefined, https_proxy: undefined };
    const RUST = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: COUNTING_ADDON, ...NO_PROXY_ENV };
    const FAST = { timeout: 2000, interval: 100 };
    const closers = [];
    let certDir;
    let cert;
    let key;

    before(function () {
      this.timeout(30000);
      try {
        childProcess.execSync('openssl version', { stdio: 'ignore' });
      } catch {
        return; // https cells skip below
      }
      certDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-route-'));
      childProcess.execSync(
        `openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout ${certDir}/key.pem -out ${certDir}/cert.pem -days 1 -nodes -subj "/CN=localhost"`,
        { stdio: 'ignore' }
      );
      key = fs.readFileSync(path.join(certDir, 'key.pem'));
      cert = fs.readFileSync(path.join(certDir, 'cert.pem'));
    });

    after(function () {
      if (certDir) fs.rmSync(certDir, { recursive: true, force: true });
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

    const constructs = () => counting.calls.filter((c) => c.type === 'construct');
    const checks = () => counting.calls.filter((c) => c.type === 'check');

    // 'resolved' or the rejection message, so both engines' outcomes compare directly.
    function outcome(vars, opts) {
      return withEnv(vars, () => waitOn({ ...FAST, ...opts }).then(() => 'resolved', (e) => e.message));
    }

    it('should construct one HEAD and one GET checker and check each when http resources run under rust', async function () {
      const port = await httpPort();
      const resources = [`http://localhost:${port}/`, `http-get://localhost:${port}/`];
      expect(await outcome(RUST, { resources })).to.equal('resolved');
      expect(constructs().map((c) => c.opts.method).sort()).to.deep.equal(['GET', 'HEAD']);
      expect(checks().filter((c) => c.opts.method === 'HEAD')).to.have.length.of.at.least(1);
      expect(checks().filter((c) => c.opts.method === 'GET')).to.have.length.of.at.least(1);
      expect(counting.calls.filter((c) => c.type === 'cancel')).to.have.length(2);
    });

    it('should route a default https resource to the addon when strictSSL is unset', async function () {
      if (!cert) this.skip();
      const port = await httpsPort();
      expect(await outcome(RUST, { resources: [`https://localhost:${port}/`] })).to.equal('resolved');
      expect(constructs()).to.have.length(1);
      expect(constructs()[0].opts.url).to.equal(`https://localhost:${port}/`);
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
      expect(constructs()[0].opts).to.deep.equal({
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
      expect(checks()[0].validateStatus(200)).to.equal(true);
      counting.reset();
      expect(await outcome(RUST, { resources, validateStatus: (s) => s === 200 })).to.equal('resolved');
      expect(checks()[0].validateStatus(204)).to.equal(false);
      counting.reset();
      const thrower = () => {
        throw new Error('boom');
      };
      expect(await outcome(RUST, { resources, validateStatus: thrower, timeout: 300 })).to.match(/Timed out/);
      expect(checks()[0].validateStatus(200)).to.equal(false);
    });

    it('should deliver a checker construction error to the callback without throwing', async function () {
      counting.constructError = new Error('client build failed');
      const err = await withEnv(RUST, () => new Promise((resolve) => waitOn({ resources: ['http://localhost:1/'], ...FAST }, resolve)));
      expect(err.message).to.equal('client build failed');
    });

    describe('L5 cells stay on the JS check', function () {
      it('should resolve without constructing a checker when the resource is http://unix:', async function () {
        if (process.platform === 'win32') this.skip();
        const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-route-sock-'));
        closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
        const sock = path.join(dir, 'sock');
        await listen(http.createServer(handler), sock);
        expect(await outcome(RUST, { resources: [`http://unix:${sock}:/`] })).to.equal('resolved');
        expect(counting.calls).to.have.length(0);
      });

      it('should resolve without constructing a checker when ca is set', async function () {
        const port = await httpPort();
        const opts = { resources: [`http://localhost:${port}/`], ca: 'unused-for-http' };
        expect(await outcome(RUST, opts)).to.equal('resolved');
        expect(counting.calls).to.have.length(0);
      });

      it('should reject a self-signed cert like JS without constructing a checker when strictSSL is true', async function () {
        if (!cert) this.skip();
        const port = await httpsPort();
        const opts = { resources: [`https://localhost:${port}/`], strictSSL: true, timeout: 600 };
        const js = await outcome({ WAIT_ON_ENGINE: 'js', ...NO_PROXY_ENV }, opts);
        expect(js).to.match(/Timed out/);
        expect(await outcome(RUST, opts)).to.equal(js);
        expect(counting.calls).to.have.length(0);
      });

      it('should resolve without constructing a checker when proxy is false', async function () {
        const port = await httpPort();
        expect(await outcome(RUST, { resources: [`http://localhost:${port}/`], proxy: false })).to.equal('resolved');
        expect(counting.calls).to.have.length(0);
      });

      it('should time out through a dead env proxy without constructing a checker when HTTP_PROXY is set', async function () {
        const port = await httpPort();
        const vars = { ...RUST, HTTP_PROXY: 'http://127.0.0.1:1', NO_PROXY: undefined, no_proxy: undefined };
        expect(await outcome(vars, { resources: [`http://localhost:${port}/`], timeout: 600 })).to.match(/Timed out/);
        expect(counting.calls).to.have.length(0);
      });
    });

    it('should time out like JS without constructing a checker when the url has userinfo', async function () {
      const port = await httpPort();
      const opts = { resources: [`http://u:p@localhost:${port}/`], timeout: 600 };
      const js = await outcome({ WAIT_ON_ENGINE: 'js', ...NO_PROXY_ENV }, opts);
      expect(js).to.match(/Timed out/);
      expect(await outcome(RUST, opts)).to.equal(js);
      expect(constructs()).to.have.length(0);
    });

    describe('routesHttpToRust', function () {
      const base = { addon: {}, validatedOpts: { strictSSL: false }, socketPath: undefined, env: {}, url: 'http://localhost:1/' };
      const cells = [
        ['no addon is loaded', { addon: null }],
        ['a unix socketPath is set', { socketPath: '/tmp/sock' }],
        ['ca is set', { validatedOpts: { strictSSL: false, ca: 'x' } }],
        ['cert is set', { validatedOpts: { strictSSL: false, cert: 'x' } }],
        ['key is set', { validatedOpts: { strictSSL: false, key: 'x' } }],
        ['passphrase is set', { validatedOpts: { strictSSL: false, passphrase: 'x' } }],
        ['strictSSL is true', { validatedOpts: { strictSSL: true } }],
        ['proxy is false', { validatedOpts: { strictSSL: false, proxy: false } }],
        ['proxy is an object', { validatedOpts: { strictSSL: false, proxy: { host: 'h', port: 1 } } }],
        ['HTTP_PROXY is set', { env: { HTTP_PROXY: 'http://p:1' } }],
        ['http_proxy is set', { env: { http_proxy: 'http://p:1' } }],
        ['HTTPS_PROXY is set', { env: { HTTPS_PROXY: 'http://p:1' } }],
        ['https_proxy is set', { env: { https_proxy: 'http://p:1' } }],
        ['the url has userinfo', { url: 'http://u:p@localhost:1/' }]
      ];
      for (const [label, override] of cells) {
        it(`should route to JS when ${label}`, function () {
          expect(routesHttpToRust({ ...base, ...override })).to.equal(false);
        });
      }

      it('should route to Rust when the addon is loaded and no L5 condition holds', function () {
        expect(routesHttpToRust(base)).to.equal(true);
      });
    });
  });

  // AE-L4-4 / R-L4-9: an API caller's process exits once waitOn settles, even with a
  // request in flight to a server that never answers. Real clock, subprocess.
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
