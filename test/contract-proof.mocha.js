'use strict';

// The consumer contract's route verdict (features/support/proof.js): the After hook judges each
// scenario's proof records with it. Its failure cases cannot live in features/, which runs green.

const path = require('path');
const { expect } = require('chai');
const { routeVerdict } = require('../features/support/proof');

const addonDir = path.join(path.sep, 'proj', 'node_modules', 'wait-on', 'prebuilds', 'darwin-arm64');
const addon = path.join(addonDir, 'wait-on.node');
const rec = (dlopened, engineJs) => ({ pid: 1, dlopened, engineJs });

describe('contract route verdict', function () {
  it('should pass a rust-strict wait that dlopened the installed addon and never loaded engine-js', function () {
    expect(routeVerdict([rec([addon], false)], { engine: 'rust-strict', route: 'engine', addonDir })).to.equal(null);
  });

  it('should fail a rust-strict wait whose proof shows engine-js loaded, naming expected and seen', function () {
    const verdict = routeVerdict([rec([addon], true)], { engine: 'rust-strict', route: 'engine', addonDir });
    expect(verdict).to.include('expected the rust engine').and.include('engine-js loaded');
  });

  it('should fail a rust-strict wait with no proof records', function () {
    expect(routeVerdict([], { engine: 'rust-strict', route: 'engine', addonDir })).to.include('no process dlopened');
  });

  it('should fail a js wait whose proof shows a dlopened addon', function () {
    const verdict = routeVerdict([rec([addon], true)], { engine: 'js', route: 'engine', addonDir });
    expect(verdict).to.include('expected no addon').and.include(addon);
  });

  it('should judge records as a whole so a command child without engine-js still passes under js', function () {
    expect(routeVerdict([rec([], true), rec([], false)], { engine: 'js', route: 'engine', addonDir })).to.equal(null);
  });

  it('should fail an addon realpath outside the installed prebuilds dir, naming both paths', function () {
    const stray = path.join(path.sep, 'repo', 'prebuilds', 'darwin-arm64', 'wait-on.node');
    const verdict = routeVerdict([rec([stray], false)], { engine: 'rust-strict', route: 'engine', addonDir });
    expect(verdict).to.include(stray).and.include(addonDir);
  });

  it('should pass @route:none with an empty proof under both engines and fail when an engine loaded', function () {
    for (const engine of ['js', 'rust-strict']) {
      expect(routeVerdict([], { engine, route: 'none', addonDir })).to.equal(null);
      expect(routeVerdict([rec([], false)], { engine, route: 'none', addonDir })).to.equal(null);
      expect(routeVerdict([rec([], true)], { engine, route: 'none', addonDir })).to.include('expected no engine');
    }
  });

  it('should expect engine-js plus the dlopened addon for @route:js under rust-strict', function () {
    expect(routeVerdict([rec([addon], true)], { engine: 'rust-strict', route: 'js', addonDir })).to.equal(null);
    expect(routeVerdict([rec([], true)], { engine: 'rust-strict', route: 'js', addonDir })).to.include('no process dlopened');
    expect(routeVerdict([rec([], true)], { engine: 'js', route: 'js', addonDir })).to.equal(null);
    expect(routeVerdict([rec([], false)], { engine: 'js', route: 'js', addonDir })).to.include('expected engine-js');
  });
});
