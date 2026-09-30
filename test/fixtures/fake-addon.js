'use strict';

// Stands in for the native addon so the addon-present branch runs without a build.
// fileSize records each probed path and answers a constant (WAIT_ON_FAKE_FILE_SIZE, default 1)
// that JS cannot produce for a missing file, so a success proves the addon was asked.
// runCommand records { command, timeoutMs } and answers ok:true without spawning
// (ok:false when WAIT_ON_FAKE_COMMAND_OK=0), an outcome JS cannot give for the test commands.
const calls = [];

module.exports = {
  version: () => 'fake',
  noop() {},
  calls,
  async fileSize(filePath) {
    calls.push(filePath);
    return Number(process.env.WAIT_ON_FAKE_FILE_SIZE ?? 1);
  },
  async runCommand(command, timeoutMs) {
    calls.push({ command, timeoutMs });
    return { ok: process.env.WAIT_ON_FAKE_COMMAND_OK !== '0', stdout: 'fake', error: 'fake' };
  }
};
