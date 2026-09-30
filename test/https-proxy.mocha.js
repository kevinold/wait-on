'use strict';

// Parity coverage for the fetch/undici migration: TLS client options and the
// unix-socket + proxy interaction, which the original suite never exercised.
// Self-contained (generated cert, ephemeral ports, temp socket) so these do not
// share fixed ports with the rest of the suite.

const waitOn = require('../');
const http = require('http');
const https = require('https');
const crypto = require('crypto');
const net = require('net');
const path = require('path');
const tlsFixture = require('./helpers/tls-fixture');
const stubProxy = require('./helpers/stub-proxy');
const { withEnv } = require('./helpers/engine-env');

const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const before = mocha.before;
const after = mocha.after;
const afterEach = mocha.afterEach;
const beforeEach = mocha.beforeEach;
const chai = require('chai');
const expect = chai.expect;

const FAST = { timeout: 2000, interval: 100, window: 100, tcpTimeout: 500 };

// Servers opened by a test; closed after each test in both suites below.
let servers = [];
function closeServers(done) {
  const toClose = servers;
  servers = [];
  let pending = toClose.length;
  if (!pending) return done();
  toClose.forEach((s) => {
    s.close(() => { if (--pending === 0) done(); });
    s.closeAllConnections(); // pooled keep-alive connections would hold close() open
  });
}

function listenHttp(handler, cb) {
  const server = http.createServer(handler);
  servers.push(server);
  server.listen(0, 'localhost', () => cb(server.address().port));
}
const httpTarget = (handler = (req, res) => res.end('ok')) => new Promise((resolve) => listenHttp(handler, resolve));

// Under WAIT_ON_ENGINE=rust* each check loads the counting addon (delegating to the real
// prebuild), so a test proves which engine answered.
const isRust = /^rust(-strict)?$/.test(process.env.WAIT_ON_ENGINE || '');
const COUNTING_ADDON = path.join(__dirname, 'fixtures', 'counting-addon.js');
const counting = isRust ? require(COUNTING_ADDON) : null;
const PROXY_ENV = ['HTTP_PROXY', 'http_proxy', 'HTTPS_PROXY', 'https_proxy', 'NO_PROXY', 'no_proxy'];

// 'resolved' or the rejection message, with the proxy env cleared except vars (set both
// spellings unless case is the point: Windows env names are case-insensitive). Under
// rust*, asserts the addon checked (routed) or was never constructed (routed: false).
async function outcome(opts, { vars = {}, routed = true } = {}) {
  const env = {
    WAIT_ON_ENGINE: process.env.WAIT_ON_ENGINE,
    WAIT_ON_NATIVE_LIBRARY_PATH: isRust ? COUNTING_ADDON : undefined,
    ...Object.fromEntries(PROXY_ENV.map((k) => [k, undefined])),
    ...vars
  };
  if (counting) counting.reset();
  const r = await withEnv(env, () => waitOn({ ...FAST, ...opts }).then(() => 'resolved', (e) => e.message));
  if (counting) {
    if (routed) expect(counting.calls.filter((c) => c.type === 'check')).to.have.length.of.at.least(1);
    else expect(counting.calls.filter((c) => c.type === 'construct')).to.have.length(0);
  }
  return r;
}

describe('https/tls and proxy parity', function () {
  this.timeout(6000);

  let fx;
  let key;
  let cert;

  before(function () {
    // Cert generation shells out to openssl; on slow Windows CI runners this can
    // exceed the suite's tight per-test timeout, so give the one-time setup its
    // own generous budget (the EC keygen is near-instant — belt-and-suspenders).
    this.timeout(30000);
    fx = tlsFixture();
    if (!fx) this.skip(); // openssl not available in this environment
    ({ key, cert } = fx);
  });

  after(function () {
    if (fx) fx.cleanup();
  });

  afterEach(closeServers);

  const socketPathFor = (name) => fx.socketPathFor(name);

  const ok = (req, res) => res.end('ok');
  function httpsTarget(handler = ok, extra = {}) {
    const server = https.createServer({ key, cert, ...extra }, handler);
    servers.push(server);
    return new Promise((resolve) => server.listen(0, 'localhost', () => resolve(server.address().port)));
  }
  // Requires a client cert the fixture cert verifies; records whether each request's cert was authorized.
  const mtlsTarget = (authorized) =>
    httpsTarget(
      (req, res) => {
        authorized.push(req.socket.authorized);
        res.end('ok');
      },
      { requestCert: true, rejectUnauthorized: true, ca: cert }
    );
  const at = (port) => [`https://localhost:${port}/`];
  const SHORT = { timeout: 1000 };

  // AE-L5-1
  it('should fail an https self-signed cert when strictSSL is true', async function () {
    const port = await httpsTarget();
    expect(await outcome({ resources: at(port), strictSSL: true, ...SHORT })).to.match(/Timed out/);
  });

  it('should pass an https self-signed cert when strictSSL is false (default)', async function () {
    const port = await httpsTarget();
    expect(await outcome({ resources: at(port), strictSSL: false })).to.equal('resolved');
  });

  [
    ['a Buffer', () => cert],
    ['a string', () => cert.toString()],
    ['a bundle with an unrelated cert first', () => `${fx.otherCert}${cert}`]
  ].forEach(function ([label, ca]) {
    it(`should pass a self-signed cert when the matching ca is supplied as ${label} with strictSSL true`, async function () {
      const port = await httpsTarget();
      expect(await outcome({ resources: at(port), strictSSL: true, ca: ca() })).to.equal('resolved');
    });
  });

  it('should resolve an http target when strictSSL, ca, cert and key are set', async function () {
    const port = await httpTarget();
    const opts = { resources: [`http://localhost:${port}/`], strictSSL: true, ca: cert, cert, key };
    expect(await outcome(opts)).to.equal('resolved');
  });

  it('should resolve an http target when cert and key are garbage', async function () {
    const port = await httpTarget();
    expect(await outcome({ resources: [`http://localhost:${port}/`], cert: 'garbage', key: 'garbage' })).to.equal('resolved');
  });

  describe('http redirect to https (TLS material applies to every hop)', function () {
    async function redirectTarget(seen) {
      const httpsPort = await httpsTarget((req, res) => {
        seen.push(req.url);
        res.end('ok');
      });
      return httpTarget((req, res) => {
        res.writeHead(302, { Location: `https://localhost:${httpsPort}/landed` });
        res.end();
      });
    }

    it('should resolve when strictSSL is true and the matching ca is supplied', async function () {
      const seen = [];
      const port = await redirectTarget(seen);
      expect(await outcome({ resources: [`http://localhost:${port}/`], strictSSL: true, ca: cert })).to.equal('resolved');
      expect(seen).to.include('/landed');
    });

    it('should time out when strictSSL is true without ca', async function () {
      const port = await redirectTarget([]);
      expect(await outcome({ resources: [`http://localhost:${port}/`], strictSSL: true, ...SHORT })).to.match(/Timed out/);
    });
  });

  describe('client certificate (mTLS)', function () {
    const garbageCert = '-----BEGIN CERTIFICATE-----\nnot base64!\n-----END CERTIFICATE-----\n';
    [
      ['cert and key are supplied', () => ({ cert, key }), true],
      ['no client cert is supplied', () => ({}), false],
      ['the encrypted key and its passphrase are supplied', () => ({ cert, key: fx.encryptedKey, passphrase: fx.passphrase }), true],
      ['the passphrase for the encrypted key is wrong', () => ({ cert, key: fx.encryptedKey, passphrase: 'wrong' }), false],
      ['a passphrase comes with an unencrypted key', () => ({ cert, key, passphrase: 'ignored' }), true],
      ['key is an object', () => ({ cert, key: {} }), false],
      ['cert is garbage', () => ({ cert: garbageCert, key }), false],
      ['cert and key come with strictSSL and the matching ca', () => ({ cert, key, strictSSL: true, ca: cert }), true]
    ].forEach(function ([label, tlsOpts, accepted]) {
      it(`should ${accepted ? 'resolve' : 'time out'} when ${label}`, async function () {
        const authorized = [];
        const port = await mtlsTarget(authorized);
        const r = await outcome({ resources: at(port), ...tlsOpts(), ...SHORT });
        if (accepted) {
          expect(r).to.equal('resolved');
          expect(authorized).to.include(true);
        } else {
          expect(r).to.match(/Timed out/);
          expect(authorized).to.have.length(0);
        }
      });
    });
  });

  describe('https target through a proxy object (CONNECT tunnel)', function () {
    let proxy;
    afterEach(async function () {
      await proxy.close();
    });
    const proxyObj = () => ({ host: '127.0.0.1', port: Number(new URL(proxy.url).port) });

    // AE-L5-3
    it('should resolve through the tunnel when strictSSL is true and the matching ca is supplied', async function () {
      proxy = await stubProxy.start();
      const port = await httpsTarget();
      expect(await outcome({ resources: at(port), proxy: proxyObj(), strictSSL: true, ca: cert })).to.equal('resolved');
      expect(proxy.connects[0].line).to.equal(`CONNECT localhost:${port}`);
    });

    it('should verify the target inside the tunnel and time out when strictSSL is true without ca', async function () {
      proxy = await stubProxy.start();
      const port = await httpsTarget();
      expect(await outcome({ resources: at(port), proxy: proxyObj(), strictSSL: true, ...SHORT })).to.match(/Timed out/);
      expect(proxy.connects).to.have.length.of.at.least(1);
    });

    it('should present the client cert inside the tunnel', async function () {
      proxy = await stubProxy.start();
      const authorized = [];
      const port = await mtlsTarget(authorized);
      expect(await outcome({ resources: at(port), proxy: proxyObj(), cert, key })).to.equal('resolved');
      expect(authorized).to.include(true);
      expect(proxy.connects).to.have.length.of.at.least(1);
    });
  });

  describe('https target and env proxies', function () {
    let proxy;
    afterEach(async function () {
      await proxy.close();
    });

    // KTD10 (a): the JS engine answers these (it drops TLS options behind an env proxy)
    it('should tunnel through HTTPS_PROXY on the JS check', async function () {
      proxy = await stubProxy.start();
      const port = await httpsTarget();
      const vars = { HTTPS_PROXY: proxy.url, https_proxy: proxy.url };
      await outcome({ resources: at(port), strictSSL: true, ca: cert, ...SHORT }, { vars, routed: false });
      expect(proxy.connects).to.have.length.of.at.least(1);
    });

    it('should tunnel through HTTP_PROXY on the JS check when HTTPS_PROXY is unset', async function () {
      proxy = await stubProxy.start();
      const port = await httpsTarget();
      const vars = { HTTP_PROXY: proxy.url, http_proxy: proxy.url };
      await outcome({ resources: at(port), ...SHORT }, { vars, routed: false });
      expect(proxy.connects).to.have.length.of.at.least(1);
    });

    it('should connect directly when NO_PROXY exempts the https target', async function () {
      proxy = await stubProxy.start();
      const port = await httpsTarget();
      const vars = { HTTPS_PROXY: proxy.url, https_proxy: proxy.url, NO_PROXY: 'localhost', no_proxy: 'localhost' };
      expect(await outcome({ resources: at(port) }, { vars })).to.equal('resolved');
      expect(proxy.connects).to.have.length(0);
    });
  });

  // Serves a unix socket (named pipe on Windows) that records each request path.
  function unixTarget(name, seen, handler = (req, res) => res.end('x')) {
    const sockPath = socketPathFor(name);
    const server = http.createServer((req, res) => {
      seen.push(req.url);
      handler(req, res);
    });
    servers.push(server);
    return new Promise((resolve) => server.listen(sockPath, () => resolve(sockPath)));
  }

  it('should resolve a unix-socket resource in both the short and absolute URL forms', async function () {
    const seen = [];
    const sockPath = await unixTarget('sock1', seen, (req, res) => {
      res.statusCode = req.url === '/foo' ? 200 : 404;
      res.end('x');
    });
    const resources = [
      'http://unix:' + sockPath + ':/foo', // short form -> relative path /foo
      'http://unix:' + sockPath + ':http://localhost/foo' // absolute form
    ];
    expect(await outcome({ resources })).to.equal('resolved');
    expect(seen).to.include('/foo');
  });

  // AE-L5-4
  it('should not route a unix-socket check through HTTP_PROXY when it is set in the env', async function () {
    const seen = [];
    const sockPath = await unixTarget('sock2', seen);
    const dead = 'http://127.0.0.1:1'; // nothing listening
    const vars = { HTTP_PROXY: dead, http_proxy: dead };
    expect(await outcome({ resources: ['http://unix:' + sockPath + ':/health'] }, { vars })).to.equal('resolved');
    expect(seen).to.include('/health');
  });

  it('should not route a unix-socket check through a proxy object', async function () {
    const seen = [];
    const sockPath = await unixTarget('sock3', seen);
    const opts = { resources: ['http://unix:' + sockPath + ':/health'], proxy: { host: '127.0.0.1', port: 1 } };
    expect(await outcome(opts)).to.equal('resolved');
    expect(seen).to.include('/health');
  });

  it('should surface a malformed proxy object as a callback error, not a synchronous throw', function (done) {
    // proxy:{} is missing host/port; it must fail validation and reach the callback,
    // never crash waitOn synchronously.
    expect(function () {
      waitOn({ resources: ['http://localhost:65001/'], proxy: {}, timeout: 500 }, function (err) {
        expect(err).to.be.ok;
        done();
      });
    }).to.not.throw();
  });

  // ---- B1: proxy URL normalization + construction never throws synchronously ----
  // A proxy object axios normalized (bare/expanded/bracketed IPv6 host, or a protocol
  // with or without a trailing colon) must build a valid proxy URL. Proof it worked:
  // against a DEAD proxy the check reaches the overall-timeout error ('Timed out ...'),
  // which is only possible if the URL parsed and the request was actually attempted.
  // A construction error ('Invalid URL') would instead surface immediately (guard test).
  // An https: proxy stays on the JS check (KTD10 b).
  const DEAD_PROXY = { timeout: 600, interval: 100, window: 100 };
  [
    { label: 'a bare IPv6 host', proxy: { host: '::1', port: 1 } },
    { label: 'an already-bracketed IPv6 host (no double-bracket)', proxy: { host: '[::1]', port: 1 } },
    { label: 'an expanded IPv6 host', proxy: { host: '2001:db8::1', port: 1 } },
    { label: "protocol 'http:' (trailing colon stripped)", proxy: { host: '127.0.0.1', port: 1, protocol: 'http:' } },
    { label: "protocol 'https:' (trailing colon stripped)", proxy: { host: '127.0.0.1', port: 1, protocol: 'https:' }, routed: false },
    { label: "protocol 'http' (no colon)", proxy: { host: '127.0.0.1', port: 1, protocol: 'http' } },
    { label: 'an IPv6 host with proxy credentials', proxy: { host: '::1', port: 1, auth: { username: 'u', password: 'p@:/' } } }
  ].forEach(function ({ label, proxy, routed = true }) {
    it(`should build a valid proxy URL for ${label} and reach the timeout via callback`, async function () {
      const r = await outcome({ resources: ['http://localhost:65002/'], proxy, ...DEAD_PROXY }, { routed });
      expect(r).to.match(/Timed out/); // parsed OK -> dead proxy contacted -> timeout (not 'Invalid URL')
    });
  });

  it('should route a dispatcher construction error to the callback, not throw synchronously', function (done) {
    // A host that still cannot form a URL after normalization (space is not a valid
    // authority char) makes new ProxyAgent() throw; the guard must deliver it via cb.
    let threw = false;
    try {
      waitOn({ resources: ['http://localhost:65003/'], proxy: { host: 'bad host', port: 8080 }, timeout: 600 }, function (err) {
        expect(err).to.be.ok; // construction error delivered, not swallowed
        done();
      });
    } catch (e) {
      threw = true;
      done(e);
    }
    expect(threw).to.equal(false);
  });

});

// Proxy object, proxy: false and env proxies on http targets (KTD4, KTD8). The stub
// proxy counts what it forwarded, so direct cells prove it saw nothing.
describe('http proxy routing parity', function () {
  this.timeout(6000);
  let proxy;
  let port;

  beforeEach(async function () {
    proxy = await stubProxy.start();
    port = await httpTarget();
  });

  afterEach(async function () {
    await proxy.close();
    await new Promise((resolve) => closeServers(resolve));
  });

  const target = () => ({ resources: [`http://localhost:${port}/`] });
  const proxyObj = (extra) => ({ host: '127.0.0.1', port: Number(new URL(proxy.url).port), ...extra });
  const both = (name, value) => ({ [name.toUpperCase()]: value, [name.toLowerCase()]: value });
  const notOnWindows = (ctx) => process.platform === 'win32' && ctx.skip(); // env names are case-insensitive there

  it('should forward an absolute-form request through a proxy object', async function () {
    expect(await outcome({ ...target(), proxy: proxyObj() })).to.equal('resolved');
    expect(proxy.requests[0].line).to.equal(`HEAD http://localhost:${port}/`);
    expect(proxy.connects).to.have.length(0);
  });

  it('should send Basic proxy credentials from the proxy object auth', async function () {
    const auth = { username: 'u', password: 'p@:/' };
    expect(await outcome({ ...target(), proxy: proxyObj({ auth }) })).to.equal('resolved');
    expect(proxy.requests[0].proxyAuthorization).to.equal('Basic ' + Buffer.from('u:p@:/').toString('base64'));
  });

  it('should time out through a dead proxy object even though the target is reachable', async function () {
    const r = await outcome({ ...target(), proxy: { host: '127.0.0.1', port: 1 }, timeout: 1000 });
    expect(r).to.match(/Timed out/);
  });

  it('should connect directly when proxy is false even if HTTP_PROXY is set', async function () {
    expect(await outcome({ ...target(), proxy: false }, { vars: both('http_proxy', proxy.url) })).to.equal('resolved');
    expect(proxy.requests).to.have.length(0);
  });

  it('should connect directly when proxy is false and no env proxy is set', async function () {
    expect(await outcome({ ...target(), proxy: false })).to.equal('resolved');
  });

  // AE-L5-2 and the env matrix: [label, vars, proxied]
  [
    ['HTTP_PROXY is set', () => both('http_proxy', proxy.url), true],
    ['only lowercase http_proxy is set', () => ({ http_proxy: proxy.url }), true, 'case'],
    ['NO_PROXY names the host', () => ({ ...both('http_proxy', proxy.url), ...both('no_proxy', 'localhost') }), false],
    ['only lowercase no_proxy names the host', () => ({ http_proxy: proxy.url, no_proxy: 'localhost' }), false, 'case'],
    ['NO_PROXY is *', () => ({ ...both('http_proxy', proxy.url), ...both('no_proxy', '*') }), false],
    ['NO_PROXY is *.localhost (subdomains only)', () => ({ ...both('http_proxy', proxy.url), ...both('no_proxy', '*.localhost') }), true],
    ['NO_PROXY names the host and its port', () => ({ ...both('http_proxy', proxy.url), ...both('no_proxy', `localhost:${port}`) }), false],
    ['NO_PROXY names the host and another port', () => ({ ...both('http_proxy', proxy.url), ...both('no_proxy', `localhost:${port + 1}`) }), true],
    ['an empty http_proxy shadows HTTP_PROXY', () => ({ http_proxy: '', HTTP_PROXY: proxy.url }), false, 'case'],
    ['only HTTPS_PROXY is set', () => both('https_proxy', proxy.url), false]
  ].forEach(function ([label, vars, proxied, caseSensitive]) {
    it(`should ${proxied ? 'go through the proxy' : 'connect directly'} when ${label}`, async function () {
      if (caseSensitive) notOnWindows(this);
      expect(await outcome(target(), { vars: vars() })).to.equal('resolved');
      if (proxied) expect(proxy.requests[0].line).to.equal(`HEAD http://localhost:${port}/`);
      else expect(proxy.requests).to.have.length(0);
    });
  });
});

// Plain-http auth/headers/validateStatus parity: no TLS or proxy, so no openssl gate.
describe('plain http auth, headers and validateStatus parity', function () {
  this.timeout(6000);
  afterEach(closeServers);

  it('should send a Basic Authorization header built from opts.auth', function (done) {
    let seen;
    listenHttp((req, res) => { seen = req.headers.authorization; res.statusCode = 200; res.end('ok'); }, function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], auth: { username: 'user', password: 'p@ss/word' }, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        const expected = 'Basic ' + Buffer.from('user:p@ss/word').toString('base64');
        expect(seen).to.equal(expected);
        done();
      });
    });
  });


  // ---- B2: opts.auth -> Basic header, axios parity ----
  // capture the Authorization header the server actually receives.
  function seenAuthFor(opts, assertSeen) {
    return function (done) {
      let seen = 'MISSING';
      listenHttp((req, res) => { seen = req.headers.authorization; res.statusCode = 200; res.end('ok'); }, function (port) {
        waitOn({ resources: [`http://localhost:${port}/`], ...opts, ...FAST }, function (err) {
          expect(err).to.not.be.ok;
          assertSeen(seen);
          done();
        });
      });
    };
  }
  const basic = (u, p) => 'Basic ' + Buffer.from(`${u}:${p}`).toString('base64');

  // auth overrides an existing Authorization header (any case) instead of comma-merging.
  ['Authorization', 'authorization', 'AUTHORIZATION', 'AuThOrIzAtIon'].forEach(function (headerName) {
    it(`should let opts.auth override a custom ${headerName} header (no comma-merge)`,
      seenAuthFor(
        { headers: { [headerName]: 'Bearer CUSTOM' }, auth: { username: 'user', password: 'p' } },
        (seen) => expect(seen).to.equal(basic('user', 'p'))
      ));
  });

  // partial / empty auth builds a Basic header exactly as axios did (each side -> '').
  [
    { label: 'username + password', auth: { username: 'user', password: 'p@ss/word' }, expected: basic('user', 'p@ss/word') },
    { label: 'password only', auth: { password: 'p' }, expected: basic('', 'p') },
    { label: 'username only', auth: { username: 'u' }, expected: basic('u', '') },
    { label: 'empty object', auth: {}, expected: basic('', '') }
  ].forEach(function ({ label, auth, expected }) {
    it(`should build a Basic header for auth with ${label}`,
      seenAuthFor({ auth }, (seen) => expect(seen).to.equal(expected)));
  });

  it('should pass a custom Authorization header through untouched when no auth is set',
    seenAuthFor({ headers: { Authorization: 'Bearer KEEPME' } }, (seen) => expect(seen).to.equal('Bearer KEEPME')));

  it('should send no Authorization header when neither auth nor a header is set',
    seenAuthFor({}, (seen) => expect(seen).to.equal(undefined)));

  it('should keep other custom headers while opts.auth sets Authorization', function (done) {
    let seen;
    listenHttp((req, res) => { seen = req.headers; res.statusCode = 200; res.end('ok'); }, function (port) {
      waitOn(
        { resources: [`http://localhost:${port}/`], headers: { 'X-Custom': 'keep', Authorization: 'Bearer OLD' }, auth: { username: 'u', password: 'p' }, ...FAST },
        function (err) {
          expect(err).to.not.be.ok;
          expect(seen['x-custom']).to.equal('keep'); // non-authorization header survives the strip loop
          expect(seen.authorization).to.equal('Basic ' + Buffer.from('u:p').toString('base64')); // auth still wins
          done();
        }
      );
    });
  });

  // ---- validateStatus over real HTTP (fetch decides success after the response) ----
  // fetch never rejects on status, so success is decided by validateStatus (default 2xx).
  // These exercise that decision end-to-end over HTTP for HEAD and GET.
  function statusServer(code) {
    return function (cb) {
      listenHttp((req, res) => { res.statusCode = code; res.end('x'); }, cb);
    };
  }

  it('should succeed on a non-2xx when validateStatus accepts it (HEAD)', function (done) {
    statusServer(404)(function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], validateStatus: (s) => s === 404, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });

  it('should succeed on a non-2xx when validateStatus accepts it (GET)', function (done) {
    statusServer(404)(function (port) {
      waitOn({ resources: [`http-get://localhost:${port}/`], validateStatus: (s) => s === 404, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });

  it('should fail a 2xx when validateStatus rejects it', function (done) {
    statusServer(200)(function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], validateStatus: (s) => s === 500, timeout: 600, interval: 100, window: 100 }, function (err) {
        expect(err).to.be.ok; // 200 not accepted -> never ready -> timeout
        done();
      });
    });
  });

  it('should fail a non-2xx by default (no validateStatus)', function (done) {
    statusServer(404)(function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], timeout: 600, interval: 100, window: 100 }, function (err) {
        expect(err).to.be.ok; // default 2xx check rejects 404
        done();
      });
    });
  });

  it('should succeed on 204 by default (2xx boundary)', function (done) {
    statusServer(204)(function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });
});

// Self-checks for the shared test helpers (KTD6, KTD8).
describe('test helpers: tls fixture and stub proxy', function () {
  this.timeout(30000);

  describe('tls fixture', function () {
    let fx;
    before(function () {
      fx = tlsFixture();
      if (!fx) this.skip(); // openssl not available in this environment
    });
    after(function () {
      if (fx) fx.cleanup();
    });

    it('should issue a leaf with a localhost SAN that is not a CA', function () {
      const x509 = new crypto.X509Certificate(fx.cert);
      expect(x509.subjectAltName).to.include('DNS:localhost');
      expect(x509.ca).to.equal(false);
    });

    it('should issue an unrelated second leaf', function () {
      const fp = (pem) => new crypto.X509Certificate(pem).fingerprint256;
      expect(fp(fx.otherCert)).to.not.equal(fp(fx.cert));
    });

    it('should encrypt the key so it opens only with the passphrase', function () {
      expect(crypto.createPrivateKey({ key: fx.encryptedKey, passphrase: fx.passphrase }).asymmetricKeyType).to.equal('ec');
      expect(() => crypto.createPrivateKey(fx.encryptedKey)).to.throw();
    });
  });

  describe('stub proxy', function () {
    let proxy;
    afterEach(async function () {
      if (proxy) await proxy.close();
      proxy = undefined;
      await new Promise((resolve) => closeServers(resolve));
    });

    const listenTarget = () => new Promise((resolve) => listenHttp((req, res) => res.end('hit ' + req.url), resolve));

    it('should forward an absolute-form request and record it', async function () {
      const port = await listenTarget();
      proxy = await stubProxy.start();
      const proxyPort = new URL(proxy.url).port;
      const body = await new Promise((resolve, reject) => {
        const headers = { 'proxy-authorization': 'Basic dTpw' };
        http
          .get({ host: '127.0.0.1', port: proxyPort, path: `http://localhost:${port}/x`, headers }, (res) => {
            let data = '';
            res.on('data', (c) => (data += c));
            res.on('end', () => resolve(data));
          })
          .on('error', reject);
      });
      expect(body).to.equal('hit /x');
      expect(proxy.requests).to.deep.equal([{ line: `GET http://localhost:${port}/x`, proxyAuthorization: 'Basic dTpw' }]);
      expect(proxy.connects).to.have.length(0);
    });

    it('should tunnel a raw CONNECT and record it', async function () {
      const port = await listenTarget();
      proxy = await stubProxy.start();
      const proxyPort = new URL(proxy.url).port;
      const reply = await new Promise((resolve, reject) => {
        const sock = net.connect(proxyPort, '127.0.0.1', () => {
          sock.write(`CONNECT localhost:${port} HTTP/1.1\r\nHost: localhost:${port}\r\n\r\n`);
        });
        let data = '';
        let sent = false;
        sock.on('data', (c) => {
          data += c;
          if (!sent && data.includes('\r\n\r\n')) {
            sent = true;
            sock.write('GET /t HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n');
          }
        });
        sock.on('end', () => resolve(data));
        sock.on('error', reject);
      });
      expect(reply).to.match(/^HTTP\/1\.1 200 /);
      expect(reply).to.include('hit /t');
      expect(proxy.connects).to.deep.equal([{ line: `CONNECT localhost:${port}`, proxyAuthorization: undefined }]);
      expect(proxy.requests).to.have.length(0);
    });
  });
});
