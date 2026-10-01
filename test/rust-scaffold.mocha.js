'use strict';

// Toolchain pin (PO21): the Rust version rust-toolchain.toml installs must be
// the workspace MSRV, and the lockfile is committed.

const fs = require('fs');
const path = require('path');
const { describe, it } = require('mocha');
const { expect } = require('chai');

const root = path.join(__dirname, '..');
const read = (f) => fs.readFileSync(path.join(root, f), 'utf8');

describe('rust scaffold', function () {
  it('pins the same Rust version in rust-toolchain.toml and the workspace rust-version', function () {
    const channel = /^\s*channel\s*=\s*"([^"]+)"/m.exec(read('rust-toolchain.toml'));
    const msrv = /^\s*rust-version\s*=\s*"([^"]+)"/m.exec(read('Cargo.toml'));
    expect(channel && channel[1]).to.match(/^\d+\.\d+\.\d+$/);
    expect(msrv && msrv[1]).to.equal(channel[1]);
    expect(fs.existsSync(path.join(root, 'Cargo.lock'))).to.equal(true);
  });

  it('installs llvm-tools-preview for the coverage gate', function () {
    const components = /^\s*components\s*=\s*\[([^\]]*)\]/m.exec(read('rust-toolchain.toml'));
    expect(components && components[1]).to.include('"llvm-tools-preview"');
  });

  it('registers the xtask crate and the cargo xtask alias', function () {
    expect(read('Cargo.toml')).to.match(/members\s*=\s*\[[^\]]*"xtask"/);
    expect(read('.cargo/config.toml')).to.match(/^\s*xtask\s*=\s*"run --package xtask --"/m);
    expect(read('xtask/Cargo.toml')).to.match(/^\s*publish\.workspace\s*=\s*true/m);
  });

  it('routes the Rust npm scripts through cargo xtask and keeps no ported JS in scripts/', function () {
    const { scripts } = JSON.parse(read('package.json'));
    // vet before cargo builds xtask (KD4); the 100% coverage gate runs last (L13 KTD1)
    expect(scripts['ci:rs']).to.equal(
      'cargo vet --locked && cargo xtask ci && cargo xtask cov --exclude xtask --fail-under-lines 100 --fail-under-regions 100',
    );
    expect(scripts['build:napi']).to.equal('cargo xtask build-napi');
    expect(scripts['ci:rs:package']).to.equal('cargo xtask package');
    expect(scripts['bench:startup']).to.equal('cargo xtask bench-startup');
    expect(scripts.lint).to.include('"xtask/**/*.js"');
    expect(scripts.lint).to.not.include('scripts/');
    expect(fs.readdirSync(path.join(root, 'scripts'))).to.deep.equal(['reindex-codebase-memory.sh']);
    expect(fs.existsSync(path.join(__dirname, 'scripts.mocha.js'))).to.equal(false);
  });
});
