'use strict';

// Unit tests for the native replacements introduced when lodash/fp was dropped
// (#239). These are pure helpers used inside lib/wait-on.js; exposed via
// `_internal` solely so their behavior is pinned directly, not just exercised
// incidentally by the integration suite.

const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const chai = require('chai');
const expect = chai.expect;

const { _internal } = require('../lib/wait-on');
const { once, noop, isNotABoolean, isNotEmpty, determineRemainingResources } = _internal;

describe('native lodash replacements (#239)', function () {
  describe('noop', function () {
    it('returns undefined and ignores any arguments', function () {
      expect(noop()).to.equal(undefined);
      expect(noop(1, 2, 3)).to.equal(undefined);
    });
  });

  describe('once', function () {
    it('invokes the wrapped fn exactly once', function () {
      let calls = 0;
      const f = once(() => ++calls);
      f();
      f();
      f();
      expect(calls).to.equal(1);
    });

    it('returns the first result on every subsequent call', function () {
      let n = 0;
      const f = once(() => ++n); // would return 1,2,3… if called each time
      expect(f()).to.equal(1);
      expect(f()).to.equal(1);
      expect(f()).to.equal(1);
    });

    it('passes the first call arguments through to the wrapped fn', function () {
      let seen;
      const f = once((...args) => (seen = args));
      f('a', 'b');
      f('c'); // ignored
      expect(seen).to.deep.equal(['a', 'b']);
    });

    it('preserves `this` on the first call', function () {
      const obj = {
        val: 42,
        m: once(function () {
          return this.val;
        })
      };
      expect(obj.m()).to.equal(42);
    });

    it('caches a falsy/undefined result and still short-circuits', function () {
      let calls = 0;
      const f = once(() => {
        calls++;
        return undefined;
      });
      expect(f()).to.equal(undefined);
      expect(f()).to.equal(undefined);
      expect(calls).to.equal(1);
    });
  });

  describe('isNotABoolean', function () {
    it('is false for true and false', function () {
      expect(isNotABoolean(true)).to.equal(false);
      expect(isNotABoolean(false)).to.equal(false);
    });

    it('is true for every non-boolean value', function () {
      [undefined, null, 0, 1, '', 'x', {}, [], NaN, () => {}].forEach((v) => {
        expect(isNotABoolean(v)).to.equal(true);
      });
    });
  });

  describe('isNotEmpty', function () {
    it('is true for a non-empty array', function () {
      expect(isNotEmpty([1])).to.equal(true);
      expect(isNotEmpty(['a', 'b'])).to.equal(true);
    });

    it('is false for an empty array', function () {
      expect(isNotEmpty([])).to.equal(false);
    });
  });

  describe('determineRemainingResources (native zip replacement)', function () {
    it('returns resources whose state is falsy, preserving order', function () {
      const resources = ['a', 'b', 'c', 'd'];
      const states = [true, false, true, false];
      expect(determineRemainingResources(resources, states)).to.deep.equal(['b', 'd']);
    });

    it('returns an empty array when every resource is ready', function () {
      expect(determineRemainingResources(['a', 'b'], [true, true])).to.deep.equal([]);
    });

    it('returns all resources when none are ready (states undefined)', function () {
      expect(determineRemainingResources(['a', 'b'], [])).to.deep.equal(['a', 'b']);
    });
  });
});

// KTD4: the proxy decision JS hands the Rust addon, ported from undici's
// EnvHttpProxyAgent (env passed explicitly, so no process.env or Windows case rules).
describe('proxy decision helpers (L5)', function () {
  const { envProxyFor, proxyObjectUri } = _internal;
  const P = 'http://proxy:1';

  describe('envProxyFor', function () {
    const cells = [
      ['no proxy env is set', 'http://a.test/', {}, undefined],
      ['HTTP_PROXY is set for an http target', 'http://a.test/', { HTTP_PROXY: P }, P],
      ['http_proxy wins over HTTP_PROXY', 'http://a.test/', { http_proxy: P, HTTP_PROXY: 'http://other:2' }, P],
      ['an empty http_proxy shadows HTTP_PROXY', 'http://a.test/', { http_proxy: '', HTTP_PROXY: P }, undefined],
      ['an http target ignores HTTPS_PROXY', 'http://a.test/', { HTTPS_PROXY: P }, undefined],
      ['https_proxy wins over HTTPS_PROXY', 'https://a.test/', { https_proxy: P, HTTPS_PROXY: 'http://other:2' }, P],
      ['an https target reads HTTPS_PROXY', 'https://a.test/', { HTTPS_PROXY: P, HTTP_PROXY: 'http://other:2' }, P],
      ['an https target falls back to the http proxy', 'https://a.test/', { HTTP_PROXY: P }, P],
      ['an empty https_proxy falls back to the http proxy', 'https://a.test/', { https_proxy: '', HTTP_PROXY: P }, P],
      ['NO_PROXY is empty', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: '' }, P],
      ['NO_PROXY names the host', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test' }, undefined],
      ['no_proxy wins over NO_PROXY', 'http://a.test/', { HTTP_PROXY: P, no_proxy: 'b.test', NO_PROXY: 'a.test' }, P],
      ['NO_PROXY matches case-insensitively', 'http://A.Test/', { HTTP_PROXY: P, NO_PROXY: 'a.TEST' }, undefined],
      ['NO_PROXY splits on commas and whitespace', 'http://c.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test, b.test\tc.test' }, undefined],
      ['NO_PROXY is *', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: 'x.test, * ' }, undefined],
      ['NO_PROXY *:port matches that port', 'http://a.test:8080/', { HTTP_PROXY: P, NO_PROXY: '*:8080' }, undefined],
      ['NO_PROXY *:port skips another port', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: '*:8080' }, P],
      ['NO_PROXY *.x matches a subdomain', 'http://s.a.test/', { HTTP_PROXY: P, NO_PROXY: '*.a.test' }, undefined],
      ['NO_PROXY *.x skips the apex', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: '*.a.test' }, P],
      ['NO_PROXY .x matches the apex', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: '.a.test' }, undefined],
      ['NO_PROXY .x matches a subdomain', 'http://s.a.test/', { HTTP_PROXY: P, NO_PROXY: '.a.test' }, undefined],
      ['NO_PROXY x matches a subdomain', 'http://s.a.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test' }, undefined],
      ['NO_PROXY x does not match a suffix without a dot', 'http://ba.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test' }, P],
      ['NO_PROXY host:port matches the explicit port', 'http://a.test:8080/', { HTTP_PROXY: P, NO_PROXY: 'a.test:8080' }, undefined],
      ['NO_PROXY host:port skips another port', 'http://a.test:8081/', { HTTP_PROXY: P, NO_PROXY: 'a.test:8080' }, P],
      ['NO_PROXY host:80 matches the http default port', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test:80' }, undefined],
      ['NO_PROXY host:443 matches the https default port', 'https://a.test/', { HTTPS_PROXY: P, NO_PROXY: 'a.test:443' }, undefined],
      ['NO_PROXY host:80 skips an https target', 'https://a.test/', { HTTPS_PROXY: P, NO_PROXY: 'a.test:80' }, P],
      ['the target has a trailing dot', 'http://a.test./', { HTTP_PROXY: P, NO_PROXY: 'a.test' }, undefined],
      ['the NO_PROXY entry has a trailing dot', 'http://a.test/', { HTTP_PROXY: P, NO_PROXY: 'a.test.' }, undefined],
      ['NO_PROXY [::1]:port matches an IPv6 target', 'http://[::1]:8080/', { HTTP_PROXY: P, NO_PROXY: '[::1]:8080' }, undefined],
      ['NO_PROXY [::1]:port skips another port', 'http://[::1]:8081/', { HTTP_PROXY: P, NO_PROXY: '[::1]:8080' }, P],
      ['NO_PROXY bare ::1 matches an IPv6 target', 'http://[::1]:8080/', { HTTP_PROXY: P, NO_PROXY: '::1' }, undefined],
      ['NO_PROXY bracketed [::1] matches an IPv6 target', 'http://[::1]/', { HTTP_PROXY: P, NO_PROXY: '[::1]' }, undefined]
    ];
    for (const [label, url, env, expected] of cells) {
      it(`should return ${expected === undefined ? 'no proxy' : 'the proxy'} when ${label}`, function () {
        expect(envProxyFor(url, env)).to.equal(expected);
      });
    }
  });

  describe('proxyObjectUri', function () {
    const cells = [
      ['a bare IPv6 host', { host: '::1', port: 1 }, 'http://[::1]:1'],
      ['an already-bracketed IPv6 host', { host: '[::1]', port: 1 }, 'http://[::1]:1'],
      ['an expanded IPv6 host', { host: '2001:db8::1', port: 1 }, 'http://[2001:db8::1]:1'],
      ["protocol 'http:'", { host: '127.0.0.1', port: 1, protocol: 'http:' }, 'http://127.0.0.1:1'],
      ["protocol 'https:'", { host: '127.0.0.1', port: 1, protocol: 'https:' }, 'https://127.0.0.1:1'],
      ["protocol 'http'", { host: '127.0.0.1', port: 1, protocol: 'http' }, 'http://127.0.0.1:1'],
      ['credentials with reserved characters', { host: '::1', port: 1, auth: { username: 'u', password: 'p@:/' } }, 'http://u:p%40%3A%2F@[::1]:1'],
      ['a username without a password', { host: 'h', port: 1, auth: { username: 'u' } }, 'http://u:@h:1']
    ];
    for (const [label, proxy, expected] of cells) {
      it(`should build ${expected} for ${label}`, function () {
        expect(proxyObjectUri(proxy)).to.equal(expected);
      });
    }
  });
});
