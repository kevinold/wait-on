/**
 * Parity guard against @types/wait-on 5.3.4 (vendored at ./dt-index.d.ts).
 *
 * Every shape a @types/wait-on consumer relies on must stay assignable to the
 * bundled index.d.ts, so a project can drop the @types dev-dependency without
 * new type errors. A future narrowing of index.d.ts fails `npm run test:types`.
 *
 * Compiled by test/tsconfig.json. The exactOptionalPropertyTypes dimension is
 * covered separately by ./exact-optional.test-d.ts under test/tsconfig.exact.json.
 */
import ours = require('../../index');
import dt = require('./dt-index');

// 1. Every @types WaitOnOptions value is assignable to our WaitOnOptions.
declare const dtOpts: dt.WaitOnOptions;
const oursOpts: ours.WaitOnOptions = dtOpts;
void oursOpts;

// 2. The @types callback shape `(err: any) => void` is assignable to our cb
//    parameter. Reverting cb to a non-compatible signature breaks this.
const dtCb: (err: any) => void = () => {};
ours({ resources: ['tcp:3000'] }, dtCb);

// 3. Every @types exported type name resolves on our namespace and is
//    structurally assignable from its @types counterpart.
const _auth: ours.WaitOnAuth = {} as dt.WaitOnAuth;
const _validate: ours.ValidateStatus = {} as dt.ValidateStatus;
const _proxy: ours.AxiosProxyConfig = {} as dt.AxiosProxyConfig;
const _sig: ours.HttpSignature = {} as dt.HttpSignature;
void _auth;
void _validate;
void _proxy;
void _sig;

// 4. Headers. @types types headers as Record<string, any>, which the runtime
//    accepts. Non-string header values must type-check; reverting headers to
//    Record<string, string> breaks this line (the assignability checks above
//    would NOT catch it, since Record<string, any> is assignable to
//    Record<string, string>).
ours({ resources: ['http://localhost:3000'], headers: { 'x-count': 5, 'x-flag': true } });
