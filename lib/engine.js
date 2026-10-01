'use strict';

// Engine selection (KD-S1). WAIT_ON_ENGINE: unset/''/js -> JS engine, addon untouched;
// rust -> load the native addon, fall back to JS silently if it cannot load;
// rust-strict -> a load failure is an error. WAIT_ON_NATIVE_LIBRARY_PATH (absolute)
// overrides the addon path; it is a dev/test hook, not a public option.

const path = require('path');

const ENGINES = ['js', 'rust', 'rust-strict'];

function prebuildDir({ platform, arch, musl }) {
  return `${platform}-${arch}${musl ? '-musl' : ''}`;
}

function isMusl({ platform, report }) {
  return platform === 'linux' && !report().header.glibcVersionRuntime;
}

let hostMusl; // libc cannot change while the process runs, so detect it once
function hostIsMusl() {
  if (hostMusl === undefined) {
    hostMusl = isMusl({ platform: process.platform, report: process.report.getReport.bind(process.report) });
  }
  return hostMusl;
}

function addonPath(env) {
  if (env.WAIT_ON_NATIVE_LIBRARY_PATH) return env.WAIT_ON_NATIVE_LIBRARY_PATH;
  const dir = prebuildDir({ platform: process.platform, arch: process.arch, musl: hostIsMusl() });
  return path.join(__dirname, '..', 'prebuilds', dir, 'wait-on.node');
}

function resolveEngine(env) {
  const value = env.WAIT_ON_ENGINE || 'js';
  if (!ENGINES.includes(value)) {
    throw new Error(`WAIT_ON_ENGINE="${value}" is not one of ${ENGINES.join(', ')}`);
  }
  if (value === 'js') return { engine: 'js', addon: null, loadError: null };
  const file = addonPath(env);
  let addon;
  try {
    addon = require(file);
  } catch (loadError) {
    if (value === 'rust-strict') {
      throw new Error(`WAIT_ON_ENGINE=rust-strict: failed to load the native addon at ${file}: ${loadError.message}`);
    }
    return { engine: 'js', addon: null, loadError };
  }
  // KTD6: an addon without wait (a stale prebuild) cannot run the loop, so it is a load failure
  if (typeof addon.wait !== 'function') {
    if (value === 'rust-strict') throw new Error(`WAIT_ON_ENGINE=rust-strict: the native addon at ${file} has no wait export`);
    return { engine: 'js', addon: null, loadError: new Error(`the native addon at ${file} has no wait export`) };
  }
  return { engine: 'rust', addon, loadError: null };
}

module.exports = { resolveEngine, prebuildDir, isMusl, addonPath };
