'use strict';

const childProcess = require('child_process');
const path = require('path');

const CLI_PATH = path.resolve(__dirname, '../../bin/wait-on');
const ENGINE_VARS = ['WAIT_ON_ENGINE', 'WAIT_ON_NATIVE_LIBRARY_PATH', 'WAIT_ON_FAKE_FILE_SIZE'];

// The engine vars plus any other key passed: each is set to vars[k], or unset when absent.
function keysOf(vars) {
  return [...new Set([...ENGINE_VARS, ...Object.keys(vars)])];
}

function applyVars(env, keys, vars) {
  for (const k of keys) {
    if (vars[k] === undefined) delete env[k];
    else env[k] = vars[k];
  }
}

// Run fn with process.env set per vars, then restore.
async function withEnv(vars, fn) {
  const keys = keysOf(vars);
  const saved = Object.fromEntries(keys.map((k) => [k, process.env[k]]));
  applyVars(process.env, keys, vars);
  try {
    return await fn();
  } finally {
    applyVars(process.env, keys, saved);
  }
}

// Spawn the CLI with args and the env set per vars.
function runCLI(vars, args) {
  const env = { ...process.env };
  applyVars(env, keysOf(vars), vars);
  const r = childProcess.spawnSync(process.execPath, [CLI_PATH, ...args], { env, encoding: 'utf8' });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
}

module.exports = { withEnv, runCLI };
