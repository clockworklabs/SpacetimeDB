import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage } from 'node:http';
import type { Socket } from 'node:net';
import { afterEach, describe, expect, it } from 'vitest';
import { openNodeWebSocket, NodeWebSocketAdapter } from '../src/sdk/node';
import { WebSocket } from 'ws';
import { once } from 'node:events';
import type { WebSocketArgs } from '../src/sdk/ws';
import { DbConnection } from '../test-app/src/module_bindings';
import { BinaryWriter, ConnectionId, Identity } from '../src';
import { ServerMessage } from '../src/sdk/client_api/types';

const sockets: Set<Socket> = new Set();
const adapters: NodeWebSocketAdapter[] = [];
const servers: ReturnType<typeof createServer>[] = [];
const token = 'fixture.hosted.credential';

afterEach(async () => {
  const closed = await Promise.allSettled(
    adapters.splice(0).map(a => a.shutdown())
  );
  for (const socket of sockets) socket.destroy();
  await Promise.all(
    servers.splice(0).map(
      server =>
        new Promise<void>((resolve, reject) => {
          server.close(error => (error ? reject(error) : resolve()));
        })
    )
  );
  sockets.clear();
  for (const result of closed) expect(result.status).toBe('fulfilled');
});

async function endpoint(
  upgrade: (req: IncomingMessage, socket: Socket) => void
) {
  const server = createServer((_req, res) => {
    // A token-exchange request must never be sent by this transport.
    res.writeHead(500).end();
  });
  server.on('connection', socket => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
  });
  server.on('upgrade', (req, socket) => upgrade(req, socket as Socket));
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  servers.push(server);
  const address = server.address();
  if (!address || typeof address === 'string')
    throw new Error('Expected owned TCP listener');
  return new URL(`ws://127.0.0.1:${address.port}/`);
}

function args(url: URL): WebSocketArgs {
  return {
    url,
    nameOrAddress: 'test/database',
    authToken: token,
    wsProtocol: ['fixture-protocol'],
    compression: 'none',
    lightMode: true,
    confirmedReads: false,
  };
}

function accept(
  req: IncomingMessage,
  socket: Socket,
  payload = Buffer.from([0, 111, 107]),
  answerClose = true
) {
  const accept = createHash('sha1')
    .update(
      req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
    )
    .digest('base64');
  socket.write(
    'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
      `Sec-WebSocket-Accept: ${accept}\r\nSec-WebSocket-Protocol: ${req.headers['sec-websocket-protocol']?.split(',')[0].trim()}\r\n\r\n`
  );
  // A single uncompressed SpacetimeDB payload inside a binary WS frame.
  if (payload.length >= 126)
    throw new Error('Fixture message exceeded short-frame bound');
  socket.write(Buffer.concat([Buffer.from([0x82, payload.length]), payload]));
  socket.on('data', () => {
    if (answerClose) socket.end(Buffer.from([0x88, 0]));
  });
}

describe('explicit Node.js header transport', () => {
  it('connects through the generated builder with its actual connection ID', async () => {
    let request: IncomingMessage | undefined;
    const identity = new Identity(123n);
    const url = await endpoint((req, socket) => {
      request = req;
      const connectionId = new URL(
        req.url!,
        'http://127.0.0.1'
      ).searchParams.get('connection_id');
      if (!connectionId) throw new Error('Builder connection ID missing');
      const writer = new BinaryWriter(1024);
      ServerMessage.serialize(
        writer,
        ServerMessage.InitialConnection({
          identity,
          token,
          connectionId: new ConnectionId(BigInt(`0x${connectionId}`)),
        })
      );
      accept(
        req,
        socket,
        Buffer.concat([Buffer.from([0]), writer.getBuffer()])
      );
    });
    let resolve!: () => void;
    let reject!: (error: Error) => void;
    const ready = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const connection = DbConnection.builder()
      .withUri(url)
      .withDatabaseName('fixture')
      .withToken(token)
      .withWSFn(async input => {
        const adapter = await openNodeWebSocket(input);
        adapters.push(adapter);
        return adapter;
      })
      .onConnect((_connection, actualIdentity) => {
        expect(actualIdentity.toHexString()).toBe(identity.toHexString());
        resolve();
      })
      .onConnectError((_context, error) => reject(error))
      .build();
    try {
      await ready;
      expect(request?.headers.authorization).toBe(`Bearer ${token}`);
      expect(
        new URL(request!.url!, url).searchParams.get('connection_id')
      ).toMatch(/^[0-9a-f]{32}$/);
    } finally {
      connection.disconnect();
    }
  });
  it('uses Authorization on the exact route and preserves subscription options', async () => {
    let request: IncomingMessage | undefined;
    const url = await endpoint((req, socket) => {
      request = req;
      accept(req, socket);
    });
    // The exported Node class's static factory must also use headers directly.
    const adapter = await NodeWebSocketAdapter.openWebSocket(args(url));
    adapters.push(adapter);
    const received = await new Promise<Uint8Array>((resolve, reject) => {
      adapter.onmessage = event => resolve(event.data);
      adapter.onerror = () => reject(new Error('Fixture connection failed'));
    });
    expect(new TextDecoder().decode(received)).toBe('ok');
    expect(request?.headers.authorization).toBe(`Bearer ${token}`);
    expect(request?.url).toBe(
      '/v1/database/test%2Fdatabase/subscribe?compression=None&light=true&confirmed=false'
    );
    expect(request?.url).not.toContain(token);
    expect(adapter.protocol).toBe('fixture-protocol');
    await Promise.all([adapter.shutdown(), adapter.shutdown()]);
    expect(adapter.readyState).toBe(3);
  });

  it('does not follow an upgrade redirect with credentials', async () => {
    let forwarded = false;
    const destination = await endpoint((req, socket) => {
      forwarded = true;
      accept(req, socket);
    });
    const url = await endpoint((_req, socket) => {
      socket.end(
        `HTTP/1.1 302 Found\r\nLocation: ${destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n`
      );
    });
    const adapter = await openNodeWebSocket(args(url));
    adapters.push(adapter);
    const error = await new Promise<ErrorEvent>(resolve => {
      adapter.onerror = resolve;
    });
    expect(error.message).toBe('Database WebSocket transport failed');
    expect(error.error).toBeUndefined();
    await adapter.shutdown();
    expect(forwarded).toBe(false);
  });

  it('rejects ambiguous URLs and malformed credentials before connecting', async () => {
    let connected = false;
    const url = await endpoint((req, socket) => {
      connected = true;
      accept(req, socket);
    });
    const invalid = [
      { ...args(url), authToken: 'secret\r\ninjected: header' },
      { ...args(url), authToken: 'x'.repeat(8193) },
      { ...args(url), authToken: '' },
      { ...args(url), nameOrAddress: '.' },
      { ...args(url), nameOrAddress: '..' },
      { ...args(url), nameOrAddress: 'database\n' },
      { ...args(url), nameOrAddress: 'database\u007f' },
      { ...args(url), url: new URL('?connection_id=invalid', url) },
      {
        ...args(url),
        url: new URL(
          '?connection_id=00000000000000000000000000000001&connection_id=00000000000000000000000000000002',
          url
        ),
      },
      { ...args(url), url: new URL('?token=secret', url) },
      { ...args(url), url: new URL('#secret', url) },
      { ...args(url), url: new URL(`ws://secret:secret@${url.host}/`) },
      { ...args(url), url: new URL('file:///not-a-server') },
    ];
    for (const input of invalid) {
      await expect(openNodeWebSocket(input)).rejects.toThrow(
        'Invalid Node.js database WebSocket configuration'
      );
    }
    expect(connected).toBe(false);
  });

  it('immediately completes shutdown when wrapping an already closed socket', async () => {
    const url = await endpoint((req, socket) => accept(req, socket));
    const raw = new WebSocket(url, ['fixture-protocol']);
    raw.on('error', () => {});
    try {
      await once(raw, 'open');
      const closed = once(raw, 'close');
      raw.close();
      await closed;
      const adapter = new NodeWebSocketAdapter(raw);
      adapters.push(adapter);
      await adapter.shutdown();
      expect(adapter.readyState).toBe(3);
    } finally {
      raw.terminate();
    }
  });

  it('cleans up a connection cancelled during its upgrade', async () => {
    let admitted!: () => void;
    const accepted = new Promise<void>(resolve => {
      admitted = resolve;
    });
    const url = await endpoint(() => admitted());
    const adapter = await openNodeWebSocket(args(url));
    adapters.push(adapter);
    await accepted;
    let closeEvents = 0;
    adapter.onclose = () => {
      closeEvents++;
    };
    await adapter.shutdown();
    expect(adapter.readyState).toBe(3);
    expect(closeEvents).toBe(1);
  });

  it('terminates the actual upgraded socket when the peer ignores close', async () => {
    let peerClosed!: () => void;
    const closed = new Promise<void>(resolve => {
      peerClosed = resolve;
    });
    const peerEvents: string[] = [];
    const url = await endpoint((req, socket) => {
      // Node's upgraded HTTP socket keeps its writable half open after EOF.
      // Do not confuse that with a live client transport. Observe the client's
      // actual FIN first, then finish the fixture's own writable half.
      socket.once('end', () => {
        peerEvents.push('end');
        socket.end();
      });
      socket.once('close', () => {
        peerEvents.push('close');
        peerClosed();
      });
      accept(req, socket, Buffer.from([0, 111, 107]), false);
    });
    const adapter = await openNodeWebSocket(args(url));
    adapters.push(adapter);
    await new Promise<void>(resolve => {
      adapter.onmessage = () => resolve();
    });
    let closeEvents = 0;
    adapter.onclose = () => {
      closeEvents++;
    };
    await adapter.shutdown();
    await closed;
    expect(peerEvents).toEqual(['end', 'close']);
    expect(adapter.readyState).toBe(3);
    expect(closeEvents).toBe(1);
  }, 10_000);

  it('bounds a stalled upgrade without waiting for a caller to close it', async () => {
    const url = await endpoint(() => {});
    const adapter = await openNodeWebSocket(args(url));
    adapters.push(adapter);
    await new Promise<void>(resolve => {
      adapter.onerror = () => resolve();
    });
    await adapter.shutdown();
    expect(adapter.readyState).toBe(3);
  }, 10_000);

  it('redacts a constructor failure before opening a socket', async () => {
    const url = await endpoint(() => {});
    await expect(
      openNodeWebSocket({
        ...args(url),
        wsProtocol: ['private invalid protocol'],
      })
    ).rejects.toThrow('Database WebSocket transport could not start');
  });
});
