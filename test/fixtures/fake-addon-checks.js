'use strict';

// Stands in for the native addon's tcp/socket checks (network-edge stub).
// WAIT_ON_FAKE_ADDON_ANSWER picks the answer: ready (default) | refused | timeout.
// Every call lands in module.exports.calls and, when WAIT_ON_FAKE_ADDON_LOG names a
// file, as one JSON line there (proof from a CLI subprocess).

const fs = require('fs');

const ANSWERS = {
  ready: { ready: true, timedOut: false, reason: null },
  refused: { ready: false, timedOut: false, reason: 'fake refused' },
  timeout: { ready: false, timedOut: true, reason: 'fake timed out' }
};

function record(call) {
  module.exports.calls.push(call);
  if (process.env.WAIT_ON_FAKE_ADDON_LOG) {
    fs.appendFileSync(process.env.WAIT_ON_FAKE_ADDON_LOG, JSON.stringify(call) + '\n');
  }
  return Promise.resolve({ ...ANSWERS[process.env.WAIT_ON_FAKE_ADDON_ANSWER || 'ready'] });
}

module.exports = {
  version: () => 'fake',
  noop() {},
  tcpCheck: (host, port, timeoutMs) => record({ fn: 'tcpCheck', host, port, timeoutMs }),
  socketCheck: (path) => record({ fn: 'socketCheck', path }),
  calls: []
};
