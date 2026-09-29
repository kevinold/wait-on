---
title: index.d.ts Parity with @types/wait-on 5.3.4 - Plan
type: fix
date: 2026-09-29
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# index.d.ts Parity with @types/wait-on 5.3.4 - Plan

## Goal Capsule

- **Objective:** A project that depended on `@types/wait-on@5.3.4` can delete that dev-dependency and rely solely on wait-on's bundled `index.d.ts` — under any tsconfig `@types` compiled cleanly, including `exactOptionalPropertyTypes: true` — with no new type errors.
- **Means:** Widen the bundled declarations to be a superset of `@types/wait-on@5.3.4` (optional props gain `| undefined`, `headers` widens to `Record<string, any>`) and lock parity with a vendored copy of the DT declarations plus two `tsc` passes (KTD1, KTD2, KTD3).
- **Authority:** Settled decisions in this plan are user-directed; do not relitigate. On any conflict, this plan's KTDs win over inherited habit.
- **Stop conditions:** A runtime change becomes necessary (types-only is a hard constraint); a new dependency would be required; parity cannot be expressed without breaking an existing valid usage in `test/types.test-d.ts` or `test/types-compat/dt-wait-on-tests.ts`.
- **Execution profile:** Small, mechanical, types + tests only. No change under `lib/` or `bin/`.
- **Finishes/ships:** `ce-work` implements; the lfg pipeline ships to an open, green fork PR based on `fix/types-callback-compat`.

---

## Product Contract

### Summary

Bring wait-on's bundled `index.d.ts` to full structural parity with `@types/wait-on@5.3.4` so downstream consumers migrating off the DefinitelyTyped package are not broken, and guard that parity with automated type tests that fail on any future narrowing.

### Problem Frame

The prior PR (`fix/types-callback-compat`, fork #46 / upstream #251) restored callback and alias compatibility but left two narrowings versus `@types/wait-on@5.3.4`: optional properties are declared `?: T` rather than `?: T | undefined`, and `headers` is `Record<string, string>` rather than `Record<string, any>`. Under `exactOptionalPropertyTypes: true`, a consumer passing an explicit `undefined` (e.g. `timeout: cfg.timeout`) now errors; a consumer setting a non-string header value now errors. Both compile against `@types/wait-on` today, so migrating to the bundled types is a regression until parity is closed.

### Requirements

Type surface (index.d.ts):
- R1. Every optional property in `WaitOnOptions` is declared `?: T | undefined` (delay, httpTimeout, interval, log, reverse, simultaneous, timeout, validateStatus, verbose, window, tcpTimeout, commandTimeout, proxy, auth, strictSSL, followRedirect, headers).
- R2. Every optional property in `WaitOnProxyOptions` is declared `?: T | undefined` (protocol, auth), and the nested `auth` object stays as-is except the outer `auth` property gains `| undefined`.
- R3. `headers` is typed `Record<string, any> | undefined`, matching `@types/wait-on@5.3.4`.
- R4. All five `@types/wait-on@5.3.4` exported names resolve on the wait-on namespace and are structurally assignable from their DT counterparts: `WaitOnOptions`, `WaitOnAuth`, `ValidateStatus`, `AxiosProxyConfig`, `HttpSignature`.
- R5. No runtime file changes: `lib/**` and `bin/**` are untouched.

Automated parity guard (test only):
- R6. `@types/wait-on@5.3.4`'s `index.d.ts` is vendored at `test/types-compat/dt-index.d.ts` with an MIT header naming the DefinitelyTyped source URL.
- R7. A type test asserts: every `@types` `WaitOnOptions` value is assignable to our `WaitOnOptions`; the `@types` callback shape `(err: any) => void` is assignable to our callback parameter; each `@types` exported type name resolves on our namespace (R4). It is compiled by the existing `test/tsconfig.json`.
- R8. A second `tsc` pass (`test/tsconfig.exact.json`, extending `test/tsconfig.json` with `exactOptionalPropertyTypes: true`) compiles a test that passes explicit `undefined` for every optional option and errors if any optional loses its `| undefined`.
- R9. `npm run test:types` runs both `tsc` passes; `npm test` (lint + test:types + mocha) is green.

### Scope Boundaries

- Deferred/out of scope: any runtime behavior change; adding dependencies; touching the upstream `jeffbski/wait-on` repo; redoing the callback/alias/TLS fixes already shipped on the base branch.
- In scope: `index.d.ts`, files under `test/types-compat/`, `test/tsconfig*.json`, and the `test:types` script in `package.json`.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **`headers?: Record<string, any> | undefined`.** (session-settled: user-directed — chosen over `Record<string, string | number | boolean>`: the runtime validates headers with a bare `Joi.object()` — no value schema, no coercion — then passes them straight to `axios()` (`lib/wait-on.js:60`, `lib/wait-on.js:356`), which accepts `string | number | boolean | string[] | null`; nothing narrows to string, `Record<string, any>` is exactly what `@types/wait-on` uses, and a narrower union would reject valid runtime values like multi-value `string[]` headers.) Governs R3.
- KTD2. **Add `| undefined` to every optional property.** (session-settled: user-directed — chosen over leaving `?: T`: `@types/wait-on` declares all optionals `?: T | undefined`, and without it `exactOptionalPropertyTypes: true` consumers passing explicit `undefined` get errors — the exact incompatibility this plan closes.) Governs R1, R2.
- KTD3. **Guard parity with a vendored DT declaration + assignability test + a second `exactOptionalPropertyTypes` tsc pass.** (session-settled: user-directed — chosen over a one-time manual diff: a future narrowing must fail `npm run test:types`, not pass silently.) Governs R6, R7, R8, R9.
- KTD4. **Keep the wait-on additions (`WaitOnInput`, `WaitOnProxyOptions`, `proxy: false`, `commandTimeout`).** Parity means our surface is a superset of `@types`; extra optional members do not break DT-shaped consumers. No change needed to these; they are not narrowings.

### Design notes

The assignability test is one-directional and deliberate: it asserts `@types → ours` (a DT-typed value is accepted by ours), which is the migration direction. It does not assert `ours → @types`. Note the assignability check alone does not catch the `headers` narrowing (`Record<string, any>` is assignable to `Record<string, string>` because `any` is bidirectional), so R7 adds an explicit assertion that a non-string-valued header object is accepted — that is what actually pins KTD1.

The vendored `dt-index.d.ts` is a module (`export = waitOn`), imported by the parity test via `import dt = require('./dt-index')`; it does not collide with the bundled `index.d.ts` (each is its own module). `skipLibCheck: true` in the base tsconfig means the vendored file is not deep-checked, only resolved.

### Assumptions

- The base branch `fix/types-callback-compat` already compiles green (`test/types.test-d.ts`, `test/types-compat/dt-wait-on-tests.ts` in `test/tsconfig.json`). This plan only widens types and adds tests, so it cannot regress those.
- `import { WaitOnOptions } from '../../index'` (named import from an `export =` module) already compiles on the base branch, so no `esModuleInterop` change is needed.

### Sequencing

U1 (widen index.d.ts) → U2 (vendor DT decl + parity test) → U3 (exact-optional pass). U3's test only compiles green after U1; U2 and U3 both depend on U1.

---

## Implementation Units

### U1. Widen index.d.ts optionals and headers

- **Goal:** R1, R2, R3, R5 — make the bundled declarations a superset of `@types/wait-on@5.3.4`.
- **Files:** `index.d.ts`.
- **Approach:** Append `| undefined` to each optional property in `WaitOnOptions` and `WaitOnProxyOptions` (outer `auth` and `protocol`). Change `headers?: Record<string, string>` to `headers?: Record<string, any> | undefined`. Leave required `resources`, the nested proxy `auth` object shape, type aliases, and all JSDoc intact. Do not touch `lib/**` or `bin/**`.
- **Test Scenarios:** existing `test/types.test-d.ts` and `test/types-compat/dt-wait-on-tests.ts` still compile; U2/U3 tests compile.
- **Verification:** `npm run test:types`.

### U2. Vendor DT declarations and add the assignability parity test

- **Goal:** R4, R6, R7.
- **Files:** `test/types-compat/dt-index.d.ts` (new), `test/types-compat/parity.test-d.ts` (new), `test/tsconfig.json` (add both — the `.test-d.ts` to `include`; the `.d.ts` is picked up via import).
- **Approach:** Copy `@types/wait-on@5.3.4`'s `index.d.ts` verbatim into `dt-index.d.ts` with a top MIT header citing `https://github.com/DefinitelyTyped/DefinitelyTyped/blob/master/types/wait-on/index.d.ts`. In `parity.test-d.ts`: `import ours = require('../../index')` and `import dt = require('./dt-index')`; assign `dt.WaitOnOptions` → `ours.WaitOnOptions`; assign a `(err: any) => void` callback into a real `ours(...)` call; for each DT exported name assign `dt.X` → `ours.X` (proves both name resolution and structural assignability); add an explicit assertion that a non-string-valued header object (e.g. `{ 'x-n': 5 }`) is accepted by `ours` `headers` (pins KTD1).
- **Test Scenarios:** file compiles under `test/tsconfig.json`; reverting `headers` to `Record<string, string>` makes the numeric-header assertion fail.
- **Verification:** `npm run test:types`.

### U3. Add the exactOptionalPropertyTypes second tsc pass

- **Goal:** R8, R9.
- **Files:** `test/tsconfig.exact.json` (new), `test/types-compat/exact-optional.test-d.ts` (new), `package.json` (`test:types` script).
- **Approach:** `tsconfig.exact.json` extends `./tsconfig.json`, sets `compilerOptions.exactOptionalPropertyTypes: true`, and `include`s only `types-compat/exact-optional.test-d.ts` and `../index.d.ts`. The test passes explicit `undefined` for every optional `WaitOnOptions` property and constructs a `WaitOnProxyOptions` with `protocol: undefined` and `auth: undefined`. Update `test:types` to `tsc -p test/tsconfig.json && tsc -p test/tsconfig.exact.json`.
- **Test Scenarios:** both passes green after U1; dropping `| undefined` from any optional makes the exact pass fail on that property.
- **Verification:** `npm run test:types`, then `npm test`.

---

## Verification Contract

| Gate | Command | Applies to |
|---|---|---|
| Lint | `npm run lint` | all |
| Types (both passes) | `npm run test:types` | U1, U2, U3 |
| Unit tests | `npm run test:mocha` | regression guard (no runtime change expected) |
| Full check | `npm test` | Definition of Done |

Parity-regression proof: temporarily reverting `headers` to `Record<string, string>` must fail `test:types` (U2 numeric-header assertion); dropping any `| undefined` must fail the exact pass (U3).

---

## Definition of Done

- Global: `npm test` green locally (lint + both tsc passes + mocha); no changes under `lib/**` or `bin/**`; no new dependency in `package.json`; commit uses `fix(types):`; this plan committed under `docs/plans/`; fork PR opened against base `fix/types-callback-compat` on the kevinold fork with fork CI green.
- U1: R1, R2, R3, R5 met; existing type tests still compile.
- U2: R4, R6, R7 met; vendored file carries the MIT/source header.
- U3: R8, R9 met; `test:types` runs both passes.
- Cleanup: no scratch/experimental type files left in the diff.
