---
title: Porting undici TLS options to reqwest+rustls needs JS-supplied roots, a PEM identity, and a build()-failure fallback
date: 2026-09-30
category: best-practices
module: crates/wait-on-core
problem_type: best_practice
component: rust-engine
severity: high
applies_when:
  - "Porting a Node/undici TLS option (strictSSL, ca, cert, key, passphrase) to the Rust http checker"
  - "Adding or changing reqwest TLS features or root/identity handling in HttpChecker::new or with_tls"
  - "Writing a TLS test fixture that must verify under both Node and webpki"
retire_when: "reqwest drops rustls-platform-verifier from rustls-no-provider and gains a rustls Identity::from_pkcs8_pem; check the [features] table and src/tls.rs of the locked reqwest version"
related_components: [lib/wait-on.js, crates/wait-on-napi, test/helpers/tls-fixture.js, docs/guides/architecture.md]
tags: [reqwest, rustls, webpki, undici, tls, pkcs8, napi-rs, parity, spike-next-rs]
---

# Porting undici TLS options to reqwest+rustls needs JS-supplied roots, a PEM identity, and a build()-failure fallback

## Context

Lane L5 (issue #57) moved the undici TLS client options (`strictSSL`, `ca`, `cert`, `key`, `passphrase`) to the opt-in Rust engine (`WAIT_ON_ENGINE=rust`). The Rust engine uses reqwest 0.13 with rustls. Both engines must pass the same mocha suites. reqwest's defaults look close to undici's, but they differ in ways that only show up under a real handshake:

- reqwest picks a different trust store than undici.
- The Identity constructor you would reach for first is not compiled under rustls.
- Bad TLS material fails at a different point.
- webpki is stricter about certificate shape than OpenSSL.

The plan got two of these wrong. It assumed `Identity::from_pkcs8_pem`, and it assumed a stored-error mechanism for bad material. Review caught both before the code landed.

The crate enables reqwest with `default-features = false, features = ["rustls-no-provider"]` (`crates/wait-on-core/Cargo.toml:10`).

## Guidance

### 1. Pass Node's roots from JS and call `tls_certs_only`. Never fall back to reqwest's default verifier.

`rustls-no-provider` pulls in `rustls-platform-verifier` (reqwest-0.13.5 `Cargo.toml:174-175`). In `ClientBuilder::build`, whenever `tls_certs_only` is **not** set (reqwest-0.13.5 `src/async_impl/client.rs:750`, crate source, not this repo):

- no roots → `rustls_platform_verifier::Verifier::new` (`client.rs:759`). The OS trust store.
- roots added with `tls_certs_merge` → `Verifier::new_with_extra_roots` (`client.rs:767`). The OS store plus yours.

Only `tls_certs_only` (which sets `config.tls_certs_only = true`, `client.rs:1902-1905`) reaches `with_root_certificates(rustls_store(root_certs))` (`client.rs:788-790`). That is rustls's webpki verifier over exactly the given roots.

undici trusts Node's bundled roots, not the OS store, and a `ca` option replaces those defaults. So JS prepares the root list and Rust uses it as-is:

```js
// lib/wait-on.js:463-466
function rustTlsOptions({ ca, cert, key, passphrase, strictSSL }) {
  const roots = strictSSL ? (ca === undefined ? tls.getCACertificates('default') : [String(ca)]) : undefined;
  ...
}
```

`tls.getCACertificates('default')` includes `NODE_EXTRA_CA_CERTS`, so that env var still works under the Rust engine.

```rust
// crates/wait-on-core/src/http.rs:187-196
Some(roots) => {
    let mut certs = vec![];
    for pem in roots { certs.extend(Certificate::from_pem_bundle(pem.as_bytes())?); }
    b.tls_certs_only(certs)
}
None => b.tls_danger_accept_invalid_certs(true),   // strictSSL: false
```

If `ca` is not PEM text, it parses to an empty root set on both engines, so neither engine trusts anything.

### 2. Under rustls, build the identity with `Identity::from_pem`, and decrypt the key in JS.

`Identity::from_pkcs8_pem(cert, key)` is `#[cfg(feature = "__native-tls")]` (reqwest-0.13.5 `src/tls.rs:338-339`, crate source). The rustls constructor is `Identity::from_pem(buf)` (`tls.rs:372-373`). It takes a single buffer holding both the cert and the key:

```rust
// crates/wait-on-core/src/http.rs:198-199
if let (Some(cert), Some(key)) = (&opts.cert, &opts.key) {
    b = b.identity(Identity::from_pem(format!("{cert}\n{key}").as_bytes())?);
}
```

rustls cannot read encrypted keys, so `passphrase` gets no Rust crate. Node decrypts the key and re-exports it as plain PKCS#8:

```js
// lib/wait-on.js:468-474
function pkcs8Pem(key, passphrase) {
  try {
    return crypto.createPrivateKey({ key, passphrase }).export({ type: 'pkcs8', format: 'pem' });
  } catch {
    return String(key);   // unparsable: pass as given, let the Rust fallback handle it
  }
}
```

### 3. Wrap both PEM parsing and `build()` in the bad-material fallback.

Successful PEM parsing does not mean the TLS material is valid. In reqwest:

- `Certificate::from_der` only stores the bytes when rustls is the backend (`tls.rs:166-172`). Bad DER fails later, inside `rustls_store` → `add_to_rustls` (`tls.rs:639-644`), and that only runs during `build()`.
- A cert/key mismatch also fails during `build()`, while client auth is being set up.

Node fails such material per connection: TLS hops fail and plain http hops still succeed. It never rejects `waitOn` up front. To match that, the Rust fallback wraps `with_tls` **and** `build()`. On error it rebuilds with an empty root set and no danger flag:

```rust
// crates/wait-on-core/src/http.rs:98-108
let client = with_tls(base(), &opts)
    .and_then(ClientBuilder::build)
    .or_else(|e| if has_tls { base().tls_certs_only([]).build() } else { Err(e) })
    .map_err(|e| error_chain(&e))?;
```

With `tls_certs_only([])` no certificate verifies, so every https hop fails, and http hops are unaffected. A constructor error would instead reject `waitOn` immediately, which Node never does. A stored-error mechanism would add state just to reproduce what the empty root set already gives.

### 4. Generate test certificates with a SAN and `CA:FALSE`.

webpki needs the server name in `subjectAltName` and rejects an end-entity certificate marked `basicConstraints CA:TRUE`. A bare `openssl req -x509 -subj /CN=localhost` produces a CN-only certificate with `CA:TRUE`. Node accepts that certificate as its own `ca`; Rust polls until timeout. The fixture adds both extensions:

```js
// test/helpers/tls-fixture.js:30-32
'-subj', '/CN=localhost',
'-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1',
'-addext', 'basicConstraints=critical,CA:FALSE'
```

This difference is documented as a deliberate engine delta in `docs/guides/architecture.md:86`.

### 5. Pass TLS material for every target scheme.

One reqwest client follows the whole redirect chain, so an `http:` resource can redirect to `https:`. `rustTlsOptions` is spread into the transport options regardless of scheme (`lib/wait-on.js:527`; documented at `docs/guides/architecture.md:78`).

## Why This Matters

Each of these mistakes is quiet.

- With the platform verifier, tests pass on a developer machine whose OS trusts the right CA, then diverge in CI or for users who rely on `NODE_EXTRA_CA_CERTS`.
- `from_pkcs8_pem` does not exist under rustls, so a plan built around it stalls at compile time.
- If bad material fails at construction, `waitOn` rejects immediately where Node would keep polling.
- A CN-only fixture makes the Rust engine look broken when the real cause is webpki's certificate-shape rules.

## When to Apply

- Mapping Node/undici TLS options (`ca`, `cert`, `key`, `passphrase`, `rejectUnauthorized`/`strictSSL`) onto reqwest + rustls, especially with `rustls-no-provider` or `rustls`. Both pull in `rustls-platform-verifier`.
- Any reqwest client that must trust exactly a given root set (`tls_certs_only`, not `tls_certs_merge` and not the default).
- Writing self-signed fixtures that a webpki-based verifier must accept.
- Designing fallback behavior for invalid user-supplied TLS material. Remember that `build()` can fail after parsing has succeeded.

## Examples

Where each input lands (full table at `docs/guides/architecture.md:67-76`):

| Input | JS (`lib/wait-on.js`) | Rust (`crates/wait-on-core/src/http.rs`) |
|---|---|---|
| `strictSSL: true`, no `ca` | `tls.getCACertificates('default')` | `tls_certs_only(roots)` |
| `strictSSL: true`, `ca` | `[String(ca)]` (replaces defaults) | `tls_certs_only(roots)` |
| `strictSSL: false` | `roots` undefined | `tls_danger_accept_invalid_certs(true)` |
| `key` + `passphrase` | `createPrivateKey(...).export({ type: 'pkcs8' })` | `Identity::from_pem(cert + "\n" + key)` |
| material that fails parse or `build()` | passed as given | rebuild with `tls_certs_only([])`: https hops fail, http hops work |

What doesn't work:

```rust
// Compiles only with native-tls; absent under rustls-no-provider.
Identity::from_pkcs8_pem(cert, key)
// Falls back to the OS trust store (platform verifier) — not undici's roots.
Client::builder().build()
// OS store + extra roots — still not exactly Node's set.
Client::builder().tls_certs_merge(certs).build()
```

## Related

- `docs/guides/architecture.md` (http(s) section): the routing table, option-preparation table and deliberate JS/Rust deltas this learning explains.
- `docs/solutions/best-practices/porting-js-regex-parsers-to-rust-with-a-differential-oracle.md`: the same "keep the decision in JS, let the JS engine be the oracle" approach for parsers.
- Issue #57 (lane L5), predecessor #56 (lane L4).
