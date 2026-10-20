'use strict';

// Pure helpers shared by the front door (lib/wait-on.js) and the engines. Loads neither
// rxjs nor undici (KTD5), so requiring wait-on stays light until a JS-engine wait runs.

// Native replacements for the former lodash/fp helpers (drop lodash dependency)
const noop = () => {};
function once(fn) {
  let called = false;
  let result;
  return function (...args) {
    if (!called) {
      called = true;
      result = fn.apply(this, args);
    }
    return result;
  };
}
const isNotABoolean = (x) => typeof x !== 'boolean';
const isNotEmpty = (arr) => arr.length > 0;
const PREFIX_RE = /^((https?-get|https?|tcp|socket|file|command):)(.+)$/;
// host:port, bare port (host defaults to localhost), or bracketed IPv6 [::1]:port
const HOST_PORT_RE = /^(?:\[([^\]]+)\]:|([^:]*):)?(\d+)$/;
const HTTP_GET_RE = /^https?-get:/;
const HTTP_PREFIX_RE = /^https?:\/\//;
// the socket path may contain colons (e.g. Windows named pipes like \\?\pipe\C:\app\sock),
// so split at the colon which is followed by the url or url path
const HTTP_UNIX_RE = /^http:\/\/unix:(.+?):((?:https?:\/\/|\/).*)$/;
const HTTP_UNIX_LEGACY_RE = /^http:\/\/unix:([^:]+):(.+)$/;
const TIMEOUT_ERR_MSG = 'Timed out waiting for';

function determineRemainingResources(resources, resourceStates) {
  // resourcesState is array of completed booleans; pair each resource with its state
  const resourceAndStateTuples = resources.map((r, i) => [r, resourceStates[i]]);
  return resourceAndStateTuples.filter(([, /* r */ s]) => !s).map(([r /*, s */]) => r);
}

function extractPath(resource) {
  const m = PREFIX_RE.exec(resource);
  if (m) {
    return m[3];
  }
  return resource;
}

function extractPrefix(resource) {
  const m = PREFIX_RE.exec(resource);
  if (m) {
    return m[1];
  }
  return '';
}

// axios-shaped proxy object { host, port, auth: { username, password }, protocol } -> URI.
// Normalize like axios did so the proxy URL always parses: accept a protocol with or
// without a trailing ':' ('http' or 'http:'), and bracket a bare IPv6 host ('::1' ->
// '[::1]'). Percent-encode credentials so the URL parse round-trips names/passwords
// containing reserved characters (`/`, `#`, `@`, `:`).
function proxyObjectUri(proxy) {
  const protocol = String(proxy.protocol || 'http').replace(/:$/, '');
  const host = /:/.test(proxy.host) && !/^\[.*\]$/.test(proxy.host) ? `[${proxy.host}]` : proxy.host;
  const credentials = proxy.auth
    ? `${encodeURIComponent(proxy.auth.username)}:${encodeURIComponent(proxy.auth.password ?? '')}@`
    : '';
  return `${protocol}://${credentials}${host}:${proxy.port}`;
}

const DEFAULT_PORTS = { 'http:': 80, 'https:': 443 };

// KTD4: the proxy undici's EnvHttpProxyAgent picks for url (proxy option unset), ported
// so the addon gets the same one. Returns the proxy URI or undefined (direct).
function envProxyFor(url, env) {
  const { protocol, hostname, port } = new URL(url);
  const httpProxy = env.http_proxy ?? env.HTTP_PROXY;
  const uri = (protocol === 'https:' && (env.https_proxy ?? env.HTTPS_PROXY)) || httpProxy;
  if (!uri) return undefined;
  const host = hostname.replace(/^\[(.+)\]$/, '$1').replace(/(.)\.$/, '$1');
  const noProxy = env.no_proxy ?? env.NO_PROXY ?? '';
  return noProxyMatches(host, Number(port) || DEFAULT_PORTS[protocol], noProxy) ? undefined : uri;
}

// undici's NO_PROXY grammar: entries split on commas/whitespace; `*` (or `*:port`)
// matches all; an entry port must equal the target port; `*.x` matches subdomains only;
// `.x` or `x` matches the apex and subdomains; bracketed or bare IPv6.
function noProxyMatches(hostname, port, noProxy) {
  return noProxy
    .split(/[,\s]/)
    .filter(Boolean)
    .some((entry) => {
      const v6 = /^\[(.+)\]:(\d+)$/.exec(entry);
      const bare = entry.replace(/^\[(.+)\]$/, '$1');
      const hostPort = v6 || ((bare.match(/:/g) || []).length === 1 && /^(.+):(\d+)$/.exec(bare));
      const entryPort = hostPort ? Number(hostPort[2]) : 0;
      if (entryPort && entryPort !== port) return false;
      const host = (hostPort ? hostPort[1] : bare)
        .replace(/^\*?\./, '')
        .replace(/^(.+)\.$/, '$1')
        .toLowerCase();
      if (host === '*') return true;
      return (!entry.startsWith('*') && hostname === host) || hostname.endsWith(`.${host}`);
    });
}

const isHttpUrl = (uri) => URL.canParse(uri) && new URL(uri).protocol === 'http:';

// KTD7/KTD10: the addon answers every http check except a url with userinfo (undici
// rejects it, reqwest would send Basic auth) and two proxy carve-outs that stay on
// undici: (a) an https target behind an env proxy (the JS path drops TLS options there),
// (b) a proxy URI in play that is not a valid http: URL (https:, socks*, malformed).
function routesHttpToRust({ addon, validatedOpts: { proxy }, socketPath, env, url }) {
  if (!addon || hasUserinfo(url)) return false;
  if (socketPath !== undefined || proxy === false) return true;
  if (proxy && typeof proxy === 'object') return isHttpUrl(proxyObjectUri(proxy));
  // proxy unset: undici builds a ProxyAgent from each set env value, used or not
  const envProxies = [env.http_proxy ?? env.HTTP_PROXY, env.https_proxy ?? env.HTTPS_PROXY].filter(Boolean);
  if (!envProxies.every(isHttpUrl)) return false;
  return !(new URL(url).protocol === 'https:' && envProxyFor(url, env));
}

function hasUserinfo(url) {
  const { username, password } = new URL(url);
  return Boolean(username || password);
}

// What both engines send for an http(s)[-get]: resource: method, url, unix socketPath and
// the request headers with auth folded in.
function httpRequest(resource, { auth, headers }) {
  const method = HTTP_GET_RE.test(resource) ? 'GET' : 'HEAD';
  const rawUrl = resource.replace('-get:', ':');
  // the socket path may contain colons (Windows named pipes); HTTP_UNIX_RE splits at
  // the colon before the url/url-path, with HTTP_UNIX_LEGACY_RE as the simple fallback.
  const matchHttpUnixSocket = HTTP_UNIX_RE.exec(rawUrl) || HTTP_UNIX_LEGACY_RE.exec(rawUrl); // http://unix:/sock:/url
  const socketPath = matchHttpUnixSocket ? matchHttpUnixSocket[1] : undefined;
  // For unix sockets HTTP_UNIX_RE yields either a relative ('/foo') or absolute
  // ('http://localhost/foo') path; new URL(..., base) normalizes both to a URL
  // undici accepts, while socketPath carries the actual connection.
  const url = socketPath ? new URL(matchHttpUnixSocket[2], 'http://localhost').href : rawUrl;

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
  return { method, url, socketPath, requestHeaders };
}

function tcpHostPort(tcpPath) {
  const [, /* full */ ipv6, hostMatched, port] = HOST_PORT_RE.exec(tcpPath);
  return { host: ipv6 || hostMatched || 'localhost', port };
}

module.exports = {
  noop,
  once,
  isNotABoolean,
  isNotEmpty,
  HOST_PORT_RE,
  HTTP_GET_RE,
  HTTP_PREFIX_RE,
  HTTP_UNIX_RE,
  HTTP_UNIX_LEGACY_RE,
  TIMEOUT_ERR_MSG,
  determineRemainingResources,
  extractPath,
  extractPrefix,
  proxyObjectUri,
  envProxyFor,
  routesHttpToRust,
  httpRequest,
  tcpHostPort
};
