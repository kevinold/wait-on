'use strict';

// AE-L4-4: a server that accepts and never answers, with every handle unref'd, so only a
// request left in flight by waitOn can keep this process alive after it settles.
const http = require('http');
const waitOn = require('../../lib/wait-on');

const server = http.createServer(() => {}); // accept, never write
server.on('connection', (socket) => socket.unref());
server.listen(0, 'localhost', async () => {
  server.unref();
  const { port } = server.address();
  try {
    await waitOn({ resources: [`http://localhost:${port}/`], timeout: 300, interval: 100 });
  } catch {
    process.stdout.write('settled\n');
  }
});
