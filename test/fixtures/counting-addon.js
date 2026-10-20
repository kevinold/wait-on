'use strict';

// Counting addon (KTD7; L13 KD3: the only addon stand-in, kept as the routing spy):
// load via WAIT_ON_NATIVE_LIBRARY_PATH to prove a wait ran in Rust.
// Records every wait call as { type: 'wait', spec, validateStatus } and delegates to the
// real host prebuild when it exists; otherwise answers canned: ready unless
// validateStatus(200) is false for an http resource, then the Rust timeout result.

const fs = require('fs');
const { addonPath } = require('../../lib/engine');

const realPath = addonPath({});
const real = fs.existsSync(realPath) ? require(realPath) : null;
const calls = [];

function wait(spec, log, validateStatus) {
  calls.push({ type: 'wait', spec, validateStatus });
  if (module.exports.constructError) return Promise.reject(module.exports.constructError);
  if (real) return real.wait(spec, log, validateStatus);
  const ready = !validateStatus || spec.resources.every((r) => r.kind !== 'http' || validateStatus(200));
  return Promise.resolve(
    ready ? { ok: true, error: null } : { ok: false, error: `Timed out waiting for: ${spec.resources.map((r) => r.name).join(', ')}` }
  );
}

module.exports = {
  version: () => (real ? real.version() : 'counting'),
  noop() {},
  wait,
  calls,
  constructError: null, // set to make the next wait calls reject (client build failure)
  reset() {
    calls.length = 0;
    module.exports.constructError = null;
  }
};
