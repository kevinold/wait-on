'use strict';

// The contract World (KTD1, KTD4). World parameters come from `cargo xtask contract`:
// {project, fixture, engine, hostDir}. Every child it spawns runs with a per-scenario
// WAIT_ON_PROOF_FILE, which the proof preload (already on NODE_OPTIONS) appends to.

const childProcess = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { World, setWorldConstructor, setDefaultTimeout } = require('@cucumber/cucumber');

// real-clock subprocess runs (AGENTS.md clock exception); scenarios bound their own waits
setDefaultTimeout(30 * 1000);

const RUNNERS = { cjs: 'run.js', esm: 'run.mjs', ts: path.join('dist', 'run.js') };

class ContractWorld extends World {
  constructor(options) {
    super(options);
    const { project, fixture, engine, hostDir } = this.parameters;
    Object.assign(this, { project, fixture, engine, hostDir });
    this.dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-contract-'));
    this.proofFile = path.join(this.dir, 'proof.jsonl');
    this.vars = {}; // placeholder -> actual value, e.g. tmp -> /tmp/.../file
    this.resources = []; // resource strings in declaration order (KTD5 <resource N>)
    this.cleanups = [];
  }

  // `<name>` placeholders in scenario text -> actual values
  fill(text) {
    return text.replace(/<(\w+)>/g, (m, name) => (name in this.vars ? this.vars[name] : m));
  }

  // actual values in output -> placeholders (KTD5); resources first, they contain the vars
  normalize(text) {
    let out = text;
    this.resources.forEach((r, i) => {
      out = out.split(r).join(`<resource ${i + 1}>`);
    });
    for (const [name, value] of Object.entries(this.vars)) out = out.split(String(value)).join(`<${name}>`);
    return out;
  }

  spawn(args, { env = {} } = {}) {
    return new Promise((resolve, reject) => {
      const start = Date.now();
      const child = childProcess.spawn(process.execPath, args, {
        cwd: this.project,
        env: { ...process.env, WAIT_ON_PROOF_FILE: this.proofFile, ...env }
      });
      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (d) => (stdout += d));
      child.stderr.on('data', (d) => (stderr += d));
      child.on('error', reject);
      child.on('close', (code) => resolve({ code, stdout, stderr, elapsedMs: Date.now() - start }));
    });
  }

  // KTD4: the last stdout line is the result, everything before it is log output
  async callWaitOn(opts, { callback = false, env } = {}) {
    const args = [path.join(this.project, RUNNERS[this.fixture]), JSON.stringify(opts)];
    if (callback) args.push('--callback');
    const run = await this.spawn(args, { env });
    const lines = run.stdout.trimEnd().split(/\r?\n/);
    const last = lines.pop();
    try {
      this.result = JSON.parse(last);
    } catch {
      throw new Error(`runner printed no result (exit ${run.code}):\n${run.stdout}${run.stderr}`);
    }
    this.result.lines = lines.filter(Boolean);
    this.run = run;
  }

  async cli(args, { env } = {}) {
    const bin = path.join(this.project, 'node_modules', 'wait-on', 'bin', 'wait-on');
    this.run = await this.spawn([bin, ...args], { env });
  }

  proofRecords() {
    if (!fs.existsSync(this.proofFile)) return [];
    return fs
      .readFileSync(this.proofFile, 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  }

  addonDir() {
    const dir = path.join(this.project, 'node_modules', 'wait-on', 'prebuilds', this.hostDir);
    return fs.existsSync(dir) ? fs.realpathSync(dir) : dir;
  }
}

setWorldConstructor(ContractWorld);
