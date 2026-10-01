'use strict';

// npm run bench:startup [-- --runs N] [--record]
// Startup overhead of the Rust engine (rust-strict, so a missing addon fails) over the JS
// engine: median of N interleaved `wait-on tcp:<local listener>` spawns per engine, gated
// by benchmarks/startup-baseline.json's threshold. --record rewrites this host's entry.

const childProcess = require('child_process');
const fs = require('fs');
const net = require('net');
const path = require('path');
const util = require('util');

const execFile = util.promisify(childProcess.execFile);

function median(values) {
  const s = [...values].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

function verdict({ jsMs, rustMs, threshold }) {
  const overheadMs = rustMs - jsMs;
  const allowedMs = Math.max(threshold.relative * jsMs, threshold.floorMs);
  const ok = overheadMs <= allowedMs;
  const f = (n) => n.toFixed(1);
  const message =
    `js median ${f(jsMs)} ms, rust median ${f(rustMs)} ms, ` +
    `overhead ${f(overheadMs)} ms, allowed ${f(allowedMs)} ms: ${ok ? 'ok' : 'FAIL'}`;
  return { ok, overheadMs, allowedMs, message };
}

function withRecording(baseline, key, sample) {
  return { ...baseline, recorded: { ...baseline.recorded, [key]: sample } };
}

async function main() {
  const repoRoot = path.join(__dirname, '..');
  const baselineFile = path.join(repoRoot, 'benchmarks', 'startup-baseline.json');
  const baseline = JSON.parse(fs.readFileSync(baselineFile, 'utf8'));
  const { values } = util.parseArgs({ options: { runs: { type: 'string' }, record: { type: 'boolean' } } });
  const runs = values.runs === undefined ? baseline.runs : Number(values.runs);
  if (!(Number.isInteger(runs) && runs >= 1)) throw new Error(`--runs must be a positive integer, got ${values.runs}`);

  // Async spawns keep this listener's accept loop running, so any --runs fits the backlog.
  const server = net.createServer((socket) => socket.destroy());
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const args = [path.join(repoRoot, 'bin', 'wait-on'), `tcp:127.0.0.1:${server.address().port}`, '-t', '10000'];
  const time = async (engine) => {
    const start = process.hrtime.bigint();
    try {
      await execFile(process.execPath, args, {
        cwd: repoRoot,
        env: { ...process.env, WAIT_ON_ENGINE: engine },
        timeout: 30000
      });
    } catch (err) {
      throw new Error(
        `WAIT_ON_ENGINE=${engine} run failed (exit ${err.code ?? err.signal}): ${err.stderr || err.message}`
      );
    }
    return Number(process.hrtime.bigint() - start) / 1e6;
  };

  const times = { js: [], 'rust-strict': [] };
  try {
    for (const engine of Object.keys(times)) await time(engine); // untimed warm-up
    // interleaved so runner drift hits both engines equally
    for (let i = 0; i < runs; i++) for (const engine of Object.keys(times)) times[engine].push(await time(engine));
  } finally {
    server.close();
  }

  const jsMs = median(times.js);
  const rustMs = median(times['rust-strict']);
  const v = verdict({ jsMs, rustMs, threshold: baseline.threshold });
  console.log(`bench:startup (${runs} runs per engine) ${v.message}`);
  if (values.record) {
    const r = (n) => Math.round(n * 10) / 10;
    const sample = {
      jsMs: r(jsMs),
      rustMs: r(rustMs),
      overheadMs: r(v.overheadMs),
      date: new Date().toISOString().slice(0, 10)
    };
    const key = `${process.platform}-${process.arch}`;
    fs.writeFileSync(baselineFile, JSON.stringify(withRecording(baseline, key, sample), null, 2) + '\n');
    console.log(`recorded ${key} in ${path.relative(repoRoot, baselineFile)}`);
  }
  if (!v.ok) process.exitCode = 1;
}

if (require.main === module) {
  main().catch((err) => {
    console.error(err.message);
    process.exitCode = 1;
  });
}

module.exports = { median, verdict, withRecording };
