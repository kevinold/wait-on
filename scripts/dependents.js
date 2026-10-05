'use strict';

// npm run dependents: run published dependents' own commands against a packed wait-on
// tarball. Manifest: test/dependents/dependents.json. See docs/plans/*-dependents-check-plan.md.

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

function main() {}

if (require.main === module) {
  main();
}

module.exports = { parseManifest, select, listText, parseArgs };
