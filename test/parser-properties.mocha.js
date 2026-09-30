'use strict';

// Property/differential tests for wait-on's four pure parsers (rust-port R18):
// the resource-prefix regex, host:port, the ms/s/m/h interval parser, and the
// http://unix: split. They assert structural invariants over many generated
// inputs rather than a handful of examples, and are the Node oracle the Rust
// parsers (wait_on_core::parse, via the addon) must match in the differential
// block at the bottom of this file.
//
// No new dependency: property inputs come from a self-contained seeded PRNG
// (mulberry32). The seed is fixed (override with WAITON_TEST_SEED) and printed
// on failure so any counterexample reproduces. Assertions use node:assert and
// describe/it from mocha, so the file is portable across test runners.

const childProcess = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const mocha = require('mocha');
const describe = mocha.describe;
const it = mocha.it;
const before = mocha.before;

const { parseInterval } = require('../bin/wait-on');
const { resolveEngine, addonPath } = require('../lib/engine');

// --- seeded PRNG + property runner (dependency-free) ------------------------

const BASE_SEED = (() => {
  const n = Number.parseInt(process.env.WAITON_TEST_SEED, 10);
  return Number.isFinite(n) ? n >>> 0 : 0x1a2b3c4d;
})();
const RUNS = Number.parseInt(process.env.WAITON_TEST_RUNS, 10) || 300;

function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Run `prop` over `runs` generated inputs. On failure, rethrow with the seed,
// run index, and generated input so the exact case can be reproduced.
function forAll(gen, prop, opts) {
  const runs = (opts && opts.runs) || RUNS;
  const seed = (opts && opts.seed) || BASE_SEED;
  const rand = mulberry32(seed);
  for (let i = 0; i < runs; i++) {
    const input = gen(rand);
    try {
      prop(input);
    } catch (err) {
      const where =
        `property failed on input ${JSON.stringify(input)} ` +
        `(seed=${seed}, run=${i}/${runs}; set WAITON_TEST_SEED=${seed} to reproduce)\n`;
      err.message = where + err.message;
      err.stack = where + err.stack; // mocha prints the stack's header, not err.message
      throw err;
    }
  }
}

function randInt(rand, min, max) {
  return min + Math.floor(rand() * (max - min + 1));
}
function pick(rand, arr) {
  return arr[Math.floor(rand() * arr.length)];
}
function randStr(rand, alphabet, min, max) {
  const len = randInt(rand, min, max);
  let s = '';
  for (let i = 0; i < len; i++) s += pick(rand, alphabet);
  return s;
}

const LOWER = 'abcdefghijklmnopqrstuvwxyz'.split('');
const DIGITS = '0123456789'.split('');
const HOSTCH = 'abcdefghijklmnopqrstuvwxyz0123456789-.'.split('');
const HEXCOLON = 'abcdef0123456789:'.split('');
const PATHCH = 'abcdefghijklmnopqrstuvwxyz0123456789/._'.split('');
const ANYCH = 'abcHTTP0123:/.[]-_ '.split('');
const SCHEMES = ['https-get', 'http-get', 'https', 'http', 'tcp', 'socket', 'file', 'command'];
const UNITS = ['', 'ms', 's', 'm', 'h'];

// --- Node reference parsers -------------------------------------------------
// Regexes copied verbatim from the live code and cited by line. The golden
// vectors below lock these to the live behavior; if a live regex changes, the
// golden tests break and flag the drift.

const PREFIX_RE = /^((https?-get|https?|tcp|socket|file|command):)(.+)$/; // lib/wait-on.js:35
const HOST_PORT_RE = /^(?:\[([^\]]+)\]:|([^:]*):)?(\d+)$/; // lib/wait-on.js:37
const HTTP_UNIX_RE = /^http:\/\/unix:(.+?):((?:https?:\/\/|\/).*)$/; // lib/wait-on.js:42
const HTTP_UNIX_LEGACY_RE = /^http:\/\/unix:([^:]+):(.+)$/; // lib/wait-on.js:43

// prefix -> resource type, mirroring createResource$ (lib/wait-on.js:221-238);
// anything else (incl. an explicit file: prefix or no prefix) is a file.
const PREFIX_TYPE = {
  'https-get:': 'http',
  'http-get:': 'http',
  'https:': 'http',
  'http:': 'http',
  'tcp:': 'tcp',
  'command:': 'command',
  'socket:': 'socket'
};

const nodeParsers = {
  // extractPrefix / extractPath + type routing (lib/wait-on.js:283-297, 221-238)
  prefix(resource) {
    const m = PREFIX_RE.exec(resource);
    const prefix = m ? m[1] : '';
    const rest = m ? m[3] : resource;
    return { prefix, rest, type: PREFIX_TYPE[prefix] || 'file' };
  },
  // tcpExists host/port split (lib/wait-on.js:502-503); port kept as a string.
  hostPort(str) {
    const m = HOST_PORT_RE.exec(str);
    if (!m) return null;
    return { host: m[1] || m[2] || 'localhost', port: m[3] };
  },
  // the real ms/s/m/h interval parser (bin/wait-on parseInterval)
  interval: parseInterval,
  // createHTTP$ unix-socket split (lib/wait-on.js:405-409): normalize http-get:
  // then split socketPath (group 1) from the url/url-path (group 2).
  httpUnix(resource) {
    const rawUrl = resource.replace('-get:', ':');
    const m = HTTP_UNIX_RE.exec(rawUrl) || HTTP_UNIX_LEGACY_RE.exec(rawUrl);
    if (!m) return null;
    return { socketPath: m[1], requestPath: m[2] };
  }
};

// interval unit -> multiplier, using the exact arithmetic parseInterval uses so
// the expected value is float-identical to the parser's (bin/wait-on:175-181).
function expectedInterval(value, unit) {
  switch (unit) {
    case '':
    case 'ms':
      return Math.floor(value);
    case 's':
      return Math.floor(value * 1000);
    case 'm':
      return Math.floor(value * 1000 * 60);
    case 'h':
      return Math.floor(value * 1000 * 60 * 60);
    default:
      return undefined;
  }
}

// --- golden vectors (shared by the Node properties and the differential) ----

const PREFIX_GOLDEN = [
  ['tcp:my.server.com:3000', { prefix: 'tcp:', rest: 'my.server.com:3000', type: 'tcp' }],
  ['http://foo.com:8000/bar', { prefix: 'http:', rest: '//foo.com:8000/bar', type: 'http' }],
  ['https-get:https://my/bar', { prefix: 'https-get:', rest: 'https://my/bar', type: 'http' }],
  ['http-get:http://m.com:90/foo', { prefix: 'http-get:', rest: 'http://m.com:90/foo', type: 'http' }],
  ['socket:/path/sock', { prefix: 'socket:', rest: '/path/sock', type: 'socket' }],
  ['command:pg_isready', { prefix: 'command:', rest: 'pg_isready', type: 'command' }],
  ['file:/tmp/x', { prefix: 'file:', rest: '/tmp/x', type: 'file' }],
  ['/tmp/plain', { prefix: '', rest: '/tmp/plain', type: 'file' }],
  ['tcp:', { prefix: '', rest: 'tcp:', type: 'file' }] // no body after scheme -> not classified
];

const HOST_PORT_GOLDEN = [
  ['3000', { host: 'localhost', port: '3000' }],
  ['host.example:8080', { host: 'host.example', port: '8080' }],
  ['[::1]:8080', { host: '::1', port: '8080' }],
  ['[2001:db8::1]:443', { host: '2001:db8::1', port: '443' }],
  [':3000', { host: 'localhost', port: '3000' }]
];
const HOST_PORT_REJECTS = ['', 'abc', '3000x', 'host:', 'host:port', 'a:b:80', ':', '[::1]', '80:'];

const INTERVAL_GOLDEN = [
  ['250', 250],
  ['250ms', 250],
  ['2s', 2000],
  ['1m', 60000],
  ['1h', 3600000],
  ['2.5s', 2500],
  ['.5s', 500],
  ['0', 0]
];
const INTERVAL_UPPERCASE = ['2S', '2M', '2H', '5MS'];

const HTTP_UNIX_GOLDEN = [
  ['http://unix:/path/to/sock:/foo/bar', { socketPath: '/path/to/sock', requestPath: '/foo/bar' }],
  ['http-get://unix:/path/to/sock:/foo/bar', { socketPath: '/path/to/sock', requestPath: '/foo/bar' }],
  [
    'http://unix:/var/run/app.sock:http://localhost/health',
    { socketPath: '/var/run/app.sock', requestPath: 'http://localhost/health' }
  ],
  // socket path with an embedded colon (Windows named pipe drive letter)
  ['http://unix:\\\\?\\pipe\\C:\\app\\sock:/status', { socketPath: '\\\\?\\pipe\\C:\\app\\sock', requestPath: '/status' }],
  // legacy fallback: request path is neither / nor http(s):// prefixed
  ['http://unix:/sock:foo', { socketPath: '/sock', requestPath: 'foo' }]
];
const HTTP_UNIX_REJECTS = ['https://unix:/s:/p', 'http://foo.com/bar', 'tcp:host:3000', 'file:/x'];

// --- generators: each returns its parts plus the composed `input` -----------

function genScheme(rand) {
  const scheme = pick(rand, SCHEMES);
  const rest = randStr(rand, 'abcdefghijklmnopqrstuvwxyz0123456789/._-:'.split(''), 1, 20);
  return { scheme, rest, input: scheme + ':' + rest };
}

function genUnprefixed(rand) {
  return { input: pick(rand, ['/', './', '', 'ftp:', 'ws:', 'tcpx:', 'notreal:']) + randStr(rand, LOWER, 1, 15) };
}

function genAny(rand) {
  return { input: randStr(rand, ANYCH, 0, 24) };
}

function genHostPort(rand) {
  const host = randStr(rand, HOSTCH, 1, 12);
  const port = randStr(rand, DIGITS, 1, 5);
  return { host, port, input: host + ':' + port };
}

function genBarePort(rand) {
  const port = randStr(rand, DIGITS, 1, 5);
  return { port, input: port };
}

function genIpv6(rand) {
  const inner = randStr(rand, HEXCOLON, 1, 20);
  const port = randStr(rand, DIGITS, 1, 5);
  return { inner, port, input: '[' + inner + ']:' + port };
}

function genInterval(rand) {
  const intPart = String(randInt(rand, 0, 100000));
  const num = rand() < 0.5 ? intPart : intPart + '.' + String(randInt(rand, 0, 999));
  const unit = pick(rand, UNITS);
  return { num, unit, input: num + unit };
}

function genNonInterval(rand) {
  if (rand() < 0.5) return { input: randStr(rand, LOWER, 1, 8) };
  return { input: String(randInt(rand, 0, 9999)) + pick(rand, ['x', 'sec', 'q', 'min', 'hr', 'zz']) };
}

// A socket path whose only ':' followed by '/' is the delimiter before the
// request path: colons inside the socket path are always followed by a letter
// (drive) or backslash, so the non-greedy HTTP_UNIX_RE splits at the delimiter.
function genUnixCase(rand) {
  const req = '/' + randStr(rand, PATHCH, 0, 12) + pick(rand, ['', '/' + randStr(rand, LOWER, 1, 5)]);
  let sock;
  if (rand() < 0.5) {
    const segs = [];
    const count = randInt(rand, 1, 3);
    for (let i = 0; i < count; i++) segs.push(randStr(rand, LOWER, 1, 6));
    sock = '/' + segs.join('/') + pick(rand, ['', '.sock']);
  } else {
    const drive = pick(rand, ['C', 'D', 'E']);
    sock = '\\\\?\\pipe\\' + drive + ':\\' + randStr(rand, LOWER, 1, 6) + '\\sock';
  }
  return { sock, req, input: 'http://unix:' + sock + ':' + req };
}

// Tokens where the regexes branch, including all four JS line terminators.
const JUNK_TOKENS = [
  ...SCHEMES.map((s) => s + ':'),
  '://unix:',
  '-get:',
  ':',
  '/',
  '[',
  ']',
  '\\\\?\\pipe\\',
  '0',
  '42',
  '.',
  ...['ms', 's', 'm', 'h', 'MS', 'S', 'M', 'H'],
  '\n',
  '\r',
  '\u2028',
  '\u2029'
];

function genJunk(rand) {
  const count = randInt(rand, 0, 8);
  let input = '';
  for (let i = 0; i < count; i++) input += pick(rand, JUNK_TOKENS);
  return { input };
}

describe('parser properties (rust-port R18 differential oracle)', function () {
  describe('resource-prefix parser (PREFIX_RE)', function () {
    it('classifies documented resource examples', function () {
      for (const [resource, expected] of PREFIX_GOLDEN) {
        assert.deepEqual(nodeParsers.prefix(resource), expected);
      }
    });

    it('recomposes prefix + rest === input and maps each scheme to its type', function () {
      forAll(genScheme, ({ scheme, rest, input }) => {
        const parsed = nodeParsers.prefix(input);
        assert.equal(parsed.prefix, scheme + ':');
        assert.equal(parsed.rest, rest);
        assert.equal(parsed.prefix + parsed.rest, input);
        assert.equal(parsed.type, PREFIX_TYPE[scheme + ':'] || 'file');
      });
    });

    it('classifies an unrecognized or absent prefix as file', function () {
      forAll(genUnprefixed, ({ input }) => {
        const parsed = nodeParsers.prefix(input);
        assert.equal(parsed.type, 'file');
      });
    });

    it('classification is total (always one of the five resource types)', function () {
      const types = ['http', 'tcp', 'command', 'socket', 'file'];
      forAll(genAny, ({ input }) => {
        assert.ok(types.includes(nodeParsers.prefix(input).type));
      });
    });
  });

  describe('host:port parser (HOST_PORT_RE)', function () {
    it('parses documented host:port forms', function () {
      for (const [input, expected] of HOST_PORT_GOLDEN) {
        assert.deepEqual(nodeParsers.hostPort(input), expected);
      }
    });

    it('rejects malformed host:port', function () {
      for (const input of HOST_PORT_REJECTS) {
        assert.equal(nodeParsers.hostPort(input), null);
      }
    });

    it('round-trips host and port for simple hosts', function () {
      forAll(genHostPort, ({ host, port, input }) => {
        assert.deepEqual(nodeParsers.hostPort(input), { host, port });
      });
    });

    it('defaults host to localhost for a bare port', function () {
      forAll(genBarePort, ({ port, input }) => {
        assert.deepEqual(nodeParsers.hostPort(input), { host: 'localhost', port });
      });
    });

    it('extracts the inner address from bracketed IPv6', function () {
      forAll(genIpv6, ({ inner, port, input }) => {
        assert.deepEqual(nodeParsers.hostPort(input), { host: inner, port });
      });
    });
  });

  describe('ms/s/m/h interval parser (parseInterval)', function () {
    it('scales documented intervals', function () {
      for (const [input, expected] of INTERVAL_GOLDEN) {
        assert.equal(nodeParsers.interval(input), expected);
      }
    });

    it('scales value by unit and floors the result', function () {
      forAll(genInterval, ({ num, unit, input }) => {
        const value = parseFloat(num);
        assert.equal(nodeParsers.interval(input), expectedInterval(value, unit));
      });
    });

    it('returns unparseable input unchanged (pass-through)', function () {
      forAll(genNonInterval, ({ input }) => {
        assert.equal(nodeParsers.interval(input), input);
      });
    });

    it('characterization: uppercase units return undefined (case-sensitive switch, bin/wait-on:175-181)', function () {
      // The regex is case-insensitive but the switch is not, so an uppercase
      // unit matches the regex yet falls through to undefined. Documented here
      // as current Node behavior for the differential oracle, not endorsed.
      for (const arg of INTERVAL_UPPERCASE) {
        assert.equal(nodeParsers.interval(arg), undefined);
      }
    });
  });

  describe('http://unix: split (HTTP_UNIX_RE / legacy)', function () {
    it('splits documented http-unix forms', function () {
      for (const [input, expected] of HTTP_UNIX_GOLDEN) {
        assert.deepEqual(nodeParsers.httpUnix(input), expected);
      }
    });

    it('returns null for non http-unix resources', function () {
      for (const input of HTTP_UNIX_REJECTS) {
        assert.equal(nodeParsers.httpUnix(input), null);
      }
    });

    it('recovers socketPath and requestPath, including colon-bearing socket paths', function () {
      forAll(genUnixCase, ({ sock, req, input }) => {
        assert.deepEqual(nodeParsers.httpUnix(input), { socketPath: sock, requestPath: req });
      });
    });

    it('normalizes http-get:// the same as http://', function () {
      forAll(genUnixCase, ({ sock, req, input }) => {
        const getForm = input.replace(/^http:/, 'http-get:');
        assert.deepEqual(nodeParsers.httpUnix(getForm), { socketPath: sock, requestPath: req });
      });
    });
  });
});

// Hand-picked inputs where a naive port differs from the JS regex semantics (KTD4):
// [parser, input, the JS value]. T3 holds the addon to these literals.
const DIFFERENTIAL_EXTRAS = [
  ['interval', '1.2.3s', 1200],
  ['interval', '.s', NaN],
  ['interval', '5S', undefined],
  ['interval', '2ſ', '2ſ'], // non-ASCII never case-folds
  ['hostPort', '3000\n', null],
  ['hostPort', '[]:80', { host: '[]', port: '80' }],
  ['prefix', 'tcp:a\nb', { prefix: '', rest: 'tcp:a\nb', type: 'file' }],
  ['httpUnix', 'http-get://unix:/a-get:/b:/c', { socketPath: '/a-get', requestPath: '/b:/c' }],
  ['httpUnix', 'http://unix:/a\nb:/c', { socketPath: '/a\nb', requestPath: '/c' }]
];

// nodeParsers key -> addon export.
const ADDON_PARSERS = [
  ['prefix', 'parsePrefix'],
  ['hostPort', 'parseHostPort'],
  ['interval', 'parseInterval'],
  ['httpUnix', 'parseHttpUnix']
];

const GOLDEN_INPUTS = [
  ...[PREFIX_GOLDEN, HOST_PORT_GOLDEN, INTERVAL_GOLDEN, HTTP_UNIX_GOLDEN].flatMap((g) => g.map(([input]) => input)),
  ...HOST_PORT_REJECTS,
  ...INTERVAL_UPPERCASE,
  ...HTTP_UNIX_REJECTS
];

const GENERATORS = [
  genScheme,
  genUnprefixed,
  genAny,
  genHostPort,
  genBarePort,
  genIpv6,
  genInterval,
  genNonInterval,
  genUnixCase
];

describe('Rust parsers match the Node oracle (real addon)', function () {
  let addon;

  // Every input goes through all four parsers on both sides (cross-feed).
  function agree(input) {
    for (const [node, exported] of ADDON_PARSERS) {
      assert.deepStrictEqual(addon[exported](input), nodeParsers[node](input), `${exported}(${JSON.stringify(input)})`);
    }
  }

  // Skips without a prebuild unless WAIT_ON_ENGINE=rust-strict, where resolveEngine throws
  // on a missing or stale addon so ci:rs fails instead of skipping (KTD3).
  before(function () {
    const vars = { WAIT_ON_ENGINE: 'rust-strict', WAIT_ON_NATIVE_LIBRARY_PATH: process.env.WAIT_ON_NATIVE_LIBRARY_PATH };
    if (process.env.WAIT_ON_ENGINE !== 'rust-strict' && !fs.existsSync(addonPath(vars))) this.skip();
    addon = resolveEngine(vars).addon;
  });

  it('should export the four parsers and answer the hand-picked edge cases with the JS values', function () {
    for (const [, name] of ADDON_PARSERS) assert.equal(typeof addon[name], 'function', `addon.${name}`);
    const exportOf = Object.fromEntries(ADDON_PARSERS);
    for (const [parser, input, expected] of DIFFERENTIAL_EXTRAS) {
      const label = `${parser}(${JSON.stringify(input)})`;
      assert.deepStrictEqual(nodeParsers[parser](input), expected, `node ${label}`);
      assert.deepStrictEqual(addon[exportOf[parser]](input), expected, `addon ${label}`);
    }
    assert.deepStrictEqual(Object.keys(addon.parsePrefix('tcp:x')).sort(), ['prefix', 'rest', 'type']);
    assert.strictEqual(addon.parseInterval('5S'), undefined);
    assert.ok(Number.isNaN(addon.parseInterval('.s')));
  });

  it('should agree with the Node oracle on every golden and reject vector', function () {
    for (const input of GOLDEN_INPUTS) agree(input);
  });

  it('should agree with the Node oracle on every generated input', function () {
    for (const gen of GENERATORS) forAll(gen, ({ input }) => agree(input));
  });

  it('should agree with the Node oracle on junk including line terminators', function () {
    forAll(genJunk, ({ input }) => agree(input));
  });
});

describe('parser differential under rust-strict', function () {
  this.timeout(20000);

  // Runs only the differential block in a child mocha (never this test or the property suite).
  function runDifferential(engine) {
    const env = { ...process.env, WAIT_ON_ENGINE: engine, WAIT_ON_NATIVE_LIBRARY_PATH: path.join(__dirname, 'no-such-addon.node') };
    const args = [require.resolve('mocha/bin/mocha.js'), '--reporter', 'json', '--grep', 'Rust parsers match the Node oracle', __filename];
    const r = childProcess.spawnSync(process.execPath, args, { cwd: path.join(__dirname, '..'), env, encoding: 'utf8' });
    return { code: r.status, output: r.stdout + r.stderr, report: JSON.parse(r.stdout) };
  }

  it('should fail, not skip, under rust-strict when the addon cannot load', function () {
    const { code, output, report } = runDifferential('rust-strict');
    assert.notEqual(code, 0);
    assert.match(output, /failed to load the native addon/);
    assert.equal(report.stats.pending, 0);
  });

  it('should report the differential block pending under js when the addon is missing', function () {
    const { code, report } = runDifferential('js');
    assert.equal(code, 0);
    assert.equal(report.stats.passes, 0);
    assert.equal(report.stats.pending, 4);
  });
});
