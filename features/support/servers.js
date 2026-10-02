'use strict';

// Local servers the steps start (R10): ephemeral ports, temp-path unix sockets or Windows
// named pipes. Each returns {port | path, close}.

const http = require('http');
const https = require('https');
const net = require('net');
const path = require('path');
const tlsFixture = require('../../test/helpers/tls-fixture');

let pipes = 0;
// Windows has no unix domain sockets; Node listens on a named pipe there instead
// (the \\?\pipe\<dir> form test/helpers/tls-fixture.js uses)
function socketPath(dir) {
  pipes += 1;
  const name = `sock-${pipes}`;
  return process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, name) : path.join(dir, name);
}

const closer = (server) => () => new Promise((resolve) => server.close(() => resolve()));

function listen(server, ...args) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(...args, () => resolve(server));
  });
}

async function tcpServer(host) {
  const sockets = new Set();
  const server = net.createServer((s) => {
    sockets.add(s);
    s.on('close', () => sockets.delete(s));
  });
  await listen(server, 0, host);
  return {
    port: server.address().port,
    close: () => {
      for (const s of sockets) s.destroy();
      return closer(server)();
    }
  };
}

// a port nothing listens on: bind, read the port, close
async function freePort() {
  const server = await listen(net.createServer(), 0, '127.0.0.1');
  const { port } = server.address();
  await closer(server)();
  return port;
}

const answering = (status) => (req, res) => {
  res.statusCode = status;
  res.end();
};

async function httpServer(status) {
  const server = http.createServer(answering(status));
  server.keepAliveTimeout = 1;
  await listen(server, 0, '127.0.0.1');
  return { port: server.address().port, close: () => (server.closeAllConnections(), closer(server)()) };
}

// openssl is required: the contract allows no skipped scenarios (R2), so a missing tool fails
async function httpsServer(status) {
  const tls = tlsFixture();
  if (!tls) throw new Error('openssl required for the https scenarios (not found on PATH)');
  const server = https.createServer({ key: tls.key, cert: tls.cert }, answering(status));
  await listen(server, 0, '127.0.0.1');
  return {
    port: server.address().port,
    ca: tls.cert.toString(),
    close: () => {
      server.closeAllConnections();
      tls.cleanup();
      return closer(server)();
    }
  };
}

async function unixServer(dir) {
  const file = socketPath(dir);
  const server = await listen(net.createServer((s) => s.end()), file);
  return { path: file, close: closer(server) };
}

async function httpUnixServer(dir, status) {
  const file = socketPath(dir);
  const server = http.createServer(answering(status));
  await listen(server, file);
  return {
    path: file,
    close: () => {
      server.closeAllConnections();
      return closer(server)();
    }
  };
}

module.exports = { socketPath, tcpServer, freePort, httpServer, httpsServer, unixServer, httpUnixServer };
