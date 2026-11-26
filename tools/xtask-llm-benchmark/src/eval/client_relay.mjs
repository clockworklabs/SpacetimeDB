// Runs in a network-disabled container. Only loopback is available to the candidate.
// The parent forwards these streams to one fixed test server over Docker's stdio.
import http from 'node:http';
import { createInterface } from 'node:readline';

const sockets = new Map();
const tokenReplies = new Map();
const endpoint = `/v1/database/${encodeURIComponent(process.argv[1])}/subscribe`;
let nextId = 0;
const send = message => process.stdout.write(JSON.stringify(message) + '\n');
const server = http.createServer((request, response) => {
  // The TypeScript SDK exchanges its existing credential for a short-lived WS token.
  // This fixed, bodyless authentication call cannot invoke database functions.
  if (
    request.method === 'POST' &&
    request.url === '/v1/identity/websocket-token' &&
    request.headers['transfer-encoding'] === undefined &&
    Number(request.headers['content-length'] ?? 0) === 0 &&
    typeof request.headers.authorization === 'string' &&
    tokenReplies.size < 32
  ) {
    const id = ++nextId;
    tokenReplies.set(id, response);
    response.on('close', () => {
      tokenReplies.delete(id);
      send({ type: 'close', id });
    });
    send({ type: 'token', id, authorization: request.headers.authorization });
    return;
  }
  response.writeHead(403, { Connection: 'close' });
  response.end();
});
server.maxConnections = 32;
server.on('upgrade', (request, socket, head) => {
  // Use Node's HTTP parser and reject bodies/pipelining. Only a WebSocket upgrade
  // for this database may leave this container, never general HTTP requests.
  const url = new URL(request.url, 'http://relay');
  if (
    request.method !== 'GET' ||
    !request.url.startsWith('/') ||
    url.origin !== 'http://relay' ||
    url.pathname !== endpoint ||
    request.headers.upgrade?.toLowerCase() !== 'websocket' ||
    request.headers['sec-websocket-version'] !== '13' ||
    request.headers['content-length'] !== undefined ||
    request.headers['transfer-encoding'] !== undefined ||
    head.length !== 0 ||
    sockets.size >= 32
  ) {
    socket.destroy();
    return;
  }
  const id = ++nextId;
  sockets.set(id, socket);
  const headers = { Connection: 'Upgrade', Upgrade: 'websocket' };
  for (const key of [
    'authorization',
    'sec-websocket-key',
    'sec-websocket-version',
    'sec-websocket-protocol',
  ]) {
    if (request.headers[key] !== undefined) headers[key] = request.headers[key];
  }
  send({ type: 'open', id, path: endpoint + url.search, headers });
  socket.on('data', data => {
    if (!send({ type: 'data', id, data: data.toString('base64') })) {
      socket.pause();
      process.stdout.once('drain', () => socket.resume());
    }
  });
  socket.on('error', () => socket.destroy());
  socket.on('close', () => {
    sockets.delete(id);
    send({ type: 'close', id });
  });
});
createInterface({ input: process.stdin })
  .on('line', line => {
    const message = JSON.parse(line);
    if (message.type === 'token') {
      const response = tokenReplies.get(message.id);
      tokenReplies.delete(message.id);
      response?.writeHead(message.status, {
        'Content-Type': 'application/json',
        Connection: 'close',
      });
      response?.end(Buffer.from(message.data, 'base64'));
      return;
    }
    const socket = sockets.get(message.id);
    if (message.type === 'data' && socket) {
      if (!socket.write(Buffer.from(message.data, 'base64'))) {
        send({ type: 'pause', id: message.id });
        socket.once('drain', () => send({ type: 'resume', id: message.id }));
      }
    } else if (message.type === 'close') socket?.destroy();
    else if (message.type === 'pause') socket?.pause();
    else if (message.type === 'resume') socket?.resume();
  })
  .on('close', () => {
    for (const socket of sockets.values()) socket.destroy();
    for (const response of tokenReplies.values()) response.destroy();
    server.close();
  });
server.listen(8080, '127.0.0.1', () => send({ type: 'ready' }));
