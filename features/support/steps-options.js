'use strict';

// @api steps for the option, environment and log-line inventory (U4).

const assert = require('assert');
const path = require('path');
const { Given, Then } = require('@cucumber/cucumber');

// Runs the consumer from a copy whose host addon is gone. No engine can be dlopened there,
// so the route proof judges the run as the JS engine's whatever engine cucumber runs under.
Given('the installed package has no addon for this host', function () {
  this.project = this.noAddonProject();
  this.engine = 'js';
  this.vars.addon = path.join(this.project, 'node_modules', 'wait-on', 'prebuilds', this.hostDir, 'wait-on.node');
});

// for messages whose tail is Node's own text (a require stack with host paths)
Then('the wait rejects with an Error starting with:', function (prefix) {
  assert.strictEqual(this.result.outcome, 'rejected', JSON.stringify(this.result));
  assert.strictEqual(this.result.errorName, 'Error');
  const message = this.normalize(this.result.errorMessage);
  assert.ok(message.startsWith(prefix), `${message}\ndoes not start with\n${prefix}`);
});
