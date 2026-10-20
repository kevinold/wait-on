'use strict';

// node benchmarks/http-ffi.js [--iterations 200] [--engines js,rust-strict]
// R-L4-8 / KTD11: per-check cost of an http readiness check through the waitOn front door,
// JS engine vs the Rust addon (with and without a JS validateStatus), against one local
// server. "steady" rows time the gaps between polls of ONE waitOn call whose server turns
// ready after N polls, so connection reuse across polls is measured. "ffi noop" isolates
// the bare napi boundary with no HTTP.

const fs = require('fs');
const http = require('http');
const { parseArgs } = require('util');
const waitOn = require('../lib/wait-on');
const { addonPath } = require('../lib/engine');

function summarize(samples) {
  const s = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  const median = s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
  return { median, p95: s[Math.ceil(0.95 * s.length) - 1] };
}

const ms = (start) => Number(process.hrtime.bigint() - start) / 1e6;
const OPTS = { delay: 0, interval: 0, simultaneous: 1, timeout: 30000 };

async function run({ iterations, engines }) {
  // proxy env vars send in-scope checks back to undici (KTD4); keep both engines direct
  for (const name of ['HTTP_PROXY', 'http_proxy', 'HTTPS_PROXY', 'https_proxy']) delete process.env[name];

  let pending = 0; // /steady answers 503 until this many polls have arrived
  let arrivals = [];
  const server = http.createServer((req, res) => {
    if (req.url === '/steady') {
      arrivals.push(process.hrtime.bigint());
      res.statusCode = pending-- > 0 ? 503 : 200;
    }
    res.end();
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}`;

  const perCall = async (extra) => {
    const samples = [];
    for (let i = -1; i < iterations; i++) {
      const start = process.hrtime.bigint();
      await waitOn({ ...OPTS, ...extra, resources: [`${base}/ready`] });
      if (i >= 0) samples.push(ms(start)); // i = -1 is an unrecorded warm-up (addon load, JIT)
    }
    return samples;
  };
  const steady = async () => {
    pending = iterations;
    arrivals = [];
    await waitOn({ ...OPTS, resources: [`${base}/steady`] });
    return arrivals.slice(1).map((t, i) => Number(t - arrivals[i]) / 1e6);
  };

  const rows = [];
  const add = (name, samples) => rows.push({ name, n: samples.length, ...summarize(samples) });
  try {
    for (const engine of engines) {
      if (engine !== 'js' && !fs.existsSync(addonPath(process.env))) {
        console.log(`note: no prebuild at ${addonPath(process.env)}; skipping ${engine}`);
        continue;
      }
      process.env.WAIT_ON_ENGINE = engine; // resolveEngine re-reads env on every waitOn call
      add(engine, await perCall({}));
      if (engine !== 'js') add(`${engine} + validateStatus`, await perCall({ validateStatus: (s) => s === 200 }));
      add(`${engine} steady`, await steady());
      if (engine !== 'js') {
        const { noop } = require(addonPath(process.env));
        const batch = 1000; // ponytail: one noop is below hrtime resolution; sample = mean of a batch
        const samples = [];
        for (let i = 0; i < iterations; i++) {
          const start = process.hrtime.bigint();
          for (let j = 0; j < batch; j++) noop();
          samples.push(ms(start) / batch);
        }
        add(`${engine} ffi noop`, samples);
      }
    }
  } finally {
    delete process.env.WAIT_ON_ENGINE;
    server.closeAllConnections();
    server.close();
  }

  const pad = (s, w) => String(s).padEnd(w);
  console.log(`${pad('configuration', 32)}${pad('n', 6)}${pad('median ms', 12)}p95 ms`);
  for (const r of rows) console.log(`${pad(r.name, 32)}${pad(r.n, 6)}${pad(r.median.toFixed(6), 12)}${r.p95.toFixed(6)}`);
  return rows;
}

async function main() {
  const { values } = parseArgs({
    options: { iterations: { type: 'string', default: '200' }, engines: { type: 'string', default: 'js,rust-strict' } }
  });
  await run({ iterations: Number(values.iterations), engines: values.engines.split(',') });
}

if (require.main === module) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}

module.exports = { summarize, run };
