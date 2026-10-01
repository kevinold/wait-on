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

  it('registers the xtask crate and the cargo xtask alias', function () {
    expect(read('Cargo.toml')).to.match(/members\s*=\s*\[[^\]]*"xtask"/);
    expect(read('.cargo/config.toml')).to.match(/^\s*xtask\s*=\s*"run --package xtask --"/m);
    expect(read('xtask/Cargo.toml')).to.match(/^\s*publish\.workspace\s*=\s*true/m);
  });
});
