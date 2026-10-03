'use strict';

// The @engine vocabulary (R5, KTD7): runner-neutral Givens that register resources in
// declaration order, one When that states every timing option, and the outcome. The Node
// runner turns them into resource strings and options for the installed API; the Rust
// runner (L18) builds a WaitSpec from the same words.

const assert = require('assert');
const { Given, When, Then } = require('@cucumber/cucumber');
const servers = require('./servers');

async function serve(world, start) {
  const server = await start();
  world.cleanups.push(server.close);
  return server;
}

function addResource(world, resource, vars = {}) {
  Object.assign(world.vars, vars);
  world.resources.push(resource);
}

Given('a TCP server on a free port', async function () {
  const { port } = await serve(this, () => servers.tcpServer('127.0.0.1'));
  addResource(this, `tcp:127.0.0.1:${port}`, { port });
});

Given('a TCP server on the IPv6 loopback', async function () {
  const { port } = await serve(this, () => servers.tcpServer('::1'));
  addResource(this, `tcp:[::1]:${port}`, { port });
});

Given('nothing listening on a free port', async function () {
  const port = await servers.freePort();
  addResource(this, `tcp:127.0.0.1:${port}`, { port });
});

Given('a unix socket server', async function () {
  const server = await serve(this, () => servers.unixServer(this.dir));
  addResource(this, `socket:${server.path}`, { sock: server.path });
});

Given('nothing listening on a unix socket', function () {
  const sock = servers.socketPath(this.dir);
  addResource(this, `socket:${sock}`, { sock });
});

Given('an HTTP server answering {int}', async function (status) {
  const { port } = await serve(this, () => servers.httpServer(status));
  addResource(this, `http://127.0.0.1:${port}/`, { port });
});

Given('an HTTP server answering {int} to GET', async function (status) {
  const { port } = await serve(this, () => servers.httpServer(status));
  addResource(this, `http-get://127.0.0.1:${port}/`, { port });
});

// strictSSL either way; only a trusted server's certificate is passed as `ca`
Given(/^an HTTPS server answering (\d+), (trusted through its CA|not trusted)$/, async function (status, trust) {
  const server = await serve(this, () => servers.httpsServer(Number(status)));
  const ca = trust === 'not trusted' ? {} : { ca: server.ca };
  this.extraOpts = { ...this.extraOpts, ...ca, strictSSL: true };
  addResource(this, `https://127.0.0.1:${server.port}/`, { port: server.port });
});

Given('an HTTP server on a unix socket answering {int}', async function (status) {
  const server = await serve(this, () => servers.httpUnixServer(this.dir, status));
  addResource(this, `http://unix:${server.path}:/`, { sock: server.path });
});

Given('a command that exits {int}', function (code) {
  addResource(this, `command:node -e "process.exit(${code})"`);
});

const TIMINGS =
  'timeout {int}ms, interval {int}ms, window {int}ms, delay {int}ms, tcp timeout {int}ms and command timeout {int}ms';

function waitStep(reverse) {
  return async function (timeout, interval, window, delay, tcpTimeout, commandTimeout) {
    assert.ok(window >= interval, 'engine scenarios keep window >= interval (no JS-only clamp)');
    const opts = { resources: this.resources, timeout, interval, window, delay, tcpTimeout, commandTimeout, reverse };
    await this.callWaitOn({ ...opts, ...this.extraOpts });
  };
}

When(`I wait with ${TIMINGS}`, waitStep(false));
When(`I wait in reverse with ${TIMINGS}`, waitStep(true));

Then('the wait succeeds', function () {
  assert.strictEqual(this.result.outcome, 'resolved', JSON.stringify(this.result));
});

function failsWith(world, message) {
  assert.strictEqual(world.result.outcome, 'rejected', JSON.stringify(world.result));
  assert.strictEqual(world.normalize(world.result.errorMessage), message);
}

Then('the wait fails with:', function (message) {
  failsWith(this, message);
});

// Scenario Outlines use this form: cucumber-rs rejects a `<resource 1>` docstring there,
// reading it as an Examples column it cannot resolve.
Then('the wait times out naming resource {int}', function (n) {
  failsWith(this, `Timed out waiting for: <resource ${n}>`);
});
