'use strict';

// node prebuild-probe.js [--no-listener] [--timeout <ms>]
// Run from a project that has wait-on installed (ci:rs:package install and container cells).
// Loads the installed wait-on's engine (WAIT_ON_ENGINE=rust-strict throws if the addon cannot load),
// waits on a local tcp listener through the installed API and then the installed CLI, prints one
// JSON line { addonPath, realpath, pkgDir, api, cli, cliError } and exits with the CLI's code
// (api: true or the API's error message; cliError: the CLI's own stderr).
// --no-listener waits on a closed port instead, so the check must time out.

const childProcess = require('child_process');
const fs = require('fs');
const net = require('net');
const path = require('path');

function listen() {
  return new Promise((resolve) => {
    const server = net.createServer().listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function main() {
  const args = process.argv.slice(2);
  const i = args.indexOf('--timeout');
  const timeout = i === -1 ? 5000 : Number(args[i + 1]);
  const cwd = process.cwd();

  const enginePath = require.resolve('wait-on/lib/engine', { paths: [cwd] });
  const pkgDir = path.dirname(path.dirname(enginePath));
  const { resolveEngine, addonPath: addonPathOf } = require(enginePath);
  resolveEngine(process.env);
  const addonPath = addonPathOf(process.env);
  const waitOn = require(pkgDir);

  const server = await listen();
  const { port } = server.address();
  if (args.includes('--no-listener')) await new Promise((resolve) => server.close(resolve));
  const resource = `tcp:127.0.0.1:${port}`;

  let api = true;
  try {
    await waitOn({ resources: [resource], timeout, interval: 100 });
  } catch (err) {
    api = err.message;
    console.error(err.message);
  }
  const cliArgs = [path.join(pkgDir, 'bin', 'wait-on'), '--timeout', String(timeout), '--interval', '100', resource];
  const run = childProcess.spawnSync(process.execPath, cliArgs, { stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
  const cli = run.status;
  const cliError = run.stderr.trim();
  if (cliError) console.error(cliError);

  if (server.listening) server.close();
  console.log(JSON.stringify({ addonPath, realpath: fs.realpathSync(addonPath), pkgDir, api, cli, cliError }));
  process.exitCode = cli === null ? 1 : cli;
}

main().catch((err) => {
  console.error(err.message);
  process.exitCode = 1;
});
