'use strict';

// The Rust shim (KTD5/KTD6): one addon.wait call runs the whole wait. Loads neither rxjs
// nor undici; validation, http option building and validateStatus stay in JS (R-L7-7/8).

const crypto = require('crypto');
const tls = require('tls');
const { extractPath, extractPrefix, envProxyFor, httpRequest, proxyObjectUri, routesHttpToRust, tcpHostPort } = require('./resources');

const KINDS = { 'https-get:': 'http', 'http-get:': 'http', 'https:': 'http', 'http:': 'http', 'tcp:': 'tcp', 'socket:': 'socket', 'command:': 'command' };
const U32_MAX = 2 ** 32 - 1;
// Node resets a timer delay above 2^31-1 ms to 1 ms; the Rust loop mirrors that (parity)
const timerMs = (ms) => (ms > 2 ** 31 - 1 ? 1 : ms);

// KTD6: the Rust loop runs the wait only when every http resource is Rust-routable.
function routable(validatedOpts, env) {
  return validatedOpts.resources
    .filter((resource) => KINDS[extractPrefix(resource)] === 'http')
    .every((resource) => {
      const { url, socketPath } = httpRequest(resource, validatedOpts);
      return routesHttpToRust({ addon: true, validatedOpts, socketPath, env, url });
    });
}

function buildSpec(validatedOpts, env) {
  const { delay, interval, window, tcpTimeout, commandTimeout, simultaneous, timeout, reverse, verbose } = validatedOpts;
  const spec = {
    delayMs: timerMs(delay),
    intervalMs: timerMs(interval),
    windowMs: Math.min(window, U32_MAX), // compared against wall time, not a timer
    tcpTimeoutMs: timerMs(tcpTimeout),
    commandTimeoutMs: timerMs(commandTimeout),
    reverse,
    verbose,
    resources: validatedOpts.resources.map((resource) => resourceSpec(resource, validatedOpts, env))
  };
  if (simultaneous !== Infinity) spec.simultaneous = Math.min(simultaneous, U32_MAX);
  if (timeout !== Infinity) spec.timeoutMs = timerMs(timeout);
  return spec;
}

function resourceSpec(name, validatedOpts, env) {
  const kind = KINDS[extractPrefix(name)] || 'file';
  const path = extractPath(name);
  switch (kind) {
    case 'http':
      return { name, kind, http: httpOptions(name, validatedOpts, env) };
    case 'tcp': {
      const { host, port } = tcpHostPort(path);
      return { name, kind, path, host, port: Number(port) };
    }
    case 'command':
      return { name, kind, command: path };
    default:
      return { name, kind, path };
  }
}

function httpOptions(resource, validatedOpts, env) {
  const { followRedirect, httpTimeout, proxy } = validatedOpts;
  const { method, url, socketPath, requestHeaders } = httpRequest(resource, validatedOpts);
  const options = {
    url,
    method,
    // the addon takes a string map; joi admits non-string header values
    headers: Object.fromEntries(Object.entries(requestHeaders).map(([k, v]) => [k, String(v)])),
    followRedirect
  };
  // the addon's timeoutMs is a u32; unset/0 means no per-request timeout
  if (httpTimeout) options.timeoutMs = Math.min(httpTimeout, U32_MAX);
  // KTD4/KTD5: a unix socket is never proxied; proxy: true behaves like unset
  let rustProxy;
  if (socketPath === undefined && proxy !== false) {
    rustProxy = proxy && typeof proxy === 'object' ? proxyObjectUri(proxy) : envProxyFor(url, env);
  }
  const transport = { ...rustTlsOptions(validatedOpts), proxy: rustProxy, socketPath };
  // optional fields only when defined (KTD7)
  for (const [name, value] of Object.entries(transport)) if (value !== undefined) options[name] = value;
  return options;
}

// KTD1/KTD2: TLS material for the addon, for every target scheme (redirects may reach
// https). Roots only with strictSSL (a ca replaces the defaults, as in Node); the key is
// re-exported as plain PKCS#8, or passed as given when it will not parse (KTD3).
function rustTlsOptions({ ca, cert, key, passphrase, strictSSL }) {
  const roots = strictSSL ? (ca === undefined ? tls.getCACertificates('default') : [String(ca)]) : undefined;
  return { roots, cert: cert === undefined ? undefined : String(cert), key: key === undefined ? undefined : pkcs8Pem(key, passphrase) };
}

function pkcs8Pem(key, passphrase) {
  try {
    return crypto.createPrivateKey({ key, passphrase }).export({ type: 'pkcs8', format: 'pem' });
  } catch {
    return String(key);
  }
}

function run({ validatedOpts, addon, log, cleanup }) {
  const { validateStatus } = validatedOpts;
  // KTD3: JS truthiness and a throw-as-false, since Rust accepts only a real boolean.
  const status =
    validateStatus &&
    ((s) => {
      try {
        return Boolean(validateStatus(s));
      } catch {
        return false;
      }
    });
  let result;
  try {
    // log is passed only when log/verbose is on (validatedOpts.log is set by verbose)
    result = addon.wait(buildSpec(validatedOpts, process.env), validatedOpts.log ? log : undefined, status);
  } catch (err) {
    return cleanup(err); // a malformed spec throws synchronously; errors reach the callback
  }
  result.then(({ ok, error }) => (ok ? cleanup() : cleanup(new Error(error))), cleanup);
}

module.exports = { routable, buildSpec, run };
