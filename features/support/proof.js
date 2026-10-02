'use strict';

// Judges one scenario's proof records (one per Node process, written by proof-preload.js)
// against the route the scenario expects (R12). Records are judged as a whole: a `command:`
// child never loads an engine, so "every record" rules would fail correct scenarios.
// route: 'engine' (the engine under test ran the wait), 'js' (@route:js) or 'none' (@route:none).
// Returns null on pass, else the reason.

const path = require('path');

const inside = (dir, file) => {
  const rel = path.relative(dir, file);
  return rel !== '' && !rel.startsWith('..') && !path.isAbsolute(rel);
};

function routeVerdict(records, { engine, route, addonDir }) {
  const dlopened = records.flatMap((r) => r.dlopened);
  const engineJs = records.some((r) => r.engineJs);
  const seen = `seen: dlopened [${dlopened.join(', ')}], engine-js ${engineJs ? 'loaded' : 'not loaded'}`;

  if (route === 'none') {
    return dlopened.length || engineJs ? `expected no engine to load (@route:none); ${seen}` : null;
  }
  const stray = dlopened.find((file) => !inside(addonDir, file));
  if (stray) return `dlopened ${stray}, expected the addon inside ${addonDir}`;
  // the rust engine loads before routing, so @route:js under rust-strict still dlopens it
  if (engine === 'rust-strict' && !dlopened.length) return `no process dlopened the addon under rust-strict; ${seen}`;
  if (engine === 'js' && dlopened.length) return `expected no addon under js; ${seen}`;
  if (route === 'js' || engine === 'js') {
    return engineJs ? null : `expected engine-js to run the wait; ${seen}`;
  }
  return engineJs ? `expected the rust engine to run the wait; ${seen}` : null;
}

module.exports = { routeVerdict };
