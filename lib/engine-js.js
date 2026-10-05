'use strict';

// The JS engine: the rxjs polling pipeline, required lazily by lib/wait-on.js (KTD5).

const fs = require('fs');
const { STATUS_CODES } = require('http');
const { promisify } = require('util');
const net = require('net');
const util = require('util');
// HTTP checks use undici's dispatcher.request, not fetch: fetch enforces the WHATWG
// Fetch bad-port list (6000, 5060, 6665-6669, 10080, ...) and refuses those ports
// before connecting (#104, upstream #260); axios in 9.x had no such list. A liveness
// probe must reach any port a server can listen on.
const { Agent, ProxyAgent, EnvHttpProxyAgent, interceptors } = require('undici');
const { NEVER, combineLatest, from, merge, throwError, timer } = require('rxjs');
const { distinctUntilChanged, exhaustMap, finalize, map, mergeMap, scan, startWith, take, takeWhile } = require('rxjs/operators');
const childProcess = require('child_process');
const {
  isNotABoolean,
  isNotEmpty,
  TIMEOUT_ERR_MSG,
  determineRemainingResources,
  extractPath,
  extractPrefix,
  httpRequest,
  proxyObjectUri,
  tcpHostPort
} = require('./resources');
const fstat = promisify(fs.stat);
const exec = promisify(childProcess.exec);

function run({ validatedOpts, log, output, cleanup }) {
  const { resources, timeout } = validatedOpts;
  const logWaitingForWDeps = (resourceStates) => logWaitingFor({ log, resources }, resourceStates);
  const createResourceWithDeps$ = (resource) => createResource$({ validatedOpts, output, log }, resource);

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
  { validatedOpts: { delay, interval, reverse, simultaneous, window: stabilityWindow }, output },
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
      return from(getFileSize(filePath));
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

function createHTTP$({ validatedOpts, output }, resource) {
  const {
    delay,
    followRedirect,
    httpTimeout,
    interval,
    proxy,
    reverse,
    simultaneous,
    strictSSL: rejectUnauthorized,
    validateStatus
  } = validatedOpts;
  const { method, url, socketPath, requestHeaders } = httpRequest(resource, validatedOpts);
  const socketPathDesc = socketPath ? `socketPath:${socketPath}` : '';

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

  // fetch's defaults (axios sent both too); caller headers win, matched case-insensitively.
  const headers = { ...requestHeaders };
  const callerNames = Object.keys(headers).map((name) => name.toLowerCase());
  if (!callerNames.includes('accept')) headers.accept = '*/*';
  if (!callerNames.includes('user-agent')) headers['user-agent'] = 'undici';
  // followRedirect true (default) follows 3xx to a final response, failing past 20
  // redirects like fetch; false leaves the 3xx as the response for validateStatus.
  const requestDispatcher = followRedirect
    ? dispatcher.compose(interceptors.redirect({ maxRedirections: 20, throwOnMaxRedirect: true }))
    : dispatcher;
  const { origin, pathname, search } = new URL(url);
  const requestOptions = { origin, path: pathname + search, method, headers };

  const checkFn = reverse ? negateAsync(httpCallSucceeds) : httpCallSucceeds;
  return timer(delay, interval).pipe(
    mergeMap(() => {
      output(`making HTTP(S) ${method} request to ${socketPathDesc} url:${url} ...`);
      return from(checkFn(output, url, requestDispatcher, requestOptions, validateStatus, httpTimeout, teardown.signal));
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

async function httpCallSucceeds(output, url, dispatcher, requestOptions, validateStatus, httpTimeout, teardownSignal) {
  try {
    // Combine teardown with a fresh per-request timeout signal; the timeout also
    // bounds the body read below so a slow response body still counts against httpTimeout.
    const signal = httpTimeout
      ? AbortSignal.any([teardownSignal, AbortSignal.timeout(httpTimeout)])
      : teardownSignal;
    // fetch refused a url with credentials; dispatcher.request would drop them and send
    // anyway, so keep refusing (use opts.auth for Basic auth).
    const { username, password } = new URL(url);
    if (username || password) throw new TypeError('Request cannot be constructed from a URL that includes credentials');
    const { statusCode: status, body } = await dispatcher.request({ ...requestOptions, signal });
    let consumed = false;
    try {
      const ok = validateStatus ? validateStatus(status) : status >= 200 && status < 300;
      // Consume the body under the same signal so a slow GET body still honors httpTimeout.
      if (ok) {
        consumed = true;
        await body.arrayBuffer();
      }
      output(`  HTTP(S) result for ${url}: ${util.inspect({ status, statusText: STATUS_CODES[status], ok })}`);
      return ok;
    } finally {
      // Dump the body on every remaining exit path — a false status check or a
      // validateStatus that throws — so no undici connection leaks back into the pool.
      if (!consumed) await body.dump().catch(() => {});
    }
  } catch (err) {
    output(`  HTTP(S) error for ${url} ${err.toString()}`);
    return false;
  }
}

function createTCP$({ validatedOpts: { delay, interval, tcpTimeout, reverse, simultaneous }, output }, resource) {
  const tcpPath = extractPath(resource);
  const checkFn = reverse ? negateAsync(tcpExists) : tcpExists;
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

function createSocket$({ validatedOpts: { delay, interval, reverse, simultaneous }, output }, resource) {
  const socketPath = extractPath(resource);
  const checkFn = reverse ? negateAsync(socketExists) : socketExists;
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

function createCommand$({ validatedOpts: { delay, interval, reverse, commandTimeout }, output }, resource) {
  const command = extractPath(resource);
  const checkFn = reverse ? negateAsync(commandPasses) : commandPasses;
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

function negateAsync(asyncFn) {
  return async function (...args) {
    return !(await asyncFn(...args));
  };
}

module.exports = { run };
