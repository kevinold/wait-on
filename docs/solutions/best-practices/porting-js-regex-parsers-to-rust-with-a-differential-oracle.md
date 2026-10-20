---
title: Porting a JS regex parser to Rust means porting ECMAScript regex semantics, and the differential must fail loudly
date: 2026-09-30
category: best-practices
module: crates/wait-on-core
problem_type: best_practice
component: rust-engine
severity: medium
applies_when:
  - "Reimplementing a JS regex, parseFloat or String.replace based parser in Rust (by hand or with the regex crate)"
  - "Exposing a Rust function through napi so a JS test can compare it against the JS original"
  - "Adding a test that must run, not skip, under WAIT_ON_ENGINE=rust-strict"
related_components: [crates/wait-on-napi, test/parser-properties.mocha.js]
tags: [napi-rs, regex, differential-testing, property-testing, mocha, spike-next-rs]
---

# Porting a JS regex parser to Rust means porting ECMAScript regex semantics, and the differential must fail loudly

## Context

Lane L8 (#60) ported wait-on's four pure parsers to `crates/wait-on-core/src/parse.rs`: the resource prefix (`PREFIX_RE`, `lib/wait-on.js:36`), `host:port` (`HOST_PORT_RE`, `lib/wait-on.js:38`), the interval (`bin/wait-on:206`) and the `http://unix:` split (`HTTP_UNIX_RE`, `lib/wait-on.js:43`). It then checked them against the JS originals over the #245 seeded property vectors. The JS side is the oracle and does not change. Several differences are invisible in a normal port and show up only on inputs a generator rarely produces.

## Guidance

1. **Port the regex dialect, not the pattern.** A Rust `regex` crate pattern with the same text behaves differently, so the port was hand-written in std and each rule is spelled out:
   - `.` does not match `\n`, `\r`, U+2028 or U+2029, but negated classes such as `[^:]` and `[^\]]` do (`is_line_terminator`, `parse.rs:37`).
   - Without the `u` flag, `\d` is ASCII `0-9` only (`ascii_digits`, `parse.rs:45`).
   - Without the `m` flag, `$` is the true end of input. `'3000\n'` does not match.
   - Without the `u` flag, `/i` folds ASCII only, so `'2ſ'` never matches `s` (`eq_ignore_ascii_case`, `parse.rs:98`).
   - With a string pattern, `String.prototype.replace` replaces only the first occurrence (`replacen(.., 1)`, `parse.rs:127`). So `http-get://unix:/a-get:/b:/c` keeps `/a-get`.
   - `parseFloat` takes the longest decimal prefix: `'1.2.3'` gives 1.2, and `'.'` gives NaN (`parse_float`, `parse.rs:113`).
   - Keep JS's float multiply order. `value * 1000 * 60` and `value * 60 * 1000` differ for fractional values such as `66599.685m` (`parse.rs:105`).
2. **Make every JS outcome crossable through napi without normalizing on the JS side.** Use `Either3<f64, String, Undefined>` so the uppercase-unit case arrives as `undefined`, not `null`. Use `#[napi(js_name = "type")]` for a reserved key, and `Option<T>` for `null` (`crates/wait-on-napi/src/lib.rs:107` and `crates/wait-on-napi/src/lib.rs:144`). A JS adapter that "fixes up" shapes hides exactly the differences the test exists to catch.
3. **Hand-pick the vectors a naive port gets wrong, and assert their literal values on both sides.** Random generators rarely produce line terminators, non-ASCII case-folding characters or repeated `-get:`. Keep a `DIFFERENTIAL_EXTRAS` list and a junk generator that includes those tokens (`test/parser-properties.mocha.js`).
4. **A differential that is allowed to skip must fail under the strict engine.** Its `before` skips only when no prebuild exists and the engine is not `rust-strict`. Otherwise it goes through `resolveEngine`, which throws on a missing or stale addon (`test/parser-properties.mocha.js:467-471`). Pin that with a child mocha that runs only the differential block (`--grep`, line 506). Without the grep, a test that spawns its own file recurses.
5. **mocha 12 prints `err.stack`, not `err.message`.** The #245 `forAll` added the seed and the failing input to `err.message` only, so the reproduction line never appeared in the failure output. It now prefixes `err.stack` too (`test/parser-properties.mocha.js:60`).

## Why This Matters

Each mismatch passes every golden vector and most random runs, then fails on a real resource string containing a newline, a Windows pipe path or a non-ASCII character. The inversions made during L8 confirm the test catches these: letting the Rust prefix parser accept `\n`, or swapping the minute multiply order, turned the differential red and named the seed. That happens only when the test runs; a skip under `ci:rs` would have hidden all of it.

## When to Apply

- L7, when runtime parsing moves into Rust. The same rules apply to any further regex-backed code, such as `validateResource`.
- Any future Rust port of JS string handling in this repo.

## Examples

Reproduce a differential failure from its reported seed:

```sh
WAITON_TEST_SEED=439041101 npx mocha --exit test/parser-properties.mocha.js
```

Values the Rust side must return (asserted in `DIFFERENTIAL_EXTRAS`):

```js
['interval', '5S', undefined]          // regex /i matches, case-sensitive switch misses
['interval', '.s', NaN]
['hostPort', '[]:80', { host: '[]', port: '80' }]   // bracket form fails, falls to [^:]*:
['prefix', 'tcp:a\nb', { prefix: '', rest: 'tcp:a\nb', type: 'file' }]
```
