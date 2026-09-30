'use strict';

// npm run build:napi [-- --target <triple> [napi build args, e.g. -x]]
// Builds crates/wait-on-napi with @napi-rs/cli into target/napi/<dir>/ (never the repo
// root) and copies the addon to prebuilds/<platform>-<arch>[-musl]/wait-on.node, where
// lib/engine.js loads it. Without --target it builds for the rustc host.

const childProcess = require('child_process');
const fs = require('fs');
const path = require('path');
const { prebuildDir } = require('../lib/engine');

const TARGETS = {
  'aarch64-apple-darwin': { platform: 'darwin', arch: 'arm64', musl: false },
  'x86_64-apple-darwin': { platform: 'darwin', arch: 'x64', musl: false },
  'x86_64-unknown-linux-gnu': { platform: 'linux', arch: 'x64', musl: false },
  'aarch64-unknown-linux-gnu': { platform: 'linux', arch: 'arm64', musl: false },
  'x86_64-unknown-linux-musl': { platform: 'linux', arch: 'x64', musl: true },
  'aarch64-unknown-linux-musl': { platform: 'linux', arch: 'arm64', musl: true },
  'x86_64-pc-windows-msvc': { platform: 'win32', arch: 'x64', musl: false },
  'aarch64-pc-windows-msvc': { platform: 'win32', arch: 'arm64', musl: false }
};

function parseArgs(argv) {
  const i = argv.indexOf('--target');
  if (i === -1) return { target: undefined, extraArgs: argv };
  return { target: argv[i + 1], extraArgs: argv.slice(0, i).concat(argv.slice(i + 2)) };
}

function hostTriple(rustcVV) {
  return /^host:\s*(\S+)/m.exec(rustcVV)[1];
}

function planBuild({ target, extraArgs, repoRoot }) {
  const host = TARGETS[target];
  if (!host) {
    throw new Error(`unsupported target ${target}; expected one of ${Object.keys(TARGETS).join(', ')}`);
  }
  const dir = prebuildDir(host);
  const outputDir = path.join(repoRoot, 'target', 'napi', dir);
  return {
    outputDir,
    prebuildPath: path.join(repoRoot, 'prebuilds', dir, 'wait-on.node'),
    napiArgs: [
      'build',
      '--release',
      '--manifest-path',
      path.join(repoRoot, 'crates', 'wait-on-napi', 'Cargo.toml'),
      '--output-dir',
      outputDir,
      '--target',
      target,
      ...extraArgs
    ]
  };
}

function main() {
  const repoRoot = path.join(__dirname, '..');
  const args = parseArgs(process.argv.slice(2));
  const target = args.target || hostTriple(childProcess.execFileSync('rustc', ['-vV'], { encoding: 'utf8' }));
  const plan = planBuild({ target, extraArgs: args.extraArgs, repoRoot });
  const cliDir = path.dirname(require.resolve('@napi-rs/cli/package.json'));
  const napiBin = path.join(cliDir, require('@napi-rs/cli/package.json').bin.napi);

  const r = childProcess.spawnSync(process.execPath, [napiBin, ...plan.napiArgs], { cwd: repoRoot, stdio: 'inherit' });
  if (r.status !== 0) process.exit(r.status || 1);

  const built = fs.readdirSync(plan.outputDir).filter((f) => f.endsWith('.node'));
  if (built.length !== 1) {
    console.error(`expected one .node file in ${plan.outputDir}, found: ${built.join(', ') || 'none'}`);
    process.exit(1);
  }
  fs.mkdirSync(path.dirname(plan.prebuildPath), { recursive: true });
  fs.copyFileSync(path.join(plan.outputDir, built[0]), plan.prebuildPath);
  console.log(`built ${path.relative(repoRoot, plan.prebuildPath)}`);
}

if (require.main === module) main();

module.exports = { planBuild, hostTriple, parseArgs };
