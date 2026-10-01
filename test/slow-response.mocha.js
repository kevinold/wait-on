'use strict';

// Characterization tests for jeffbski/wait-on#259: an http(s) resource that answers
// slower than `interval` must still succeed, because a later polling tick must not
// cancel the request already in flight. Self-contained on purpose (no frozen clock,
// no shared helpers or fixed ports) so the same file runs unmodified against older
// releases and can be transcribed for other implementations.

const { execSync, spawn } = require('child_process');
const fs = require('fs');
const http = require('http');
const https = require('https');
const os = require('os');
const path = require('path');
const waitOn = require('../');
const CLI_PATH = path.resolve(__dirname, '../bin/wait-on');

const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const before = mocha.before;
const after = mocha.after;
const afterEach = mocha.afterEach;
const expect = require('chai').expect;

const D = 400; // every response is delayed this long, well past `interval`
const FAST = { interval: 100, timeout: 3000, window: 100 };

// Windows has no Unix domain sockets, Node listens on named pipes instead
function socketPathIn(dirPath) {
  return process.platform === 'win32' ? path.join('\\\\?\\pipe', dirPath, 'sock') : path.resolve(dirPath, 'sock');
}

// Server that delays every response by `delay` and answers `first` to request #1 and
// `later` to every other request, so success can only come from request #1 surviving
// the ticks that fire while it is in flight. Records whether each request was
// client-closed before its response finished, and the peak number in flight.
function slowServer({ delay = D, first = 200, later = 503, headersFirst = false, createServer = http.createServer } = {}) {
  const requests = [];
  let inFlight = 0;
  let peak = 0;
  const server = createServer((req, res) => {
    const record = { aborted: false };
    const status = requests.length === 0 ? first : later;
    requests.push(record);
    peak = Math.max(peak, ++inFlight);
    if (headersFirst) {
      res.writeHead(status);
      res.write('slow body ');
    }
    const timer = setTimeout(() => {
      if (!headersFirst) res.writeHead(status);
      res.end('done');
    }, delay);
    res.on('close', () => {
      clearTimeout(timer);
      inFlight--;
      if (!res.writableFinished) record.aborted = true;
    });
  });
  return { server, requests, maxConcurrent: () => peak };
}

describe('slow http responses (issue #259)', function () {
  this.timeout(10000);

  let servers = [];
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-slow-'));

  afterEach(function (done) {
    const toClose = servers;
    servers = [];
    let pending = toClose.length;
    if (!pending) return done();
    toClose.forEach((s) => {
      s.closeAllConnections();
      s.close(() => {
        if (--pending === 0) done();
      });
    });
  });

  after(function () {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function listen(srv, cb) {
    servers.push(srv.server);
    srv.server.listen(0, 'localhost', () => cb(srv.server.address().port));
  }

  // A client abort reaches the server's 'close' handler a few event-loop turns after the
  // client side settles, so give the server time to record it before asserting.
  function afterServerSettles(fn) {
    setTimeout(fn, 100);
  }

  // wait-on succeeded, request #1 was answered rather than cancelled, and later ticks
  // ran while it was in flight (or did not, when `simultaneous` is 1)
  function expectSurvived(srv, err, done, { overlap = true } = {}) {
    afterServerSettles(() => {
      try {
        expect(err).to.not.be.ok;
        expect(srv.requests[0].aborted).to.equal(false);
        if (overlap) expect(srv.maxConcurrent()).to.be.at.least(2);
        else expect(srv.maxConcurrent()).to.equal(1);
        done();
      } catch (e) {
        done(e);
      }
    });
  }

  it('should succeed when an http HEAD response arrives after several polling intervals', function (done) {
    const srv = slowServer();
    listen(srv, (port) => {
      waitOn({ ...FAST, resources: [`http://localhost:${port}/`] }, (err) => expectSurvived(srv, err, done));
    });
  });

  it('should succeed when an http GET body completes after several polling intervals', function (done) {
    const srv = slowServer({ headersFirst: true });
    listen(srv, (port) => {
      waitOn({ ...FAST, resources: [`http-get://localhost:${port}/`] }, (err) => expectSurvived(srv, err, done));
    });
  });

  it('should succeed with simultaneous 1 without overlapping or cancelling the slow request', function (done) {
    const srv = slowServer();
    listen(srv, (port) => {
      waitOn({ ...FAST, simultaneous: 1, resources: [`http://localhost:${port}/`] }, (err) =>
        expectSurvived(srv, err, done, { overlap: false })
      );
    });
  });

  it('should succeed when httpTimeout exceeds the slow response delay', function (done) {
    const srv = slowServer();
    listen(srv, (port) => {
      waitOn({ ...FAST, httpTimeout: 1500, resources: [`http://localhost:${port}/`] }, (err) =>
        expectSurvived(srv, err, done)
      );
    });
  });

  it('should fail and abort the request when httpTimeout is shorter than the response delay', function (done) {
    const srv = slowServer();
    listen(srv, (port) => {
      waitOn({ ...FAST, timeout: 1000, httpTimeout: 150, resources: [`http://localhost:${port}/`] }, (err) => {
        try {
          expect(err).to.be.ok;
          expect(srv.requests[0].aborted).to.equal(true);
          done();
        } catch (e) {
          done(e);
        }
      });
    });
  });

  it('should succeed in reverse mode when the first slow response is a failing status', function (done) {
    const srv = slowServer({ first: 503, later: 200 });
    listen(srv, (port) => {
      waitOn({ ...FAST, reverse: true, resources: [`http://localhost:${port}/`] }, (err) =>
        expectSurvived(srv, err, done)
      );
    });
  });

  it('should succeed when a unix-socket http response arrives after several polling intervals', function (done) {
    const srv = slowServer();
    const socketPath = socketPathIn(tmpDir);
    servers.push(srv.server);
    srv.server.listen(socketPath, () => {
      waitOn({ ...FAST, resources: [`http://unix:${socketPath}:/`] }, (err) => expectSurvived(srv, err, done));
    });
  });

  it('should exit 0 when the CLI polls a server slower than its interval', function (done) {
    const srv = slowServer();
    listen(srv, (port) => {
      const args = [CLI_PATH, `http://localhost:${port}/`, '-i', '100', '-t', '3000'];
      // only exit code 0 is success; a non-zero code or a signal kill (null) is a failure
      spawn(process.execPath, args).on('exit', (code, signal) =>
        expectSurvived(srv, code === 0 ? null : new Error(`CLI exited ${code} ${signal}`), done)
      );
    });
  });

  describe('https', function () {
    let key;
    let cert;

    before(function () {
      // one-time cert generation can be slow on Windows runners
      this.timeout(30000);
      try {
        execSync('openssl version', { stdio: 'ignore' });
      } catch {
        this.skip(); // openssl not available in this environment
      }
      // EC P-256: constant-time keygen (RSA prime search is slow on Windows)
      execSync(
        `openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -keyout "${path.join(tmpDir, 'key.pem')}" -out "${path.join(tmpDir, 'cert.pem')}" -days 1 -nodes -subj "/CN=localhost"`,
        { stdio: 'ignore' }
      );
      key = fs.readFileSync(path.join(tmpDir, 'key.pem'));
      cert = fs.readFileSync(path.join(tmpDir, 'cert.pem'));
    });

    it('should succeed when an https response arrives after several polling intervals', function (done) {
      const srv = slowServer({ createServer: (handler) => https.createServer({ key, cert }, handler) });
      listen(srv, (port) => {
        waitOn({ ...FAST, strictSSL: false, resources: [`https://localhost:${port}/`] }, (err) =>
          expectSurvived(srv, err, done)
        );
      });
    });
  });
});
