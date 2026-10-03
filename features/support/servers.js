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

async function httpServer(status) {
  const server = http.createServer(answering(status));
  server.keepAliveTimeout = 1;
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, close: closer(server) };
}

// records each request's method, url and headers (headers, auth)
async function recordingHttpServer(status) {
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push({ method: req.method, url: req.url, headers: req.headers });
    answering(status)(req, res);
  });
  server.keepAliveTimeout = 1;
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, requests, close: closer(server) };
}

// `/` answers 302 to `/ok`, which answers 200 (followRedirect)
async function redirectingHttpServer() {
  const server = http.createServer((req, res) => {
    if (req.url === '/ok') return answering(200)(req, res);
    res.writeHead(302, { location: '/ok' });
    res.end();
  });
  server.keepAliveTimeout = 1;
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, close: closer(server) };
}

// accepts requests and never answers (httpTimeout); close() drops the held connections
async function silentHttpServer() {
  const server = http.createServer(() => {});
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, close: closer(server) };
}

// answers after `ms`, recording the most requests it held at once (simultaneous)
async function slowHttpServer(status, ms) {
  const counts = { inFlight: 0, peak: 0 };
  const server = http.createServer((req, res) => {
    counts.peak = Math.max(counts.peak, ++counts.inFlight);
    setTimeout(() => {
      counts.inFlight--;
      answering(status)(req, res);
    }, ms);
  });
  await listening(server, 0, '127.0.0.1');
  return { port: server.address().port, counts, close: closer(server) };
}

// openssl is required: the contract allows no skipped scenarios (R2), so a missing tool fails
async function httpsServer(status) {
  const tls = tlsFixture();
  if (!tls) throw new Error('openssl required for the https scenarios (not found on PATH)');
  const server = https.createServer({ key: tls.key, cert: tls.cert }, answering(status));
  await listening(server, 0, '127.0.0.1');
  const close = closer(server);
  return {
    port: server.address().port,
    ca: tls.cert.toString(),
    close: () => close().then(tls.cleanup)
  };
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
  unixServer,
  httpUnixServer
};
