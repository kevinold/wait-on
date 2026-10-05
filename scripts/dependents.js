'use strict';

// npm run dependents: run published dependents' own commands against a packed wait-on
// tarball. Manifest: test/dependents/dependents.json. See docs/plans/*-dependents-check-plan.md.

const fs = require('fs');
const path = require('path');

const USAGE =
  'usage: npm run dependents -- [--list] [--only <name>] [--include-optional] [--control] [--tgz <path>] [--keep]';

const FIELDS = {
  name: 'string',
  repo: 'string',
  tag: 'string',
  swap: 'string',
  scripts: 'boolean',
  build: 'commands',
  run: 'commands',
  os: 'strings',
  optional: 'boolean'
};
const SWAPS = ['install', 'overrides'];

function isStrings(v) {
  return Array.isArray(v) && v.every((s) => typeof s === 'string');
}

function checkEntry(e) {
  const name = e && typeof e.name === 'string' ? e.name : '?';
  const fail = (msg) => {
    throw new Error(`dependents.json: entry ${name} ${msg}`);
  };
  Object.entries(FIELDS).forEach(([field, type]) => {
    const v = e[field];
    const ok = type === 'commands' || type === 'strings' ? isStrings(v) : typeof v === type;
    if (!ok) fail(`needs ${field} (${type === 'commands' || type === 'strings' ? 'array of strings' : type})`);
  });
  if (!SWAPS.includes(e.swap)) fail(`has swap ${e.swap}, expected one of ${SWAPS.join(', ')}`);
  if (e.run.length === 0) fail('needs at least one run command');
  e.build.concat(e.run).forEach((cmd) => {
    const first = cmd.split(/\s+/)[0];
    if (first !== 'npm' && first !== 'node') fail(`command ${cmd}: commands start with npm or node`);
  });
  return e;
}

function parseManifest(text) {
  let json;
  try {
    json = JSON.parse(text);
  } catch (err) {
    throw new Error(`dependents.json: ${err.message}`);
  }
  if (!Array.isArray(json)) throw new Error('dependents.json: expected an array');
  return json.map(checkEntry);
}

function select(entries, opts, platform) {
  const runsHere = (e) => e.os.length === 0 || e.os.includes(platform);
  if (opts.only) {
    const entry = entries.find((e) => e.name === opts.only);
    if (!entry) {
      throw new Error(`--only ${opts.only}: expected one of ${entries.map((e) => e.name).join(', ')}`);
    }
    if (!runsHere(entry)) {
      throw new Error(`--only ${opts.only}: runs only on ${entry.os.join(', ')}, this host is ${platform}`);
    }
    return [entry];
  }
  return entries.filter((e) => (!e.optional || opts.includeOptional) && runsHere(e));
}

function listText(entries) {
  return entries
    .map((e) => {
      const os = e.os.length ? e.os.join(',') : 'all';
      const head = `${e.name}  ${e.tag}  swap=${e.swap}  os=${os}  optional=${e.optional ? 'yes' : 'no'}`;
      return [head].concat(e.run.map((cmd) => `  ${cmd}`)).join('\n');
    })
    .join('\n\n');
}

function parseArgs(argv) {
  const opts = { list: false, only: undefined, includeOptional: false, control: false, tgz: undefined, keep: false };
  const flags = { '--list': 'list', '--include-optional': 'includeOptional', '--control': 'control', '--keep': 'keep' };
  const values = { '--only': 'only', '--tgz': 'tgz' };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (flags[arg]) {
      opts[flags[arg]] = true;
    } else if (values[arg]) {
      if (i + 1 >= argv.length) throw new Error(`${arg} needs a value\n${USAGE}`);
      opts[values[arg]] = argv[++i];
    } else {
      throw new Error(`unknown argument ${arg}\n${USAGE}`);
    }
  }
  return opts;
}

function cloneArgs(entry, dest) {
  return ['-c', 'core.longpaths=true', 'clone', '--depth', '1', '--branch', entry.tag, entry.repo, dest];
}

function installArgs(entry) {
  return entry.scripts ? ['ci'] : ['ci', '--ignore-scripts'];
}

// target: { tgz: <absolute path> } for the tarball, { version } for the 9.5.1 control
function swapArgs(entry, target) {
  const ignore = entry.scripts ? [] : ['--ignore-scripts'];
  if (entry.swap === 'install') {
    const spec = target.tgz || `wait-on@${target.version}`;
    return [['install', '--no-save', ...ignore, spec]];
  }
  const override = target.tgz ? `file:${target.tgz}` : target.version;
  return [['pkg', 'set', `overrides.wait-on=${override}`], ['install', ...ignore]];
}

// no shell: `npm ...` runs npm-cli.js under node, `node ...` runs as is
function nodeArgs(command, npm) {
  const [first, ...rest] = command.split(/\s+/);
  return first === 'npm' ? [npm, ...rest] : rest;
}

function tgzVersion(file) {
  const m = /^wait-on-(.+)\.tgz$/.exec(path.basename(file));
  if (!m) throw new Error(`${file}: expected the npm pack output, wait-on-<version>.tgz`);
  return m[1];
}

const PROXIES = ['HTTP_PROXY', 'http_proxy', 'HTTPS_PROXY', 'https_proxy', 'NO_PROXY', 'no_proxy'];

// The env a dependent's commands run in: no proxies (they probe localhost), and no
// pre/post lifecycle scripts (start-server-and-test's pretest runs prettier --write).
function runEnv(parent) {
  const env = Object.assign({}, parent);
  PROXIES.forEach((k) => delete env[k]);
  env.npm_config_ignore_scripts = 'true';
  return env;
}

// npm ls exits 1 after a --no-save swap breaks the dependent's exact pin, so only the
// parsed versions are judged: every wait-on at any depth must be the expected one.
function lsVerdict(stdout, version) {
  let tree;
  try {
    tree = JSON.parse(stdout);
  } catch (err) {
    throw new Error(`npm ls output is not JSON: ${err.message}`);
  }
  const found = [];
  const walk = (node, trail) => {
    Object.entries(node.dependencies || {}).forEach(([name, dep]) => {
      const here = trail ? `${trail} > ${name}` : name;
      if (name === 'wait-on') found.push(`${here}@${dep.version}`);
      walk(dep, here);
    });
  };
  walk(tree, '');
  if (found.length === 0) throw new Error('npm ls found no wait-on to swap');
  const stale = found.filter((f) => !f.endsWith(`@${version}`));
  if (stale.length) throw new Error(`swap incomplete, expected ${version}: ${stale.join(', ')}`);
}

// rows: { command, baseline, tarball, control } with control undefined when not run
function verdict(label, rows) {
  const withControl = rows.some((r) => r.control !== undefined);
  const mark = (ok) => (ok ? 'pass' : 'FAIL');
  const width = Math.max(7, ...rows.map((r) => r.command.length));
  const line = (cells) => '  ' + cells.map((c, i) => (i === 0 ? c.padEnd(width) : c.padEnd(9))).join(' ').trimEnd();
  const header = ['command', 'baseline', 'tarball'].concat(withControl ? ['9.5.1'] : [], ['verdict']);
  let code = 0;
  const body = rows.map((r) => {
    let call = 'ok';
    if (!r.tarball) {
      call = r.baseline ? 'regression' : 'pre-existing';
      if (r.baseline) code = 1;
    }
    const cells = [r.command, mark(r.baseline), mark(r.tarball)];
    if (withControl) cells.push(r.control === undefined ? '-' : mark(r.control));
    return line(cells.concat(call));
  });
  return { text: [label, line(header)].concat(body).join('\n') + '\n', code };
}

// Windows holds node_modules files briefly after their processes exit (EBUSY); cleanup
// retries and never fails the run.
function removeDir(dir) {
  try {
    fs.rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 1000 });
  } catch {
    console.error(`could not remove ${dir}; remove it by hand`);
  }
}

function main() {}

if (require.main === module) {
  main();
}

module.exports = {
  parseManifest,
  select,
  listText,
  parseArgs,
  cloneArgs,
  installArgs,
  swapArgs,
  nodeArgs,
  tgzVersion,
  runEnv,
  lsVerdict,
  verdict,
  removeDir
};
