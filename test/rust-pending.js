'use strict';

// Rust pending list (KD-S2). Tests that cannot pass on the Rust engine yet, by mocha
// full title (describe titles + test title, space-joined). Under WAIT_ON_ENGINE=rust or
// rust-strict each listed test reports pending; a listed title that no suite registers
// fails the run so stale entries cannot pile up. Under js the list has no effect.
// Each lane shrinks this list; it must be empty before the spike PR leaves draft.
const pending = [];

const isRust = () => /^rust(-strict)?$/.test(process.env.WAIT_ON_ENGINE || '');

function createHooks(list) {
  return {
    beforeAll() {
      if (!isRust()) return;
      const titles = new Set();
      this.test.parent.eachTest((t) => titles.add(t.fullTitle()));
      const stale = list.filter((title) => !titles.has(title));
      if (stale.length) {
        throw new Error(`test/rust-pending.js lists tests no suite registers: ${stale.join('; ')}`);
      }
    },
    beforeEach() {
      if (isRust() && list.includes(this.currentTest.fullTitle())) this.skip();
    }
  };
}

module.exports = { pending, createHooks, mochaHooks: createHooks(pending) };
