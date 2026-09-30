'use strict';

// npm run ci:rs:package [-- --host-only]
// The CI `package` job's hook: with every target's prebuild downloaded into prebuilds/, refuse a
// partial bundle, npm pack once, check the tarball's files and manifest, print sizes, write
// SHA256SUMS next to wait-on-*.tgz, then install the tarball (npm, npm --omit=optional, pnpm; scripts
// disabled) and prove each install loads the host addon from inside the installed package, then run
// AE1 in read-only, no-network glibc and musl containers (skipped without docker, except in CI).
// --host-only (developer runs) requires only the host prebuild.
// No shell: npm runs as `node $npm_execpath`, so it behaves the same under Windows cmd.

const childProcess = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { TARGETS } = require('./build-napi');
const { prebuildDir, isMusl } = require('../lib/engine');

const ADDON = 'wait-on.node';
const NOT_SHIPPED = /^(target|crates|scripts|docs|test|benchmarks)\/|^Cargo\./;
const INSTALL_SCRIPTS = ['preinstall', 'install', 'postinstall', 'prepare'];
const PNPM_VERSION = '10.34.6';

function expectedPrebuildDirs() {
  return Object.values(TARGETS).map(prebuildDir);
}

function hostDir() {
  const report = process.report.getReport.bind(process.report);
  return prebuildDir({ platform: process.platform, arch: process.arch, musl: isMusl({ platform: process.platform, report }) });
}

function requiredDirs({ hostOnly }) {
  return hostOnly ? [hostDir()] : expectedPrebuildDirs();
}

function missingPrebuilds({ prebuildsRoot, dirs }) {
  return dirs.filter((dir) => !fs.existsSync(path.join(prebuildsRoot, dir, ADDON)));
}

function formatMissing(missing) {
  return `missing prebuilds (download every napi artifact, or pass --host-only locally):\n${missing
    .map((dir) => `  prebuilds/${dir}/${ADDON}`)
    .join('\n')}`;
}

function checkPack(packJson, dirs) {
  const paths = packJson.files.map((f) => f.path);
  return [
    ...dirs.filter((dir) => !paths.includes(`prebuilds/${dir}/${ADDON}`)).map((dir) => `not packed: prebuilds/${dir}/${ADDON}`),
    ...paths.filter((p) => NOT_SHIPPED.test(p)).map((p) => `must not ship: ${p}`)
  ];
}

function checkManifest(manifest) {
  const scripts = manifest.scripts || {};
  return [
    ...INSTALL_SCRIPTS.filter((name) => scripts[name] !== undefined).map((name) => `lifecycle script "${name}" is declared`),
    ...(manifest.optionalDependencies ? ['optionalDependencies is declared'] : [])
  ];
}

function sizeReport({ size, unpackedSize, files }) {
  const targets = files
    .filter((f) => f.path.startsWith('prebuilds/') && f.path.endsWith(`/${ADDON}`))
    .map((f) => ({ dir: f.path.split('/')[1], size: f.size }));
  const addonBytes = targets.reduce((sum, t) => sum + t.size, 0);
  return { packed: size, unpacked: unpackedSize, jsOnlyUnpacked: unpackedSize - addonBytes, targets };
}

function sha256sumsLine(file) {
  const hex = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  return `${hex}  ${path.basename(file)}\n`;
}

// One cell per install shape, all with lifecycle scripts disabled. pnpm comes from the registry via
// npm exec (no devDependency, no corepack). The addon override is scrubbed so it cannot stand in
// for the installed prebuild.
function installCells({ tgz, npmExecPath, env }) {
  const cellEnv = { ...env };
  delete cellEnv.WAIT_ON_NATIVE_LIBRARY_PATH;
  const npmInstall = [npmExecPath, 'install', '--ignore-scripts', '--no-audit', '--no-fund'];
  return [
    { name: 'npm', args: [...npmInstall, tgz] },
    { name: 'npm-omit-optional', args: [...npmInstall, '--omit=optional', tgz] },
    {
      name: 'pnpm',
      args: [npmExecPath, 'exec', '--yes', '--package', `pnpm@${PNPM_VERSION}`, '--', 'pnpm', 'add', tgz, '--ignore-scripts']
    }
  ].map((cell) => ({ ...cell, cmd: process.execPath, env: cellEnv }));
}

function assertInstalledAddon({ realpath, projectRoot, dir }) {
  const root = fs.realpathSync(projectRoot) + path.sep;
  const tail = path.join('prebuilds', dir, ADDON);
  if (!realpath.startsWith(root) || !realpath.endsWith(path.sep + tail)) {
    throw new Error(`loaded ${realpath}, expected ${tail} inside the installed package under ${root}`);
  }
}

// AE1: install the tarball at image build time with ignore-scripts=true, then run with a read-only
// root and no network. The probe serves its own tcp listener inside the container; the timeout
// cells wait on a closed port so the check must poll and time out.
function containerCells({ arch }) {
  const images = [
    // ponytail: trixie (glibc 2.41) because the gnu addons need GLIBC_2.39 (built on ubuntu-24.04);
    // lowering that floor is a napi build change, see docs/guides/releasing.md.
    { libc: 'glibc', image: 'node:24-trixie-slim', expectedDir: `linux-${arch}` },
    { libc: 'musl', image: 'node:24-alpine', expectedDir: `linux-${arch}-musl` }
  ];
  return images.flatMap(({ libc, image, expectedDir }) => {
    const tag = `wait-on-ae1-${libc}`;
    const dockerfile = [
      `FROM ${image}`,
      'WORKDIR /app',
      'COPY .npmrc package.json wait-on.tgz prebuild-probe.js ./',
      'RUN npm install --omit=optional --no-audit --no-fund ./wait-on.tgz',
      ''
    ].join('\n');
    const run = ['run', '--rm', '--read-only', '--network', 'none', '-e', 'WAIT_ON_ENGINE=rust-strict', tag];
    const probe = ['node', '/app/prebuild-probe.js'];
    const base = { image, tag, expectedDir, dockerfile, npmrc: 'ignore-scripts=true\n' };
    return [
      { ...base, name: `${libc}-ready`, expectReady: true, runArgs: [...run, ...probe] },
      { ...base, name: `${libc}-timeout`, expectReady: false, runArgs: [...run, ...probe, '--no-listener', '--timeout', '1000'] }
    ];
  });
}

function dockerDecision({ dockerFound, ci }) {
  if (dockerFound) return { action: 'run' };
  const reason = 'docker not found: read-only container cells (AE1)';
  return ci ? { action: 'fail', reason: `${reason} must run in CI` } : { action: 'skip', reason: `${reason} skipped` };
}

function parseArgs(argv) {
  return { hostOnly: argv.includes('--host-only') };
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

function main() {
  const repoRoot = path.join(__dirname, '..');
  const { hostOnly } = parseArgs(process.argv.slice(2));
  const npm = process.env.npm_execpath;
  if (!npm) fail('run this through npm: npm run ci:rs:package [-- --host-only]');

  const dirs = requiredDirs({ hostOnly });
  const missing = missingPrebuilds({ prebuildsRoot: path.join(repoRoot, 'prebuilds'), dirs });
  if (missing.length) fail(formatMissing(missing));

  for (const f of fs.readdirSync(repoRoot)) {
    if (/^wait-on-.*\.tgz$/.test(f)) fs.rmSync(path.join(repoRoot, f));
  }
  const packed = childProcess.spawnSync(process.execPath, [npm, 'pack', '--json'], { cwd: repoRoot, encoding: 'utf8' });
  if (packed.status !== 0) fail(`npm pack failed:\n${packed.stderr}`);
  const packJson = JSON.parse(packed.stdout)[0];
  const tgz = path.join(repoRoot, packJson.filename);

  const problems = [...checkPack(packJson, dirs), ...checkManifest(require(path.join(repoRoot, 'package.json')))];
  if (problems.length) fail(`tarball check failed:\n  ${problems.join('\n  ')}`);

  const sizes = sizeReport(packJson);
  console.log(`size report for ${packJson.filename}`);
  console.log(`  packed           ${sizes.packed}`);
  console.log(`  unpacked         ${sizes.unpacked}`);
  console.log(`  js-only unpacked ${sizes.jsOnlyUnpacked}`);
  for (const t of sizes.targets) console.log(`  ${t.dir.padEnd(16)} ${t.size}`);

  fs.writeFileSync(path.join(repoRoot, 'SHA256SUMS'), sha256sumsLine(tgz));
  console.log(`wrote ${packJson.filename} and SHA256SUMS`);

  const probe = path.join(__dirname, 'prebuild-probe.js');
  for (const cell of installCells({ tgz, npmExecPath: npm, env: process.env })) {
    const project = fs.mkdtempSync(path.join(os.tmpdir(), `wait-on-${cell.name}-`));
    fs.writeFileSync(path.join(project, 'package.json'), '{"name":"probe","private":true}\n');
    const installed = childProcess.spawnSync(cell.cmd, cell.args, { cwd: project, env: cell.env, stdio: 'inherit' });
    if (installed.status !== 0) fail(`${cell.name}: install failed`);
    const env = { ...cell.env, WAIT_ON_ENGINE: 'rust-strict' };
    const run = childProcess.spawnSync(process.execPath, [probe], { cwd: project, env, encoding: 'utf8' });
    if (run.status !== 0) fail(`${cell.name}: probe exited ${run.status}\n${run.stdout}${run.stderr}`);
    const { realpath } = JSON.parse(run.stdout.trim());
    assertInstalledAddon({ realpath, projectRoot: project, dir: hostDir() });
    console.log(`${cell.name}: loaded ${realpath}`);
    fs.rmSync(project, { recursive: true, force: true });
  }

  const ci = Boolean(process.env.CI);
  const decision = dockerDecision({ dockerFound: !childProcess.spawnSync('docker', ['--version']).error, ci });
  if (decision.action === 'fail') fail(decision.reason);
  if (decision.action === 'skip') return console.log(decision.reason);
  const cells = containerCells({ arch: process.arch });
  const unpacked = cells.filter((c) => !packJson.files.some((f) => f.path === `prebuilds/${c.expectedDir}/${ADDON}`));
  if (unpacked.length) {
    const reason = `no ${[...new Set(unpacked.map((c) => c.expectedDir))].join(', ')} prebuild: container cells (AE1)`;
    if (ci) fail(`${reason} must run in CI`);
    return console.log(`${reason} skipped`);
  }

  for (const cell of cells.filter((c) => c.expectReady)) {
    const context = fs.mkdtempSync(path.join(os.tmpdir(), `${cell.tag}-`));
    fs.writeFileSync(path.join(context, 'Dockerfile'), cell.dockerfile);
    fs.writeFileSync(path.join(context, '.npmrc'), cell.npmrc);
    fs.writeFileSync(path.join(context, 'package.json'), '{"name":"ae1","private":true}\n');
    fs.copyFileSync(tgz, path.join(context, 'wait-on.tgz'));
    fs.copyFileSync(probe, path.join(context, 'prebuild-probe.js'));
    const built = childProcess.spawnSync('docker', ['build', '-t', cell.tag, context], { stdio: 'inherit' });
    if (built.status !== 0) fail(`${cell.tag}: docker build failed`);
    fs.rmSync(context, { recursive: true, force: true });
  }
  for (const cell of cells) {
    const run = childProcess.spawnSync('docker', cell.runArgs, { encoding: 'utf8' });
    const output = `${run.stdout}${run.stderr}`;
    const passed = cell.expectReady
      ? run.status === 0 && JSON.parse(run.stdout.trim()).realpath.endsWith(`/prebuilds/${cell.expectedDir}/${ADDON}`)
      : run.status !== 0 && run.stderr.includes('Timed out waiting for');
    if (!passed) fail(`${cell.name}: unexpected result (exit ${run.status})\n${output}`);
    console.log(`${cell.name}: ok (exit ${run.status})\n${output.trim()}`);
  }
}

if (require.main === module) main();

module.exports = {
  expectedPrebuildDirs,
  requiredDirs,
  missingPrebuilds,
  formatMissing,
  checkPack,
  checkManifest,
  sizeReport,
  sha256sumsLine,
  installCells,
  assertInstalledAddon,
  containerCells,
  dockerDecision,
  parseArgs,
  PNPM_VERSION
};
