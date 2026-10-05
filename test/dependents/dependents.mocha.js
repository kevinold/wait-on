'use strict';

const fs = require('fs');
const path = require('path');

const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const chai = require('chai');
const expect = chai.expect;

const dependents = require('../../scripts/dependents');

const MANIFEST_PATH = path.join(__dirname, 'dependents.json');
const manifestText = () => fs.readFileSync(MANIFEST_PATH, 'utf8');

const VALID = {
  name: 'x',
  repo: 'https://example.com/x.git',
  tag: 'v1',
  swap: 'install',
  scripts: false,
  build: [],
  run: ['npm run demo'],
  os: [],
  optional: false
};
const manifestWith = (patch) => JSON.stringify([Object.assign({}, VALID, patch)]);
const names = (entries) => entries.map((e) => e.name);

describe('dependents: manifest', function () {
  it('should parse the committed manifest into the anchor and the optional linux entry', function () {
    const entries = dependents.parseManifest(manifestText());
    expect(names(entries)).to.eql(['start-server-and-test', 'jest-dev-server']);
    const [sst, jds] = entries;
    expect(sst).to.include({ tag: 'v3.0.12', swap: 'install', scripts: false, optional: false });
    expect(sst.os).to.eql([]);
    expect(sst.build).to.eql([]);
    expect(jds).to.include({ tag: 'v11.0.0', swap: 'overrides', scripts: true, optional: true });
    expect(jds.os).to.eql(['linux']);
    expect(jds.build).to.eql(['npm run build']);
  });

  it('should run the anchor spec and demo scripts, never its npm test', function () {
    const [sst] = dependents.parseManifest(manifestText());
    expect(sst.run[0]).to.equal('node node_modules/mocha/bin/mocha.js test/helper src/*-spec.js');
    expect(sst.run).to.include('npm run demo-multiple');
    ['npm test', 'npm run test', 'npm run demo4', 'npm run demo8', 'npm run demo10'].forEach((cmd) =>
      expect(sst.run).to.not.include(cmd)
    );
  });

  it('should reject text that is not JSON', function () {
    expect(() => dependents.parseManifest('{nope')).to.throw(/dependents\.json/);
  });

  it('should reject a manifest that is not an array', function () {
    expect(() => dependents.parseManifest('{}')).to.throw(/expected an array/);
  });

  it('should name the entry and the missing field', function () {
    expect(() => dependents.parseManifest('[{"name":"x"}]')).to.throw(/x.*repo/);
  });

  it('should reject an os that is not an array', function () {
    expect(() => dependents.parseManifest(manifestWith({ os: 'linux' }))).to.throw(/x.*os/);
  });

  it('should reject an unknown swap mode', function () {
    expect(() => dependents.parseManifest(manifestWith({ swap: 'link' }))).to.throw(/swap.*install, overrides/);
  });

  it('should reject an empty run list', function () {
    expect(() => dependents.parseManifest(manifestWith({ run: [] }))).to.throw(/x.*run/);
  });

  it('should reject a command that does not start with npm or node', function () {
    expect(() => dependents.parseManifest(manifestWith({ run: ['curl http://x'] }))).to.throw(
      /curl http:\/\/x.*npm or node/
    );
  });
});

describe('dependents: select', function () {
  const entries = () => dependents.parseManifest(manifestText());

  it('should pick only required entries by default', function () {
    expect(names(dependents.select(entries(), {}, 'linux'))).to.eql(['start-server-and-test']);
  });

  it('should add optional entries on their own OS only', function () {
    const opts = { includeOptional: true };
    expect(names(dependents.select(entries(), opts, 'linux'))).to.eql(['start-server-and-test', 'jest-dev-server']);
    expect(names(dependents.select(entries(), opts, 'darwin'))).to.eql(['start-server-and-test']);
    expect(names(dependents.select(entries(), opts, 'win32'))).to.eql(['start-server-and-test']);
  });

  it('should run an --only entry even when it is optional', function () {
    expect(names(dependents.select(entries(), { only: 'jest-dev-server' }, 'linux'))).to.eql(['jest-dev-server']);
  });

  it('should refuse an --only entry on an OS it does not support', function () {
    expect(() => dependents.select(entries(), { only: 'jest-dev-server' }, 'darwin')).to.throw(
      /jest-dev-server.*linux.*darwin/
    );
  });

  it('should list the known names for an unknown --only', function () {
    expect(() => dependents.select(entries(), { only: 'nope' }, 'linux')).to.throw(
      /nope.*start-server-and-test, jest-dev-server/
    );
  });
});

describe('dependents: list', function () {
  it('should print each entry with its tag, OS, optional flag and indented commands', function () {
    const text = dependents.listText(dependents.parseManifest(manifestText()));
    ['start-server-and-test', 'v3.0.12', 'jest-dev-server', 'v11.0.0', 'optional'].forEach((s) =>
      expect(text).to.include(s)
    );
    const lines = text.split('\n');
    expect(lines).to.include('  node node_modules/mocha/bin/mocha.js test/helper src/*-spec.js');
    expect(lines).to.include('  npm run demo-multiple');
    const header = (name) => lines.find((l) => l.startsWith(name));
    expect(header('start-server-and-test')).to.match(/\ball\b/);
    expect(header('jest-dev-server')).to.match(/\blinux\b/);
  });
});

describe('dependents: args', function () {
  it('should parse every flag', function () {
    const argv = ['--only', 'jest-dev-server', '--include-optional', '--control', '--tgz', 'x.tgz', '--keep', '--list'];
    expect(dependents.parseArgs(argv)).to.eql({
      list: true,
      only: 'jest-dev-server',
      includeOptional: true,
      control: true,
      tgz: 'x.tgz',
      keep: true
    });
  });

  it('should default every flag off', function () {
    expect(dependents.parseArgs([])).to.eql({
      list: false,
      only: undefined,
      includeOptional: false,
      control: false,
      tgz: undefined,
      keep: false
    });
  });

  it('should reject an unknown argument with the usage line', function () {
    expect(() => dependents.parseArgs(['--frob'])).to.throw(/--frob[\s\S]*usage: npm run dependents/);
  });

  it('should reject a value flag without its value', function () {
    expect(() => dependents.parseArgs(['--tgz'])).to.throw(/--tgz needs a value/);
    expect(() => dependents.parseArgs(['--only'])).to.throw(/--only needs a value/);
  });
});
