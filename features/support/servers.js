'use strict';

// Local servers the steps start (R10): ephemeral ports, temp-path unix sockets or Windows
// named pipes. Each returns {port | path, close}.

const http = require('http');
const https = require('https');
const net = require('net');
const path = require('path');
const tlsFixture = require('../../test/helpers/tls-fixture');
const { getFreePort, listening } = require('../../test/helpers/cli-conformance');

let pipes = 0;
// Windows has no unix domain sockets; Node listens on a named pipe there instead
// (the \\?\pipe\<dir> form test/helpers/tls-fixture.js uses); a counter keeps names unique
function socketPath(dir) {
  pipes += 1;
  const name = `sock-${pipes}`;
  return process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, name) : path.join(dir, name);
}

// close, dropping kept-alive http connections first so close() does not wait on them
const closer = (server) => () => {
  if (server.closeAllConnections) server.closeAllConnections();
  return new Promise((resolve) => server.close(() => resolve()));
};

async function tcpServer(host) {
  const sockets = new Set();
  const server = net.createServer((s) => {
    sockets.add(s);
    s.on('close', () => sockets.delete(s));
  });
  await listening(server, 0, host);
  const close = closer(server);
  return {
    port: server.address().port,
    close: () => {
      for (const s of sockets) s.destroy();
      return close();
    }
  };
}

const answering = (status) => (req, res) => {
  res.statusCode = status;
  res.end();
};

// listens on an ephemeral loopback port; keepAlive 1ms so close() never waits on a client
async function start(server, extra = {}) {
  server.keepAliveTimeout = 1;
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, close: closer(server), ...extra };
}

const httpServer = (status) => start(http.createServer(answering(status)));

// records each request's method, url and headers (headers, auth)
function recordingHttpServer(status) {
  const requests = [];
  const handler = (req, res) => {
    requests.push({ method: req.method, url: req.url, headers: req.headers });
    answering(status)(req, res);
  };
  return start(http.createServer(handler), { requests });
}

// `/` answers 302 to `/ok`, which answers 200 (followRedirect)
function redirectingHttpServer() {
  const handler = (req, res) => {
    if (req.url === '/ok') return answering(200)(req, res);
    res.writeHead(302, { location: '/ok' });
    res.end();
  };
  return start(http.createServer(handler));
}

// accepts requests and never answers (httpTimeout); close() drops the held connections
const silentHttpServer = () => start(http.createServer(() => {}));

// answers after `ms`, recording the most requests it held at once (simultaneous)
function slowHttpServer(status, ms) {
  const counts = { inFlight: 0, peak: 0 };
  const handler = (req, res) => {
    counts.peak = Math.max(counts.peak, ++counts.inFlight);
    setTimeout(() => {
      counts.inFlight--;
      answering(status)(req, res);
    }, ms);
  };
  return start(http.createServer(handler), { counts });
}

// A self-signed https server (tls: the fixture's PEMs) recording each request's
// `authorized`; with clientCert it requires a client certificate its own cert verifies.
// openssl is required: the contract allows no skipped scenarios (R2), so a missing tool fails
async function tlsServer({ status = 200, clientCert = false } = {}) {
  const tls = tlsFixture();
  if (!tls) throw new Error('openssl required for the https scenarios (not found on PATH)');
  const authorized = [];
  const mtls = clientCert ? { requestCert: true, rejectUnauthorized: true, ca: tls.cert } : {};
  const server = https.createServer({ key: tls.key, cert: tls.cert, ...mtls }, (req, res) => {
    authorized.push(req.socket.authorized);
    answering(status)(req, res);
  });
  const started = await start(server, { tls, authorized });
  return { ...started, close: () => started.close().then(tls.cleanup) };
}

// the @engine https server: trusted through `ca`, its own certificate
async function httpsServer(status) {
  const server = await tlsServer({ status });
  return { ...server, ca: server.tls.cert.toString() };
}

async function unixServer(dir) {
  const file = socketPath(dir);
  const server = await listening(
    net.createServer((s) => s.end()),
    file
  );
  return { path: file, close: closer(server) };
}

async function httpUnixServer(dir, status) {
  const file = socketPath(dir);
  const server = await listening(http.createServer(answering(status)), file);
  return { path: file, close: closer(server) };
}

module.exports = {
  socketPath,
  tcpServer,
  freePort: getFreePort,
  httpServer,
  recordingHttpServer,
  redirectingHttpServer,
  silentHttpServer,
  slowHttpServer,
  httpsServer,
  tlsServer,
  unixServer,
  httpUnixServer
};
