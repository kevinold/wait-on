'use strict';

// The JS engine: the rxjs polling pipeline, required lazily by lib/wait-on.js (KTD5).

const crypto = require('crypto');
const fs = require('fs');
const { promisify } = require('util');
const net = require('net');
const tls = require('tls');
const util = require('util');
// Import fetch from undici (not the global): Node's built-in fetch is backed by
// Node's *bundled* undici, whose Dispatcher interface differs across Node majors
// (Node 22 ships undici 6, Node 26 ships 8), so a dispatcher from this dependency
// would be rejected by the global fetch. Using undici's own fetch keeps fetch and
// the dispatcher on the same version across every supported Node.
const { fetch, Agent, ProxyAgent, EnvHttpProxyAgent } = require('undici');
const { NEVER, combineLatest, from, merge, throwError, timer } = require('rxjs');
const { distinctUntilChanged, exhaustMap, finalize, map, mergeMap, scan, startWith, take, takeWhile } = require('rxjs/operators');
const childProcess = require('child_process');
const {
  isNotABoolean,
  isNotEmpty,
  HTTP_GET_RE,
  HTTP_UNIX_RE,
  HTTP_UNIX_LEGACY_RE,
  TIMEOUT_ERR_MSG,
  determineRemainingResources,
  extractPath,
  extractPrefix,
  proxyObjectUri,
  envProxyFor,
  routesHttpToRust,
  tcpHostPort
} = require('./resources');
const fstat = promisify(fs.stat);
const exec = promisify(childProcess.exec);

function run({ validatedOpts, addon, log, output, cleanup }) {
  const { resources, timeout } = validatedOpts;
  const logWaitingForWDeps = (resourceStates) => logWaitingFor({ log, resources }, resourceStates);
  const createResourceWithDeps$ = (resource) => createResource$({ validatedOpts, output, log, addon }, resource);

  let lastResourcesState = resources; // the last state we had recorded

  const timeoutError$ =
    timeout !== Infinity
      ? timer(timeout).pipe(
          mergeMap(() => {
            const resourcesWaitingFor = determineRemainingResources(resources, lastResourcesState).join(', ');
            return throwError(Error(`${TIMEOUT_ERR_MSG}: ${resourcesWaitingFor}`));
          })
        )
      : NEVER;

  logWaitingForWDeps(resources);

  const resourcesCompleted$ = combineLatest(resources.map(createResourceWithDeps$));

  merge(timeoutError$, resourcesCompleted$)
    .pipe(takeWhile((resourceStates) => resourceStates.some((x) => !x)))
    .subscribe({
      next: (resourceStates) => {
        lastResourcesState = resourceStates;
        logWaitingForWDeps(resourceStates);
      },
      error: cleanup,
      complete: cleanup
    });
}

function logWaitingFor({ log, resources }, resourceStates) {
  const remainingResources = determineRemainingResources(resources, resourceStates);
  if (isNotEmpty(remainingResources)) {
    log(`waiting for ${remainingResources.length} resources: ${remainingResources.join(', ')}`);
  }
}

function createResource$(deps, resource) {
  const prefix = extractPrefix(resource);
  switch (prefix) {
    case 'https-get:':
    case 'http-get:':
    case 'https:':
    case 'http:':
      return createHTTP$(deps, resource);
    case 'tcp:':
      return createTCP$(deps, resource);
    case 'command:':
      return createCommand$(deps, resource);
    case 'socket:':
      return createSocket$(deps, resource);
    default:
      return createFileResource$(deps, resource);
  }
}

function createFileResource$(
  { validatedOpts: { delay, interval, reverse, simultaneous, window: stabilityWindow }, output, addon },
  resource
) {
  const filePath = extractPath(resource);
  const checkOperator = reverse
    ? map((size) => size === -1) // check that file does not exist
    : scan(
        // check that file exists and the size is stable
        (acc, x) => {
          if (x > -1) {
            const { size, t } = acc;
            const now = Date.now();
            if (size !== -1 && x === size) {
              if (now >= t + stabilityWindow) {
                // file size has stabilized
                output(`  file stabilized at size:${size} file:${filePath}`);
                return true;
              }
              output(`  file exists, checking for size change during stability window, size:${size} file:${filePath}`);
              return acc; // return acc unchanged, just waiting to pass stability window
            }
            output(`  file exists, checking for size changes, size:${x} file:${filePath}`);
            return { size: x, t: now }; // update acc with new value and timestamp
          }
          return acc;
        },
        { size: -1, t: Date.now() }
      );

  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`checking file stat for file:${filePath} ...`);
      return from(addon ? addon.fileSize(filePath) : getFileSize(filePath));
    }, simultaneous),
    checkOperator,
    map((x) => (isNotABoolean(x) ? false : x)),
    startWith(false),
    distinctUntilChanged(),
    take(2)
  );
}

async function getFileSize(filePath) {
  try {
    const { size } = await fstat(filePath);
    return size;
  } catch {
    return -1;
  }
}

// Build the undici dispatcher for a single request. TLS client options and the
// unix socketPath ride on the connect options; proxy selection wraps them.
function buildDispatcher({ ca, cert, key, passphrase, rejectUnauthorized, socketPath, proxy }) {
  const connect = { rejectUnauthorized };
  if (ca !== undefined) connect.ca = ca;
  if (cert !== undefined) connect.cert = cert;
  if (key !== undefined) connect.key = key;
  if (passphrase !== undefined) connect.passphrase = passphrase;
  // A socketPath connection always uses a plain Agent, never a proxy dispatcher:
  // axios never proxies socketPath, and undici's proxy dispatchers would route the
  // synthesized http://localhost request through HTTP_PROXY and fail.
  if (socketPath !== undefined) {
    return new Agent({ connect: { ...connect, socketPath } });
  }
  if (proxy && typeof proxy === 'object') {
    return new ProxyAgent({ uri: proxyObjectUri(proxy), requestTls: connect });
  }
  if (proxy === false) {
    return new Agent({ connect });
  }
  // proxy unset: honor HTTP(S)_PROXY / NO_PROXY environment variables like axios.
  return new EnvHttpProxyAgent({ connect });
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

function createHTTP$({ validatedOpts, output, addon }, resource) {
  const {
    auth,
    delay,
    followRedirect,
    headers,
    httpTimeout,
    interval,
    proxy,
    reverse,
    simultaneous,
    strictSSL: rejectUnauthorized,
    validateStatus
  } = validatedOpts;
  const method = HTTP_GET_RE.test(resource) ? 'GET' : 'HEAD';
  const rawUrl = resource.replace('-get:', ':');
  // the socket path may contain colons (Windows named pipes); HTTP_UNIX_RE splits at
  // the colon before the url/url-path, with HTTP_UNIX_LEGACY_RE as the simple fallback.
  const matchHttpUnixSocket = HTTP_UNIX_RE.exec(rawUrl) || HTTP_UNIX_LEGACY_RE.exec(rawUrl); // http://unix:/sock:/url
  const socketPath = matchHttpUnixSocket ? matchHttpUnixSocket[1] : undefined;
  // For unix sockets HTTP_UNIX_RE yields either a relative ('/foo') or absolute
  // ('http://localhost/foo') path; new URL(..., base) normalizes both to a URL
  // fetch accepts, while socketPath carries the actual connection.
  const url = socketPath ? new URL(matchHttpUnixSocket[2], 'http://localhost').href : rawUrl;
  const socketPathDesc = socketPath ? `socketPath:${socketPath}` : '';

  const requestHeaders = { ...headers };
  if (auth) {
    // axios parity: opts.auth overrides any Authorization already in headers (regardless
    // of case), and builds a Basic header even for partial creds (each side defaults to
    // '' -> `Basic base64(user:pass)`). Undici would otherwise comma-merge a caller's
    // capital-case `Authorization` with the lowercase one set here.
    for (const name of Object.keys(requestHeaders)) {
      if (name.toLowerCase() === 'authorization') delete requestHeaders[name];
    }
    const token = Buffer.from(`${auth.username ?? ''}:${auth.password ?? ''}`).toString('base64');
    requestHeaders.authorization = `Basic ${token}`;
  }

  const env = process.env;
  if (routesHttpToRust({ addon, validatedOpts, socketPath, env, url })) {
    // KTD4/KTD5: a unix socket is never proxied; proxy: true behaves like unset
    let rustProxy;
    if (socketPath === undefined && proxy !== false) {
      rustProxy = proxy && typeof proxy === 'object' ? proxyObjectUri(proxy) : envProxyFor(url, env);
    }
    const transport = { ...rustTlsOptions(validatedOpts), proxy: rustProxy, socketPath };
    return createRustHTTP$({ validatedOpts, output, addon }, { method, url, requestHeaders, transport, socketPathDesc });
  }

  const { ca, cert, key, passphrase } = validatedOpts;
  let dispatcher;
  try {
    dispatcher = buildDispatcher({
      ca,
      cert,
      key,
      passphrase,
      rejectUnauthorized,
      socketPath,
      proxy
    });
  } catch (err) {
    // Dispatcher/proxy construction runs synchronously here (before subscribe); route
    // any error through the stream so it reaches the callback/promise instead of
    // throwing synchronously out of waitOn() (contract: all errors via cb).
    return throwError(() => err);
  }
  // Aborted when polling for this resource stops so no in-flight request lingers to
  // hold the checked socket open past the caller's teardown (parity with axios's
  // non-keep-alive http adapter, which released each socket right after the response).
  const teardown = new AbortController();

  const fetchOptions = {
    method,
    headers: requestHeaders,
    // followRedirect true (default) follows 3xx to a final 2xx; false leaves the
    // 3xx as the response, which then fails the default 2xx status check.
    redirect: followRedirect ? 'follow' : 'manual',
    dispatcher
  };

  const checkFn = reverse ? negateAsync(httpCallSucceeds) : httpCallSucceeds;
  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`making HTTP(S) ${method} request to ${socketPathDesc} url:${url} ...`);
      return from(checkFn(output, url, fetchOptions, validateStatus, httpTimeout, teardown.signal));
    }, simultaneous),
    startWith(false),
    distinctUntilChanged(),
    take(2),
    finalize(() => {
      teardown.abort();
      dispatcher.close().catch(() => dispatcher.destroy().catch(() => {}));
    })
  );
}

// The in-scope http check answered by the addon's HttpChecker (one per resource, so the
// client and its connection pool live across polls); rxjs still drives the polling.
function createRustHTTP$({ validatedOpts, output, addon }, { method, url, requestHeaders, transport, socketPathDesc }) {
  const { delay, followRedirect, httpTimeout, interval, reverse, simultaneous, validateStatus } = validatedOpts;
  const options = {
    url,
    method,
    // the addon takes a string map; joi admits non-string header values
    headers: Object.fromEntries(Object.entries(requestHeaders).map(([k, v]) => [k, String(v)])),
    followRedirect
  };
  // the addon's timeoutMs is a u32; unset/0 means no per-request timeout
  if (httpTimeout) options.timeoutMs = Math.min(httpTimeout, 2 ** 32 - 1);
  // optional fields only when defined (KTD7)
  for (const [name, value] of Object.entries(transport)) if (value !== undefined) options[name] = value;
  let checker;
  try {
    checker = new addon.HttpChecker(options);
  } catch (err) {
    return throwError(() => err); // errors reach the callback, never a synchronous throw
  }
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
  const check = async () => {
    const r = await checker.check(status);
    if (r.error) output(`  HTTP(S) error for ${url} ${r.error}`);
    else output(`  HTTP(S) result for ${url}: ${util.inspect({ status: r.status, statusText: r.statusText, ok: r.ok })}`);
    return r.ok;
  };
  const checkFn = reverse ? negateAsync(check) : check;
  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`making HTTP(S) ${method} request to ${socketPathDesc} url:${url} ...`);
      return from(checkFn());
    }, simultaneous),
    startWith(false),
    distinctUntilChanged(),
    take(2),
    finalize(() => checker.cancel())
  );
}

async function httpCallSucceeds(output, url, fetchOptions, validateStatus, httpTimeout, teardownSignal) {
  try {
    // Combine teardown with a fresh per-request timeout signal; the timeout also
    // bounds the body read below so a slow response body still counts against httpTimeout.
    const signal = httpTimeout
      ? AbortSignal.any([teardownSignal, AbortSignal.timeout(httpTimeout)])
      : teardownSignal;
    const res = await fetch(url, { ...fetchOptions, signal });
    try {
      const ok = validateStatus ? validateStatus(res.status) : res.status >= 200 && res.status < 300;
      // Consume the body under the same signal so a slow GET body still honors httpTimeout.
      if (ok) await res.arrayBuffer();
      output(`  HTTP(S) result for ${url}: ${util.inspect({ status: res.status, statusText: res.statusText, ok })}`);
      return ok;
    } finally {
      // Drain/cancel the body on every remaining exit path — a false status check or a
      // validateStatus that throws — so no undici connection leaks back into the pool.
      if (res.body && !res.bodyUsed) await res.body.cancel().catch(() => {});
    }
  } catch (err) {
    output(`  HTTP(S) error for ${url} ${err.toString()}`);
    return false;
  }
}

function createTCP$({ validatedOpts: { delay, interval, tcpTimeout, reverse, simultaneous }, output, addon }, resource) {
  const tcpPath = extractPath(resource);
  const check = typeof addon?.tcpCheck === 'function' ? addonTcpExists(addon) : tcpExists;
  const checkFn = reverse ? negateAsync(check) : check;
  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`making TCP connection to ${tcpPath} ...`);
      return from(checkFn(output, tcpPath, tcpTimeout));
    }, simultaneous),
    startWith(false),
    distinctUntilChanged(),
    take(2)
  );
}

// the addon's tcpCheck answers; the verbose lines are tcpExists' own
function addonTcpExists(addon) {
  return async function (output, tcpPath, tcpTimeout) {
    const { host, port } = tcpHostPort(tcpPath);
    const { ready, timedOut, reason } = await addon.tcpCheck(host, Number(port), tcpTimeout);
    if (ready) output(`  TCP connection successful to host:${host} port:${port}`);
    else if (timedOut) output(`  timed out connecting to TCP host:${host} port:${port} tcpTimeout:${tcpTimeout}ms`);
    else output(`  error connecting to TCP host:${host} port:${port} ${reason}`);
    return ready;
  };
}

async function tcpExists(output, tcpPath, tcpTimeout) {
  const { host, port } = tcpHostPort(tcpPath);
  return new Promise((resolve) => {
    const conn = net
      .connect(port, host)
      .on('error', (err) => {
        output(`  error connecting to TCP host:${host} port:${port} ${err.toString()}`);
        resolve(false);
      })
      .on('timeout', () => {
        output(`  timed out connecting to TCP host:${host} port:${port} tcpTimeout:${tcpTimeout}ms`);
        conn.destroy();
        resolve(false);
      })
      .on('connect', () => {
        output(`  TCP connection successful to host:${host} port:${port}`);
        conn.destroy();
        resolve(true);
      });
    conn.setTimeout(tcpTimeout);
  });
}

function createSocket$({ validatedOpts: { delay, interval, reverse, simultaneous }, output, addon }, resource) {
  const socketPath = extractPath(resource);
  const check = typeof addon?.socketCheck === 'function' ? addonSocketExists(addon) : socketExists;
  const checkFn = reverse ? negateAsync(check) : check;
  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`making socket connection to ${socketPath} ...`);
      return from(checkFn(output, socketPath));
    }, simultaneous),
    startWith(false),
    distinctUntilChanged(),
    take(2)
  );
}

// the addon's socketCheck answers; the verbose lines are socketExists' own
function addonSocketExists(addon) {
  return async function (output, socketPath) {
    const { ready, reason } = await addon.socketCheck(socketPath);
    if (ready) output(`  connected to socket:${socketPath}`);
    else output(`  error connecting to socket socket:${socketPath} ${reason}`);
    return ready;
  };
}

async function socketExists(output, socketPath) {
  return new Promise((resolve) => {
    const conn = net
      .connect(socketPath)
      .on('error', (err) => {
        output(`  error connecting to socket socket:${socketPath} ${err.toString()}`);
        resolve(false);
      })
      .on('connect', () => {
        output(`  connected to socket:${socketPath}`);
        conn.destroy();
        resolve(true);
      });
  });
}

function createCommand$({ validatedOpts: { delay, interval, reverse, commandTimeout }, output, addon }, resource) {
  const command = extractPath(resource);
  const check = addon ? (...args) => addonCommandPasses(addon, ...args) : commandPasses;
  const checkFn = reverse ? negateAsync(check) : check;
  // exhaustMap (not mergeMap/simultaneous): one exec in flight per resource. While a
  // command runs, later timer ticks are dropped, so a command that outlasts `interval`
  // is never spawned concurrently with itself.
  return timer(delay, interval).pipe(
    exhaustMap(() => {
      output(`executing command "${command}" ...`);
      return from(checkFn(output, command, commandTimeout));
    }),
    startWith(false),
    distinctUntilChanged(),
    take(2)
  );
}

async function commandPasses(output, command, commandTimeout) {
  try {
    // commandTimeout > 0 kills a hung command at the per-attempt bound (SIGKILL, since a
    // hung process may ignore SIGTERM); the rejection is caught below and polling continues.
    // ponytail: kills the shell; a detached grandchild can outlive it. commandTimeout is
    // the per-attempt bound, not a hard process-group kill.
    const { stdout } = await exec(command, { ...(commandTimeout ? { timeout: commandTimeout, killSignal: 'SIGKILL' } : {}) });
    output(`  Command "${command}" success. stdout: "${stdout}"`);
    return true;
  } catch (e) {
    output(`  Command error: "${e.message}"`);
    return false;
  }
}

// Rust engine: the addon runs the attempt (same shell, exit 0 = ready, commandTimeout kill)
// and never rejects; the log lines keep commandPasses' shape.
async function addonCommandPasses(addon, output, command, commandTimeout) {
  const { ok, stdout, error } = await addon.runCommand(command, commandTimeout);
  output(ok ? `  Command "${command}" success. stdout: "${stdout}"` : `  Command error: "${error}"`);
  return ok;
}

function negateAsync(asyncFn) {
  return async function (...args) {
    return !(await asyncFn(...args));
  };
}

module.exports = { run };
