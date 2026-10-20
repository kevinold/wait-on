'use strict';

// Shared TLS test fixture (KTD6): an EC P-256 self-signed leaf webpki accepts as its own
// trust anchor (SAN for localhost, CA:FALSE), an unrelated second leaf, and a
// PKCS#8-encrypted copy of the key. Returns null when openssl is missing (callers skip).

const { execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const PASSPHRASE = 'wait-on-test-passphrase';

function openssl(args) {
  execFileSync('openssl', args, { stdio: 'ignore' });
}

module.exports = function tlsFixture() {
  try {
    openssl(['version']);
  } catch {
    return null;
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'wait-on-tls-'));
  const file = (f) => path.join(dir, f);
  // ponytail: -addext needs OpenSSL >= 1.1.1; add a -config fallback if an older one shows up in CI.
  const leaf = (keyFile, certFile) =>
    openssl([
      'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
      '-keyout', file(keyFile), '-out', file(certFile), '-days', '1', '-nodes', '-subj', '/CN=localhost',
      '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1',
      '-addext', 'basicConstraints=critical,CA:FALSE'
    ]);
  leaf('key.pem', 'cert.pem');
  leaf('other-key.pem', 'other-cert.pem');
  openssl(['pkcs8', '-topk8', '-v2', 'aes-256-cbc', '-in', file('key.pem'), '-out', file('key-enc.pem'), '-passout', `pass:${PASSPHRASE}`]);

  const read = (f) => fs.readFileSync(file(f));
  return {
    dir,
    key: read('key.pem'),
    cert: read('cert.pem'),
    otherCert: read('other-cert.pem'),
    encryptedKey: read('key-enc.pem'),
    passphrase: PASSPHRASE,
    // Windows has no Unix domain sockets; Node listens on named pipes instead.
    socketPathFor: (name) => (process.platform === 'win32' ? path.join('\\\\?\\pipe', dir, name) : path.join(dir, name)),
    cleanup: () => fs.rmSync(dir, { recursive: true, force: true })
  };
};
