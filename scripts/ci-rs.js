'use strict';

// npm run ci:rs — the Rust gate the CI `rust` job runs on ubuntu, macos and windows:
// fmt, clippy, cargo test, cargo deny, build the host addon, then the whole mocha suite
// with WAIT_ON_ENGINE=rust-strict, then the startup benchmark. No shell, so it behaves the same under Windows cmd.

const childProcess = require('child_process');
const path = require('path');

function steps({ repoRoot }) {
  return [
    { cmd: 'cargo', args: ['fmt', '--all', '--check'] },
    { cmd: 'cargo', args: ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'] },
    { cmd: 'cargo', args: ['test', '--workspace'] },
    { cmd: 'cargo', args: ['deny', 'check'] },
    { cmd: process.execPath, args: [path.join(repoRoot, 'scripts', 'build-napi.js')] },
    {
      cmd: process.execPath,
      args: [require.resolve('mocha/bin/mocha.js'), '--exit', 'test/**/*.mocha.js'],
      env: { ...process.env, WAIT_ON_ENGINE: 'rust-strict' }
    },
    // last, so a benchmark failure never masks a test failure
    { cmd: process.execPath, args: [path.join(repoRoot, 'scripts', 'bench-startup.js')] }
  ];
}

function main() {
  const repoRoot = path.join(__dirname, '..');
  for (const { cmd, args, env } of steps({ repoRoot })) {
    console.log(`> ${[path.basename(cmd), ...args].join(' ')}`);
    const r = childProcess.spawnSync(cmd, args, { cwd: repoRoot, stdio: 'inherit', env });
    if (r.status !== 0) process.exit(r.status || 1);
  }
}

if (require.main === module) main();

module.exports = { steps };
