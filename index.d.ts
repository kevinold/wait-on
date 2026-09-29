/// <reference types="node" />

import { SecureContextOptions } from 'tls';

declare function waitOn(opts: waitOn.WaitOnInput, cb: (err?: Error) => void): void;
declare function waitOn(opts: waitOn.WaitOnInput): Promise<void>;

declare namespace waitOn {
  /** A resources string (or array of them) may be passed directly as shorthand for `{ resources: [...] }`. */
  type WaitOnInput = WaitOnOptions | string | string[];

  /**
   * Extends the full Node `SecureContextOptions` for `@types/wait-on` source
   * compatibility. Only `ca`, `cert`, `key`, and `passphrase` are read at
   * runtime (passed to the https agent); other TLS fields type-check but are
   * ignored.
   */
  interface WaitOnOptions extends SecureContextOptions {
    /** Array of resources to wait for. Prefix determines type: file:, http:, https:, http-get:, https-get:, tcp:, socket:, command: */
    resources: string[];
    /** Initial delay in ms before polling begins. @default 0 */
    delay?: number | undefined;
    /** HTTP HEAD/GET timeout in ms. */
    httpTimeout?: number | undefined;
    /** Poll interval in ms. @default 250 */
    interval?: number | undefined;
    /** Log remaining resources to stdout. @default false */
    log?: boolean | undefined;
    /** Reverse mode: succeed when resources are NOT available. @default false */
    reverse?: boolean | undefined;
    /** Max concurrent connections per resource. @default Infinity */
    simultaneous?: number | undefined;
    /** Overall timeout in ms. Rejects/errors when exceeded. @default Infinity */
    timeout?: number | undefined;
    /** Custom function to determine if an HTTP status code is a success. Defaults to 2xx. */
    validateStatus?: ValidateStatus | undefined;
    /** Enable debug output (also enables log). @default false */
    verbose?: boolean | undefined;
    /** Stabilization window in ms. Resource must remain available for this duration. @default 750 */
    window?: number | undefined;
    /** TCP connect timeout in ms. @default 300 */
    tcpTimeout?: number | undefined;
    /** Per-attempt timeout in ms for `command:` resources; a command still running at this bound is killed and the next poll retries. 0 disables the limit. @default 0 */
    commandTimeout?: number | undefined;

    /** HTTP proxy configuration. Set to false to disable. @default undefined */
    proxy?: false | WaitOnProxyOptions | undefined;
    /** HTTP Basic auth credentials. */
    auth?: WaitOnAuth | undefined;
    /** Reject unauthorized TLS certificates. @default false */
    strictSSL?: boolean | undefined;
    /** Follow HTTP 3xx redirects. @default true */
    followRedirect?: boolean | undefined;
    /** Additional HTTP request headers. Values follow what the runtime (Joi.object + axios) accepts, matching `@types/wait-on`. */
    headers?: Record<string, any> | undefined;
  }

  interface WaitOnAuth {
    username: string;
    password: string;
  }

  interface WaitOnProxyOptions {
    host: string;
    port: number;
    /** @default 'http' */
    protocol?: string | undefined;
    auth?: {
      username: string;
      password: string;
    } | undefined;
  }

  type ValidateStatus = (status: number) => boolean;

  /** @deprecated use {@link WaitOnProxyOptions}. Kept for `@types/wait-on` compatibility. */
  type AxiosProxyConfig = WaitOnProxyOptions;

  /**
   * @deprecated Not used at runtime. Kept only for `@types/wait-on`
   * compatibility so existing consumers referencing this name keep compiling.
   */
  interface HttpSignature {
    keyId: string;
    key: string;
  }
}

export = waitOn;
