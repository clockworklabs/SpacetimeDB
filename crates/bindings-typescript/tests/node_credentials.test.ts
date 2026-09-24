import { afterEach, describe, expect, it } from 'vitest';
import {
  createServer,
  type Server,
  type ServerResponse,
  type RequestListener,
} from 'node:http';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import {
  createServer as createNetServer,
  type Server as NetServer,
  type AddressInfo,
  type Socket,
} from 'node:net';
import { join } from 'node:path';
import { inspect } from 'node:util';
import { Container, ContainerCredentialError } from '../src/sdk/node/container';
import { Identity } from '../src/lib/identity';

const OWN = '1'.repeat(64);
const OTHER = '2'.repeat(64);
const SECRET = 'opaque-fixture-credential';
const servers: NetServer[] = [];
const sockets = new Set<Socket>();
const directories: string[] = [];

async function fixture(
  handler: RequestListener,
  unix = false
): Promise<{ server: Server; broker: string }> {
  const server = createServer(handler);
  servers.push(server);
  server.on('connection', socket => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });
  if (unix) {
    const directory = await mkdtemp('/tmp/node-creds-');
    directories.push(directory);
    const path = join(directory, 'sock');
    server.listen(path);
    await once(server, 'listening');
    return { server, broker: `unix://${path}` };
  }
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    server,
    broker: `http://127.0.0.1:${(server.address() as AddressInfo).port}/v1/credentials`,
  };
}

async function rawFixture(
  reply: string,
  truncate = false
): Promise<{ server: NetServer; broker: string }> {
  const server = createNetServer(socket => {
    sockets.add(socket);
    socket.on('error', () => {});
    socket.on('close', () => sockets.delete(socket));
    // A raw upgraded peer can retain its writable half after client EOF.
    socket.on('end', () => socket.end());
    socket.once('data', () => {
      socket.write(reply);
      if (truncate) socket.end();
    });
  });
  servers.push(server);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    server,
    broker: `http://127.0.0.1:${(server.address() as AddressInfo).port}/v1/credentials`,
  };
}

async function rejectsAndClosesPeer(
  reply: string,
  code: string,
  truncate = false
): Promise<void> {
  const { server, broker } = await rawFixture(reply, truncate);
  const connected = once(server, 'connection');
  const result = expect(container(broker).tokenFor()).rejects.toMatchObject({
    code,
  });
  const [peer] = (await connected) as [Socket];
  const closed = once(peer, 'close');
  await result;
  // Observe closure before afterEach can destroy a retained socket.
  await closed;
  expect(peer.destroyed).toBe(true);
}

afterEach(async () => {
  for (const socket of sockets) socket.destroy();
  await Promise.all(
    servers.splice(0).map(
      server =>
        new Promise<void>((resolve, reject) => {
          server.close(error => (error ? reject(error) : resolve()));
        })
    )
  );
  await Promise.all(
    directories
      .splice(0)
      .map(path => rm(path, { recursive: true, force: true }))
  );
});

function container(broker: string): Container {
  return new Container({
    databaseIdentity: OWN,
    serverUri: 'https://127.0.0.1:1/',
    credentialBroker: broker,
  });
}

function success(
  response: ServerResponse,
  value: unknown = {
    token: SECRET,
    expires_unix_seconds: Math.floor(Date.now() / 1000) + 10,
  }
): void {
  const body = JSON.stringify(value);
  response.writeHead(200, {
    'Content-Type': 'application/json',
    'Cache-Control': 'no-store',
    'Content-Length': Buffer.byteLength(body),
  });
  response.end(body);
}

describe('Node container credential helper', () => {
  it('uses actual loopback HTTP framing, fresh requests and exact target identities', async () => {
    const requests: { target: string; headers: object; url?: string }[] = [];
    const { broker } = await fixture((request, response) => {
      let body = '';
      request.setEncoding('utf8');
      request.on('data', chunk => {
        body += chunk;
      });
      request.on('end', () => {
        requests.push({
          target: body,
          headers: request.headers,
          url: request.url,
        });
        success(response);
      });
    });
    const discovered = container(broker);
    expect(discovered.databaseIdentity.toHexString()).toBe(OWN);
    const mutableCopy = discovered.databaseIdentity;
    mutableCopy.__identity__ = 0n;
    const first = await discovered.tokenFor();
    const second = await discovered.tokenFor(new Identity(OTHER));
    expect(requests.map(r => r.target)).toEqual([
      JSON.stringify({ target_database: OWN }),
      JSON.stringify({ target_database: OTHER }),
    ]);
    for (const request of requests) {
      expect(request.url).toBe('/v1/credentials');
      expect(request.headers).toMatchObject({
        'content-type': 'application/json',
        connection: 'close',
      });
      expect(request.headers).not.toHaveProperty('authorization');
    }
    expect(first.target.toHexString()).toBe(OWN);
    expect(second.target.toHexString()).toBe(OTHER);
    expect(first.value).toBe(SECRET);
    expect(first.remainingLifetimeMs).toBeGreaterThan(0);
    expect(first.remainingLifetimeMs).toBeLessThanOrEqual(10_000);
    expect(inspect(first)).not.toContain(SECRET);
    expect(JSON.stringify(first)).not.toContain(SECRET);
  });

  it('uses only the owned Unix socket and required logical HTTP authority', async () => {
    const hosts: (string | undefined)[] = [];
    const { broker } = await fixture((request, response) => {
      hosts.push(request.headers.host);
      request.resume();
      request.on('end', () => success(response));
    }, true);
    const token = await container(broker).tokenFor();
    expect(token.value).toBe(SECRET);
    expect(hosts).toEqual(['127.0.0.1:18081']);
  });

  it('rejects invalid discovery without connecting or choosing defaults', () => {
    const invalid = [
      'http://localhost:18081/v1/credentials',
      'http://127.1:18081/v1/credentials',
      'http://0x7f000001:18081/v1/credentials',
      'http://192.0.2.1:18081/v1/credentials',
      'http://user:secret@127.0.0.1:18081/v1/credentials',
      'http://127.0.0.1:18081/v1/credentials?',
      'http://127.0.0.1:18081/v1/credentials#',
      'http://127.0.0.1:0/v1/credentials',
      'https://127.0.0.1:18081/v1/credentials',
      'unix://elsewhere/run/socket',
      'unix:///run/../socket',
      'unix:///run/%73ocket',
      `unix:///${'a'.repeat(104)}`,
    ];
    for (const broker of invalid) {
      expect(() => container(broker)).toThrow(ContainerCredentialError);
    }
    expect(
      () =>
        new Container({
          databaseIdentity: OWN,
          serverUri: `https://127.0.0.1/${'é'.repeat(2100)}`,
          credentialBroker: 'unix:///run/credentials.sock',
        })
    ).toThrow('invalid_discovery');
    expect(() => Container.fromEnvironment({})).toThrow('missing_environment');
    const discovered = Container.fromEnvironment({
      SPACETIMEDB_DATABASE_IDENTITY: OWN,
      SPACETIMEDB_SERVER_URI: 'https://127.0.0.1:1/',
      SPACETIMEDB_CREDENTIAL_BROKER: 'unix:///run/spacetimedb/credentials.sock',
    });
    expect(discovered.databaseIdentity.toHexString()).toBe(OWN);
    expect(inspect(discovered)).not.toContain('/run/spacetimedb');
  });

  it('does not follow redirects or retry denial, and preserves a later valid request', async () => {
    let count = 0;
    const { broker } = await fixture((request, response) => {
      request.resume();
      const status = [302, 403, 503, 200][count++];
      if (status === 200) success(response);
      else
        response
          .writeHead(status, {
            Location: 'http://192.0.2.1/secret',
            'Content-Length': 0,
          })
          .end();
    });
    const discovered = container(broker);
    for (const code of ['invalid_response', 'denied', 'unavailable']) {
      await expect(discovered.tokenFor()).rejects.toMatchObject({ code });
    }
    expect((await discovered.tokenFor()).value).toBe(SECRET);
    expect(count).toBe(4);
  });

  it('rejects missing no-store, oversized bodies, wrong fields and invalid expiry without exposing tokens', async () => {
    const values: unknown[] = [
      {
        token: SECRET,
        expires_unix_seconds: Math.floor(Date.now() / 1000) - 1,
      },
      {
        token: SECRET,
        expires_unix_seconds: Math.floor(Date.now() / 1000) + 60,
      },
      { token: SECRET, expires_unix_seconds: '123' },
      { token: SECRET, expires_unix_seconds: 1, extra: true },
      {
        token: '\nsecret',
        expires_unix_seconds: Math.floor(Date.now() / 1000) + 10,
      },
    ];
    let count = 0;
    const { broker } = await fixture((request, response) => {
      request.resume();
      if (count === 0) {
        response
          .writeHead(200, {
            'Content-Type': 'application/json',
            'Content-Length': 2,
          })
          .end('{}');
      } else if (count === 1) {
        response
          .writeHead(200, {
            'Content-Type': 'application/json',
            'Cache-Control': 'no-store',
            'Content-Length': 100000,
          })
          .end();
      } else success(response, values[count - 2]);
      count++;
    });
    for (let i = 0; i < values.length + 2; i++) {
      const error = await container(broker)
        .tokenFor()
        .catch(error => error as unknown);
      expect(error).toMatchObject({ code: 'invalid_response' });
      expect(inspect(error)).not.toContain(SECRET);
    }
  });

  it('aborts a held response and observes peer closure before fixture cleanup', async () => {
    const { server, broker } = await fixture(request => {
      request.resume();
    });
    const connected = once(server, 'connection');
    const abort = new AbortController();
    const result = container(broker).tokenFor(undefined, abort.signal);
    const [peer] = (await connected) as [Socket];
    const closed = once(peer, 'close');
    abort.abort();
    await expect(result).rejects.toMatchObject({ code: 'aborted' });
    await closed;
    expect(peer.destroyed).toBe(true);
    await expect(
      container(broker).tokenFor(undefined, abort.signal)
    ).rejects.toMatchObject({ code: 'aborted' });
  });

  it('bounds a stalled response by five seconds and destroys its actual socket', async () => {
    const { server, broker } = await fixture(request => {
      request.resume();
    });
    const connected = once(server, 'connection');
    const result = container(broker).tokenFor();
    const [peer] = (await connected) as [Socket];
    const closed = once(peer, 'close');
    await expect(result).rejects.toMatchObject({ code: 'timeout' });
    await closed;
    expect(peer.destroyed).toBe(true);
  }, 10_000);

  it('rejects an invalid exact target before opening a broker connection', async () => {
    let accepted = 0;
    const { server, broker } = await fixture(request => request.resume());
    server.on('connection', () => accepted++);
    const target = new Identity(OTHER);
    target.toHexString = () => 'not-an-identity';
    await expect(container(broker).tokenFor(target)).rejects.toMatchObject({
      code: 'invalid_target',
    });
    expect(accepted).toBe(0);
  });

  it('closes a request cancelled before its socket is assigned', async () => {
    const { broker } = await fixture(request => request.resume());
    const abort = new AbortController();
    const result = container(broker).tokenFor(undefined, abort.signal);
    abort.abort();
    await expect(result).rejects.toMatchObject({ code: 'aborted' });
  });

  it('rejects duplicate response headers and closes the actual peer', async () => {
    const body = JSON.stringify({
      token: SECRET,
      expires_unix_seconds: Math.floor(Date.now() / 1000) + 20,
    });
    await rejectsAndClosesPeer(
      `HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`,
      'invalid_response'
    );
  });

  it('rejects a truncated response and closes the actual peer', async () => {
    await rejectsAndClosesPeer(
      'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: 100\r\n\r\n{}',
      'transport',
      true
    );
  });

  it('rejects an upgrade while retaining ownership through actual socket close', async () => {
    await rejectsAndClosesPeer(
      'HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: arbitrary\r\n\r\n',
      'invalid_response'
    );
  });
});
