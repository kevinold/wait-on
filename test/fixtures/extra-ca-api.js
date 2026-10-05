'use strict';

// KTD1: serves https with the tls-fixture cert in argv[2] and polls it with strictSSL and
// no ca, so only NODE_EXTRA_CA_CERTS can make it trusted. Prints the outcome and the
// counting addon's wait-call total (0 when no addon path is set, so js never loads it).
const fs = require('fs');
const https = require('https');
const path = require('path');
const waitOn = require('../../lib/wait-on');

const counting = process.env.WAIT_ON_NATIVE_LIBRARY_PATH ? require('./counting-addon') : null;
const read = (f) => fs.readFileSync(path.join(process.argv[2], f));
const server = https.createServer({ key: read('key.pem'), cert: read('cert.pem') }, (req, res) => res.end());
server.listen(0, 'localhost', async () => {
  const resources = [`https://localhost:${server.address().port}/`];
  const outcome = await waitOn({ resources, strictSSL: true, timeout: 2000, interval: 100 }).then(() => 'resolved', (e) => e.message);
  const waits = counting ? counting.calls.filter((c) => c.type === 'wait').length : 0;
  process.stdout.write(`${outcome} ${waits}\n`);
  server.closeAllConnections();
  server.close();
});
