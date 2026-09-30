'use strict';

// Counting addon (KTD10): load via WAIT_ON_NATIVE_LIBRARY_PATH to prove which HTTP
// checks reached the addon. Records every HttpChecker construction and check call, and
// delegates to the real host prebuild when it exists; otherwise answers canned results.

const fs = require('fs');
const { addonPath } = require('../../lib/engine');

const realPath = addonPath({});
const real = fs.existsSync(realPath) ? require(realPath) : null;
const calls = [];

class HttpChecker {
  constructor(opts) {
    this.opts = opts;
    calls.push({ type: 'construct', opts });
    if (module.exports.constructError) throw module.exports.constructError;
    this.inner = real ? new real.HttpChecker(opts) : null;
  }

  check(validateStatus) {
    calls.push({ type: 'check', opts: this.opts, validateStatus });
    if (this.inner) return this.inner.check(validateStatus);
    const ok = validateStatus ? validateStatus(200) : true;
    return Promise.resolve({ ok, status: 200, statusText: 'OK' });
  }

  cancel() {
    calls.push({ type: 'cancel', opts: this.opts });
    if (this.inner) this.inner.cancel();
  }
}

module.exports = {
  version: () => (real ? real.version() : 'counting'),
  noop() {},
  HttpChecker,
  calls,
  constructError: null, // set to make the next constructions throw (client build failure)
  reset() {
    calls.length = 0;
    module.exports.constructError = null;
  }
};
