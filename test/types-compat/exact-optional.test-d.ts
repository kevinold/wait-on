/**
 * exactOptionalPropertyTypes guard (compiled by test/tsconfig.exact.json).
 *
 * A @types/wait-on 5.3.4 consumer with `exactOptionalPropertyTypes: true` can
 * pass an explicit `undefined` for any optional option — e.g. `timeout: cfg.timeout`
 * where `cfg.timeout` is `number | undefined`. That requires every optional to be
 * declared `?: T | undefined`. If any optional in index.d.ts loses its
 * `| undefined`, assigning `undefined` to it errors under this stricter pass and
 * `npm run test:types` fails.
 */
import waitOn = require('../../index');

// Every optional WaitOnOptions property accepts an explicit `undefined`.
waitOn({
  resources: ['tcp:3000'],
  delay: undefined,
  httpTimeout: undefined,
  interval: undefined,
  log: undefined,
  reverse: undefined,
  simultaneous: undefined,
  timeout: undefined,
  validateStatus: undefined,
  verbose: undefined,
  window: undefined,
  tcpTimeout: undefined,
  commandTimeout: undefined,
  proxy: undefined,
  auth: undefined,
  strictSSL: undefined,
  followRedirect: undefined,
  headers: undefined,
});

// WaitOnProxyOptions optional properties (including the nested auth) also
// accept an explicit `undefined`.
const proxy: waitOn.WaitOnProxyOptions = {
  host: 'localhost',
  port: 8080,
  protocol: undefined,
  auth: undefined,
};
void proxy;
