'use strict';

// After every scenario: close what it started, then judge its engine proof (R12).

const fs = require('fs');
const { After, Status } = require('@cucumber/cucumber');
const { routeVerdict } = require('./proof');

After(async function ({ pickle, result }) {
  for (const cleanup of this.cleanups.reverse()) await cleanup();
  try {
    // a failed scenario already says why; a killed child writes no record, so its proof
    // would only add a misleading route failure on top
    if (result.status !== Status.PASSED) return;
    const tags = pickle.tags.map((t) => t.name);
    const route = tags.includes('@route:none') ? 'none' : tags.includes('@route:js') ? 'js' : 'engine';
    const verdict = routeVerdict(this.proofRecords(), { engine: this.engine, route, addonDir: this.addonDir() });
    if (verdict) throw new Error(`route proof failed under ${this.engine}: ${verdict}`);
  } finally {
    fs.rmSync(this.dir, { recursive: true, force: true });
  }
});
