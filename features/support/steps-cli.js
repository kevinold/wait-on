'use strict';

// @cli steps: the installed bin/wait-on under node (KTD4, no .cmd shim).

const assert = require('assert');
const { When, Then } = require('@cucumber/cucumber');

When('I run wait-on with {string}', async function (args) {
  await this.cli(this.fill(args).split(' '));
});

Then('it exits {int}', function (code) {
  assert.strictEqual(this.run.code, code, this.run.stdout + this.run.stderr);
});

// stderr beyond the first line is not compared (R2)
Then('it exits {int} with the first stderr line:', function (code, line) {
  assert.strictEqual(this.run.code, code, this.run.stdout + this.run.stderr);
  const first = this.run.stderr.split(/\r?\n/)[0];
  assert.strictEqual(this.normalize(first, { resources: false }), line);
});

Then('stdout starts with {string}', function (text) {
  assert.ok(this.run.stdout.startsWith(text), this.run.stdout);
});
