'use strict';

// @api steps for the option, environment and log-line inventory (U4).

const assert = require('assert');
const fs = require('fs');
const path = require('path');
const { Given, Then } = require('@cucumber/cucumber');
const servers = require('./servers');

// Runs the consumer from a copy whose host addon is gone. No engine can be dlopened there,
// so the route proof judges the run as the JS engine's whatever engine cucumber runs under.
Given('the installed package has no addon for this host', function () {
  this.project = this.noAddonProject();
  this.engine = 'js';
  this.vars.addon = path.join(this.project, 'node_modules', 'wait-on', 'prebuilds', this.hostDir, 'wait-on.node');
});

Given('an HTTP server answering {int} after {int}ms', async function (status, ms) {
  const server = await servers.slowHttpServer(status, ms);
  this.cleanups.push(server.close);
  this.server = server;
  this.vars.port = server.port;
  this.resources.push(`http://127.0.0.1:${server.port}/`);
});

Then(/^the server saw at (most|least) (\d+) requests? in flight$/, function (bound, n) {
  const { peak } = this.server.counts;
  assert.ok(bound === 'most' ? peak <= Number(n) : peak >= Number(n), `peak ${peak} in flight`);
});

// "never" is 5s, past every scenario's timeout: an attempt started just before the wait
// settles is not killed by anyone, so it must end itself. A script, not `node -e "..."`:
// killing that form left the inner node running on macOS.
Given('a command that never exits', function () {
  const script = path.join(this.dir, 'hang.js');
  fs.writeFileSync(script, 'setTimeout(() => {}, 5000);\n');
  this.resources.push(`command:node ${script}`);
});

// log lines carry the runner's pid, which the step cannot know in advance
const logLines = (world) => world.result.lines.map((line) => world.normalize(line).replace(/^wait-on\(\d+\)/, 'wait-on(<pid>)'));

Then('stdout is empty', function () {
  assert.deepStrictEqual(this.result.lines, []);
});

Then('stdout is:', function (text) {
  assert.strictEqual(logLines(this).join('\n'), text);
});

Then('stdout includes the line {string}', function (line) {
  assert.ok(logLines(this).includes(line), logLines(this).join('\n'));
});

// verbose detail is not contract (R2): only that it exists beside the log lines
Then('stdout has lines beyond the log lines', function () {
  const extra = logLines(this).filter((line) => !/^(wait-on|waiting for \d+ resources: )/.test(line));
  assert.ok(extra.length > 0, logLines(this).join('\n'));
});

// for messages whose tail is Node's own text (a require stack with host paths)
Then('the wait rejects with an Error starting with:', function (prefix) {
  assert.strictEqual(this.result.outcome, 'rejected', JSON.stringify(this.result));
  assert.strictEqual(this.result.errorName, 'Error');
  const message = this.normalize(this.result.errorMessage);
  assert.ok(message.startsWith(prefix), `${message}\ndoes not start with\n${prefix}`);
});
