import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createInterface } from 'node:readline';

export async function startTransport(host, database) {
  const target = new URL(host);
  assert.equal(target.protocol, 'http:');
  assert.ok(['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname));
  assert.match(database, /^[a-zA-Z0-9-]+$/);
  const name = `llm-client-relay-${randomUUID()}`;
  const source = readFileSync(
    new URL('./client_relay.mjs', import.meta.url),
    'utf8'
  );
  const child = spawn(
    'docker',
    [
      'run',
      '--rm',
      '--init',
      '-i',
      '--pull=never',
      '--name',
      name,
      '--network=none',
      '--read-only',
      '--cap-drop=ALL',
      '--security-opt=no-new-privileges',
      '--user=65534:65534',
      '--pids-limit=32',
      '--memory=128m',
      '--cpus=1',
      'node:22-bookworm-slim',
      'timeout',
      '--kill-after=5',
      '165',
      'node',
      '--input-type=module',
      '-e',
      source,
      database,
    ],
    { stdio: ['pipe', 'pipe', 'pipe'] }
  );
  const sockets = new Map();
  let online = true;
  let hideReplies = false;
  let failure;
  const send = message => child.stdin.write(JSON.stringify(message) + '\n');
  function disconnect(id) {
    const connection = sockets.get(id);
    if (!connection) return;
    sockets.delete(id);
    connection.request.destroy();
    connection.socket?.destroy();
    send(
      connection.token
        ? { type: 'token', id, status: 503, data: '' }
        : { type: 'close', id }
    );
  }
  const ready = new Promise((resolve, reject) => {
    child.on('error', error => {
      failure = error;
      reject(error);
    });
    child.on('exit', (code, signal) => {
      failure = new Error(`test relay exited: ${code ?? signal}`);
      reject(failure);
    });
    child.stdin.on('error', error => {
      failure = error;
      reject(error);
    });
    child.stderr.pipe(process.stderr);
    createInterface({ input: child.stdout }).on('line', line => {
      const message = JSON.parse(line);
      if (message.type === 'ready') {
        resolve();
        return;
      }
      if (message.type === 'token') {
        if (!online || sockets.size >= 32) {
          send({ type: 'token', id: message.id, status: 503, data: '' });
          return;
        }
        const request = http.request(
          new URL('/v1/identity/websocket-token', target),
          {
            method: 'POST',
            headers: {
              Authorization: message.authorization,
              'Content-Length': '0',
            },
          }
        );
        sockets.set(message.id, { request, socket: null, token: true });
        request.on('error', () => disconnect(message.id));
        request.setTimeout(10000, () => disconnect(message.id));
        request.on('response', response => {
          let body = Buffer.alloc(0);
          response.on('error', () => disconnect(message.id));
          response.on('data', chunk => {
            body = Buffer.concat([body, chunk]);
            if (body.length > 16384) {
              response.destroy();
              disconnect(message.id);
            }
          });
          response.on('end', () => {
            if (!sockets.delete(message.id)) return;
            send({
              type: 'token',
              id: message.id,
              status: response.statusCode,
              data: body.toString('base64'),
            });
          });
        });
        request.end();
        return;
      }
      if (message.type === 'open') {
        if (!online || sockets.size >= 32) {
          send({ type: 'close', id: message.id });
          return;
        }
        if (
          new URL(message.path, target).pathname !==
          `/v1/database/${database}/subscribe`
        ) {
          send({ type: 'close', id: message.id });
          return;
        }
        const request = http.request({
          hostname: target.hostname.replace(/^\[|\]$/g, ''),
          port: target.port,
          method: 'GET',
          path: message.path,
          headers: message.headers,
        });
        const connection = { request, socket: null };
        sockets.set(message.id, connection);
        request.on('error', () => disconnect(message.id));
        request.on('response', response => {
          response.destroy();
          disconnect(message.id);
        });
        request.setTimeout(10000, () => disconnect(message.id));
        request.on('upgrade', (response, socket, head) => {
          if (!sockets.has(message.id) || response.statusCode !== 101) {
            socket.destroy();
            disconnect(message.id);
            return;
          }
          connection.socket = socket;
          socket.setTimeout(0);
          const headers = [];
          for (let i = 0; i < response.rawHeaders.length; i += 2) {
            headers.push(
              `${response.rawHeaders[i]}: ${response.rawHeaders[i + 1]}`
            );
          }
          send({
            type: 'data',
            id: message.id,
            data: Buffer.concat([
              Buffer.from(
                `HTTP/1.1 101 Switching Protocols\r\n${headers.join('\r\n')}\r\n\r\n`
              ),
              head,
            ]).toString('base64'),
          });
          socket.on('data', data => {
            if (
              !hideReplies &&
              !send({
                type: 'data',
                id: message.id,
                data: data.toString('base64'),
              })
            ) {
              socket.pause();
              child.stdin.once('drain', () => socket.resume());
            }
          });
          socket.on('error', () => disconnect(message.id));
          socket.on('close', () => disconnect(message.id));
        });
        request.end();
      } else if (message.type === 'data') {
        const socket = sockets.get(message.id)?.socket;
        if (!socket) {
          disconnect(message.id);
          return;
        }
        if (!socket.write(Buffer.from(message.data, 'base64'))) {
          send({ type: 'pause', id: message.id });
          socket.once('drain', () => send({ type: 'resume', id: message.id }));
        }
      } else if (message.type === 'close') disconnect(message.id);
      else if (message.type === 'pause')
        sockets.get(message.id)?.socket?.pause();
      else if (message.type === 'resume')
        sockets.get(message.id)?.socket?.resume();
    });
  });
  function close() {
    for (const id of sockets.keys()) disconnect(id);
    const result = spawnSync('docker', ['rm', '-f', name], {
      timeout: 10000,
      encoding: 'utf8',
    });
    child.kill();
    if (
      result.status !== 0 &&
      !/No such container|removal of container .* is already in progress/.test(
        result.stderr ?? ''
      )
    ) {
      throw new Error(`Relay cleanup failed: ${result.error ?? result.stderr}`);
    }
  }
  let timer;
  try {
    await Promise.race([
      ready,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error('test relay startup timed out')),
          10000
        );
      }),
    ]);
  } catch (error) {
    close();
    throw error;
  } finally {
    clearTimeout(timer);
  }
  return {
    name,
    uri: 'http://127.0.0.1:8080',
    check() {
      if (failure) throw failure;
    },
    setOnline(value) {
      online = value;
      if (!online) for (const id of sockets.keys()) disconnect(id);
    },
    hideReplies(value) {
      hideReplies = value;
    },
    close,
  };
}
