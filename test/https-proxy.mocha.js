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
const tlsFixture = require('./helpers/tls-fixture');
const stubProxy = require('./helpers/stub-proxy');

const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const before = mocha.before;
const after = mocha.after;
const afterEach = mocha.afterEach;
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
  toClose.forEach((s) => s.close(() => { if (--pending === 0) done(); }));
}

function listenHttp(handler, cb) {
  const server = http.createServer(handler);
  servers.push(server);
  server.listen(0, 'localhost', () => cb(server.address().port));
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

  function listenHttps(handler, cb) {
    const server = https.createServer({ key, cert }, handler);
    servers.push(server);
    server.listen(0, 'localhost', () => cb(server.address().port));
  }

  const socketPathFor = (name) => fx.socketPathFor(name);

  it('should fail an https self-signed cert when strictSSL is true', function (done) {
    listenHttps((req, res) => { res.end('ok'); }, function (port) {
      waitOn({ resources: [`https://localhost:${port}/`], strictSSL: true, ...FAST }, function (err) {
        expect(err).to.be.ok; // rejected: DEPTH_ZERO_SELF_SIGNED_CERT
        done();
      });
    });
  });

  it('should pass an https self-signed cert when strictSSL is false (default)', function (done) {
    listenHttps((req, res) => { res.end('ok'); }, function (port) {
      waitOn({ resources: [`https://localhost:${port}/`], strictSSL: false, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });

  it('should pass a self-signed cert when the matching ca is supplied with strictSSL true', function (done) {
    listenHttps((req, res) => { res.end('ok'); }, function (port) {
      waitOn({ resources: [`https://localhost:${port}/`], strictSSL: true, ca: cert, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });

  it('should resolve a unix-socket resource in both the short and absolute URL forms', function (done) {
    const sockPath = socketPathFor('sock1');
    const server = http.createServer((req, res) => { res.statusCode = req.url === '/foo' ? 200 : 404; res.end('x'); });
    servers.push(server);
    server.listen(sockPath, function () {
      const resources = [
        'http://unix:' + sockPath + ':/foo', // short form -> relative path /foo
        'http://unix:' + sockPath + ':http://localhost/foo' // absolute form
      ];
      waitOn({ resources, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
  });

  it('should not route a unix-socket check through HTTP_PROXY when it is set in the env', function (done) {
    // A bogus proxy that would fail the check if the socket request were routed through it.
    const priorHttpProxy = process.env.HTTP_PROXY;
    const priorLower = process.env.http_proxy;
    process.env.HTTP_PROXY = 'http://127.0.0.1:1'; // nothing listening
    process.env.http_proxy = 'http://127.0.0.1:1';
    const restore = () => {
      if (priorHttpProxy === undefined) delete process.env.HTTP_PROXY; else process.env.HTTP_PROXY = priorHttpProxy;
      if (priorLower === undefined) delete process.env.http_proxy; else process.env.http_proxy = priorLower;
    };
    const sockPath = socketPathFor('sock2');
    const server = http.createServer((req, res) => { res.statusCode = 200; res.end('x'); });
    servers.push(server);
    server.listen(sockPath, function () {
      waitOn({ resources: ['http://unix:' + sockPath + ':/'], ...FAST }, function (err) {
        restore();
        expect(err).to.not.be.ok; // socket check bypasses the proxy
        done();
      });
    });
  });

  it('should route through an explicit proxy object (dead proxy fails a reachable target)', function (done) {
    // Target is live; routing the request through a dead proxy must make the check fail,
    // which proves the proxy object is honored rather than connecting directly.
    listenHttp((req, res) => { res.statusCode = 200; res.end('ok'); }, function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], proxy: { host: '127.0.0.1', port: 1 }, timeout: 1000, interval: 100, window: 100 }, function (err) {
        expect(err).to.be.ok; // could not reach the (dead) proxy
        done();
      });
    });
  });

  it('should connect directly when proxy is false even if the target is reachable', function (done) {
    listenHttp((req, res) => { res.statusCode = 200; res.end('ok'); }, function (port) {
      waitOn({ resources: [`http://localhost:${port}/`], proxy: false, ...FAST }, function (err) {
        expect(err).to.not.be.ok;
        done();
      });
    });
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
  // with or without a trailing colon) must build a valid undici URL. Proof it worked:
  // against a DEAD proxy the check reaches the overall-timeout error ('Timed out ...'),
  // which is only possible if the URL parsed and the request was actually attempted.
  // A construction error ('Invalid URL') would instead surface immediately (guard test).
  const DEAD_PROXY = { timeout: 600, interval: 100, window: 100 };
  [
    { label: 'a bare IPv6 host', proxy: { host: '::1', port: 1 } },
    { label: 'an already-bracketed IPv6 host (no double-bracket)', proxy: { host: '[::1]', port: 1 } },
    { label: 'an expanded IPv6 host', proxy: { host: '2001:db8::1', port: 1 } },
    { label: "protocol 'http:' (trailing colon stripped)", proxy: { host: '127.0.0.1', port: 1, protocol: 'http:' } },
    { label: "protocol 'https:' (trailing colon stripped)", proxy: { host: '127.0.0.1', port: 1, protocol: 'https:' } },
    { label: "protocol 'http' (no colon)", proxy: { host: '127.0.0.1', port: 1, protocol: 'http' } },
    { label: 'an IPv6 host with proxy credentials', proxy: { host: '::1', port: 1, auth: { username: 'u', password: 'p@:/' } } }
  ].forEach(function ({ label, proxy }) {
    it(`should build a valid proxy URL for ${label} and reach the timeout via callback`, function (done) {
      let threw = false;
      try {
        waitOn({ resources: ['http://localhost:65002/'], proxy, ...DEAD_PROXY }, function (err) {
          expect(err).to.be.ok;
          expect(err.message).to.match(/Timed out/); // parsed OK -> dead proxy contacted -> timeout (not 'Invalid URL')
          done();
        });
      } catch (e) {
        threw = true;
        done(e);
      }
      expect(threw).to.equal(false); // never a synchronous throw out of waitOn()
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
