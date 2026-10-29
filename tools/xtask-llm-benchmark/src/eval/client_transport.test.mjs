import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { startTransport } from './client_transport.mjs';

test('isolated client can upgrade only its own database WebSocket', async () => {
  const requests = [];
  const server = http.createServer((request, response) => {
    requests.push(request.url);
    if (
      request.url === '/v1/identity/websocket-token' &&
      request.method === 'POST'
    ) {
      assert.equal(request.headers.authorization, 'Bearer test');
      assert.equal(request.headers['content-length'], '0');
      response.end(JSON.stringify({ token: 'short-lived-test-token' }));
      return;
    }
    response.writeHead(403);
    response.end();
  });
  server.on('upgrade', (request, socket) => {
    requests.push(request.url);
    const accept = createHash('sha1')
      .update(
        request.headers['sec-websocket-key'] +
          '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
      )
      .digest('base64');
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`
    );
    socket.on('data', () => socket.destroy());
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let transport;
  try {
    transport = await startTransport(
      `http://127.0.0.1:${server.address().port}`,
      'task-a'
    );
    const source = `
      const assert = require('node:assert/strict');
      const net = require('node:net');
      const fs = require('node:fs');
      assert.deepEqual(fs.readdirSync('/sys/class/net'), ['lo']);
      assert.equal(fs.readFileSync('/proc/net/route', 'utf8').trim().split('\\n').length, 1);
      const handshake = path => 'GET ' + path + ' HTTP/1.1\\r\\nHost: relay\\r\\nConnection: Upgrade\\r\\nUpgrade: websocket\\r\\nSec-WebSocket-Version: 13\\r\\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\\r\\n\\r\\n';
      const rejectRequest = request => new Promise((resolve, reject) => {
        const socket = net.connect(8080, '127.0.0.1', () => socket.write(request));
        let reply = '';
        socket.on('data', data => { reply += data; });
        socket.on('error', reject);
        socket.on('close', () => {
          try { assert.ok(!reply.includes('101 Switching Protocols'), reply); resolve(); }
          catch (error) { reject(error); }
        });
        socket.setTimeout(5000, () => { socket.destroy(); reject(new Error('rejection timeout')); });
      });
      (async () => {
        await rejectRequest('POST /v1/database/task-b/call/add HTTP/1.1\\r\\nHost: relay\\r\\nContent-Length: 3\\r\\n\\r\\n[1]');
        await rejectRequest(handshake('/v1/database/task-b/subscribe'));
        await rejectRequest(handshake('/v1/database/task-a/sql'));
        await rejectRequest(handshake('/v1/database/task-a/subscribe') + 'GET /v1/database/task-b/subscribe HTTP/1.1\\r\\nHost: relay\\r\\n\\r\\n');
        await rejectRequest('POST /v1/identity/websocket-token HTTP/1.1\\r\\nHost: relay\\r\\nAuthorization: Bearer test\\r\\nContent-Length: 3\\r\\n\\r\\n[1]');
        const token = await fetch('http://127.0.0.1:8080/v1/identity/websocket-token', {
          method: 'POST', headers: { Authorization: 'Bearer test' },
        });
        assert.equal(token.status, 200);
        assert.deepEqual(await token.json(), { token: 'short-lived-test-token' });
        await new Promise((resolve, reject) => {
          const ws = new WebSocket('ws://127.0.0.1:8080/v1/database/task-a/subscribe');
          ws.onopen = () => { ws.close(); resolve(); };
          ws.onerror = reject;
        });
        await new Promise((resolve, reject) => {
          const outside = net.connect(80, '1.1.1.1');
          outside.on('connect', () => reject(new Error('external network reachable')));
          outside.on('error', error => {
            try { assert.equal(error.code, 'ENETUNREACH'); resolve(); }
            catch (error) { reject(error); }
          });
          outside.setTimeout(5000, () => { outside.destroy(); reject(new Error('isolation timeout')); });
        });
      })().catch(error => { console.error(error); process.exitCode = 1; });
    `;
    const child = spawn(
      'docker',
      [
        'run',
        '--rm',
        '--init',
        '--pull=never',
        `--network=container:${transport.name}`,
        '--read-only',
        '--cap-drop=ALL',
        '--security-opt=no-new-privileges',
        '--user=65534:65534',
        'node:22-bookworm-slim',
        'timeout',
        '--kill-after=1',
        '10',
        'node',
        '-e',
        source,
      ],
      { stdio: 'inherit' }
    );
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', resolve);
    });
    assert.equal(code, 0);
    assert.deepEqual(requests, [
      '/v1/identity/websocket-token',
      '/v1/database/task-a/subscribe',
    ]);
  } finally {
    try {
      transport?.close();
    } finally {
      await new Promise(resolve => server.close(resolve));
    }
  }
});
