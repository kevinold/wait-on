'use strict';

// Stands in for the native addon so the Rust-shim branch runs without a build (KTD7).
// wait(spec) records the spec in calls and answers ready, or with
// WAIT_ON_FAKE_ADDON_ANSWER=timeout the Rust timeout result naming every resource.
// With WAIT_ON_FAKE_ADDON_LOG set, each call also lands there as one JSON line (proof
// from a CLI subprocess).
const fs = require('fs');

const calls = [];

module.exports = {
  version: () => 'fake',
  noop() {},
  calls,
  async wait(spec) {
    calls.push(spec);
    if (process.env.WAIT_ON_FAKE_ADDON_LOG) fs.appendFileSync(process.env.WAIT_ON_FAKE_ADDON_LOG, JSON.stringify(spec) + '\n');
    if (process.env.WAIT_ON_FAKE_ADDON_ANSWER === 'timeout') {
      return { ok: false, error: `Timed out waiting for: ${spec.resources.map((r) => r.name).join(', ')}` };
    }
    return { ok: true, error: null };
  }
};
