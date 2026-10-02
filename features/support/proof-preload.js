'use strict';

// Engine proof (R11, KTD3). Loaded into every Node process through
// NODE_OPTIONS=--require, so it reaches ESM entries, spawned CLIs and `command:` children.
// With WAIT_ON_PROOF_FILE set, it appends one JSON line at exit:
// {pid, dlopened: [<realpaths of loaded .node files>], engineJs: <lib/engine-js.js was required>}.

const fs = require('fs');
const path = require('path');

const file = process.env.WAIT_ON_PROOF_FILE;
if (file) {
  const dlopened = [];
  const dlopen = process.dlopen;
  process.dlopen = function (module, filename, ...rest) {
    dlopened.push(filename);
    return dlopen.call(this, module, filename, ...rest);
  };
  const engineJs = path.sep + path.join('lib', 'engine-js.js');
  process.on('exit', () => {
    const real = (p) => {
      try {
        return fs.realpathSync(p);
      } catch {
        return p;
      }
    };
    const record = {
      pid: process.pid,
      dlopened: dlopened.map(real),
      engineJs: Object.keys(require.cache).some((k) => k.endsWith(engineJs))
    };
    fs.appendFileSync(file, JSON.stringify(record) + '\n');
  });
}
