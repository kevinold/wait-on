'use strict';

// Engine selection (KD-S1): WAIT_ON_ENGINE picks js (default), rust (addon with
// silent JS fallback) or rust-strict (addon load failure is an error).
// WAIT_ON_NATIVE_LIBRARY_PATH points the loader at a specific addon file.

const childProcess = require('child_process');
const fs = require('fs');
const http = require('http');
const net = require('net');
const os = require('os');
const path = require('path');
const { describe, it, before, after, afterEach } = require('mocha');
const { expect } = require('chai');

const waitOn = require('../lib/wait-on');
const { resolveEngine, prebuildDir, isMusl, addonPath } = require('../lib/engine');

const CLI_PATH = path.resolve(__dirname, '../bin/wait-on');
const REPO_ROOT = path.resolve(__dirname, '..');
const FIXTURE_ADDON = path.join(__dirname, 'fixtures', 'fake-addon.js');
const POISON = path.join(os.tmpdir(), `wait-on-no-such-addon-${process.pid}`, 'wait-on.node');
const ENGINE_VARS = ['WAIT_ON_ENGINE', 'WAIT_ON_NATIVE_LIBRARY_PATH'];
const OPTS = { resources: [__filename], timeout: 1000, interval: 100, window: 100 };

// Run fn with the engine env vars set exactly to vars (unset when absent), then restore.
async function withEnv(vars, fn) {
  const saved = {};
  for (const k of ENGINE_VARS) {
    saved[k] = process.env[k];
    if (vars[k] === undefined) delete process.env[k];
    else process.env[k] = vars[k];
  }
  try {
    return await fn();
  } finally {
    for (const k of ENGINE_VARS) {
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
