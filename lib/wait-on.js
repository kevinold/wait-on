'use strict';

const Joi = require('joi');
const { resolveEngine } = require('./engine');
const {
  noop,
  once,
  isNotABoolean,
  isNotEmpty,
  HOST_PORT_RE,
  HTTP_PREFIX_RE,
  HTTP_UNIX_RE,
  HTTP_UNIX_LEGACY_RE,
  TIMEOUT_ERR_MSG,
  determineRemainingResources,
  extractPath,
  extractPrefix,
  routesHttpToRust,
  envProxyFor,
  proxyObjectUri
} = require('./resources');

const WAIT_ON_SCHEMA = Joi.object({
  resources: Joi.array().items(Joi.string().required()).required(),
  delay: Joi.number().integer().min(0).default(0),
  httpTimeout: Joi.number().integer().min(0),
  interval: Joi.number().integer().min(0).default(250),
  log: Joi.boolean().default(false),
  reverse: Joi.boolean().default(false),
  simultaneous: Joi.number().integer().min(1).default(Infinity),
  timeout: Joi.number().integer().min(0).default(Infinity),
  validateStatus: Joi.function(),
  verbose: Joi.boolean().default(false),
  window: Joi.number().integer().min(0).default(750),
  tcpTimeout: Joi.number().integer().min(0).default(300),
  commandTimeout: Joi.number().integer().min(0).default(0), // per-attempt kill for command: resources, 0 = no limit

  // http/https options
  ca: [Joi.string(), Joi.binary()],
  cert: [Joi.string(), Joi.binary()],
  key: [Joi.string(), Joi.binary(), Joi.object()],
  passphrase: Joi.string(),
  proxy: [
    Joi.boolean(),
    Joi.object({
      host: Joi.string().required(),
      port: Joi.number().required(),
      protocol: Joi.string(),
      auth: Joi.object({ username: Joi.string(), password: Joi.string() })
    })
  ],
  auth: Joi.object({
    username: Joi.string(),
    password: Joi.string()
  }),
  strictSSL: Joi.boolean().default(false),
  followRedirect: Joi.boolean().default(true), // HTTP 3XX responses
  headers: Joi.object()
});

/**
   Waits for resources to become available before calling callback

   Polls file, http(s), tcp ports, sockets for availability.

   Resource types are distinquished by their prefix with default being `file:`
   - file:/path/to/file - waits for file to be available and size to stabilize
   - http://foo.com:8000/bar verifies HTTP HEAD request returns 2XX
   - https://my.bar.com/cat verifies HTTPS HEAD request returns 2XX
   - http-get:  - HTTP GET returns 2XX response. ex: http://m.com:90/foo
   - https-get: - HTTPS GET returns 2XX response. ex: https://my/bar
   - tcp:my.server.com:3000 verifies a service is listening on port
   - socket:/path/sock verifies a service is listening on (UDS) socket
     For http over socket, use http://unix:SOCK_PATH:URL_PATH
                    like http://unix:/path/to/sock:/foo/bar or
                         http-get://unix:/path/to/sock:/foo/bar
   - command:<shell command> succeeds when the command exits 0. ex: command:pg_isready

   @param opts object configuring waitOn, or a string / string array used as opts.resources shorthand
   @param opts.resources array of string resources to wait for. prefix determines the type of resource with the default type of `file:`
   @param opts.delay integer - optional initial delay in ms, default 0
   @param opts.httpTimeout integer - optional http HEAD/GET timeout to wait for request, default 0
   @param opts.interval integer - optional poll resource interval in ms, default 250ms
   @param opts.log boolean - optional flag to turn on logging to stdout
   @param opts.reverse boolean - optional flag which reverses the mode, succeeds when resources are not available
   @param opts.simultaneous integer - optional limit of concurrent connections to a resource, default Infinity
   @param opts.tcpTimeout - Maximum time in ms for tcp connect, default 300ms
   @param opts.commandTimeout integer - optional per-attempt timeout in ms for command: resources, default 0 (no limit). A command still running at this bound is killed and the next poll retries
   @param opts.timeout integer - optional timeout in ms, default Infinity. Aborts with error.
   @param opts.verbose boolean - optional flag to turn on debug log
   @param opts.window integer - optional stabilization time in ms, default 750ms. Waits this amount of time for file sizes to stabilize or other resource availability to remain unchanged. If less than interval then will be reset to interval
   @param [cb] optional callback function with signature cb(err) - if err is provided then, resource checks did not succeed
   if not specified, wait-on will return a promise that will be rejected if resource checks did not succeed or resolved otherwise
 */
function waitOn(opts, cb) {
  // Shorthand: a string or string array is treated as opts.resources
  if (typeof opts === 'string' || Array.isArray(opts)) opts = { resources: [].concat(opts) };
  if (cb !== undefined) {
    return waitOnImpl(opts, cb);
  } else {
    // promise API
    return new Promise(function (resolve, reject) {
      waitOnImpl(opts, function (err) {
        if (err) {
          reject(err);
        } else {
          resolve();
        }
      });
    });
  }
}

function waitOnImpl(opts, cbFunc) {
  const cbOnce = once(cbFunc);
  const validResult = WAIT_ON_SCHEMA.validate(opts);
  if (validResult.error) {
    return cbOnce(validResult.error);
  }
  const validatedOpts = {
    ...validResult.value, // use defaults
    // window needs to be at least interval
    ...(validResult.value.window < validResult.value.interval ? { window: validResult.value.interval } : {}),
    ...(validResult.value.verbose ? { log: true } : {}) // if debug logging then normal log is also enabled
  };

  const { resources, log: shouldLog, verbose, reverse } = validatedOpts;

  // fail fast on malformed resources instead of polling until timeout
  const resourceError = validateResources(resources);
  if (resourceError) {
    return cbOnce(resourceError);
  }

  // validate WAIT_ON_ENGINE and, under rust-strict, fail fast when the addon cannot load;
  // a loaded addon (null under js or rust fallback) answers the ported checks
  let addon;
  try {
    ({ addon } = resolveEngine(process.env));
  } catch (engineError) {
    return cbOnce(engineError);
  }

  const output = verbose ? console.log.bind() : noop;
  const log = shouldLog ? console.log.bind() : noop;

  function cleanup(err) {
    if (err) {
      if (err.message.startsWith(TIMEOUT_ERR_MSG)) {
        log('wait-on(%s) %s; exiting with error', process.pid, err.message);
      } else {
        log('wait-on(%s) exiting with error', process.pid, err);
      }
    } else {
      // no error, we are complete
      log('wait-on(%s) complete', process.pid);
    }
    cbOnce(err);
  }

  if (reverse) {
    log('wait-on reverse mode - waiting for resources to be unavailable');
  }
  require('./engine-js').run({ validatedOpts, addon, log, output, cleanup });
}

// Returns an Error for the first syntactically malformed resource, else null.
// These fail immediately rather than polling until the global timeout.
function validateResources(resources) {
  for (const resource of resources) {
    const err = validateResource(resource);
    if (err) {
      return err;
    }
  }
  return null;
}

function validateResource(resource) {
  switch (extractPrefix(resource)) {
    case 'https-get:':
    case 'http-get:':
    case 'https:':
    case 'http:': {
      const url = resource.replace('-get:', ':');
      // http over a unix socket/named pipe: http://unix:SOCK_PATH:URL_PATH
      if (HTTP_UNIX_RE.test(url) || HTTP_UNIX_LEGACY_RE.test(url)) {
        return null;
      }
      // require `//` right after the scheme; `new URL('http:localhost:3000')`
      // parses successfully, so parsing alone would not catch this (#217)
      if (!HTTP_PREFIX_RE.test(url)) {
        return new Error(`Invalid resource "${resource}": http(s) resources must include "//", e.g. http://host:port/path`);
      }
      try {
        new URL(url);
      } catch {
        return new Error(`Invalid resource "${resource}": not a valid URL`);
      }
      return null;
    }
    case 'tcp:': {
      // tcp uses a bare host:port, not a URL authority (#140)
      if (/^tcp:\/\//.test(resource)) {
        return new Error(`Invalid resource "${resource}": use tcp:host:port (no "//"), e.g. tcp:127.0.0.1:3000`);
      }
      if (!HOST_PORT_RE.test(extractPath(resource))) {
        return new Error(`Invalid resource "${resource}": expected tcp:host:port or tcp:[ipv6]:port`);
      }
      return null;
    }
    default:
      return null;
  }
}

module.exports = waitOn;
// Exposed for unit tests only: the native replacements for the former lodash/fp
// helpers (drop-lodash). Not part of the public API.
module.exports._internal = { once, noop, isNotABoolean, isNotEmpty, determineRemainingResources, routesHttpToRust, envProxyFor, proxyObjectUri };
