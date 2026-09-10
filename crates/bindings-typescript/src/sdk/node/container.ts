import { request, type ClientRequest, type IncomingMessage } from 'node:http';
import { isIP, type Socket } from 'node:net';
import { performance } from 'node:perf_hooks';
import { inspect } from 'node:util';
import { Identity } from '../../lib/identity';

const REQUEST_MS = 5_000;
const MAX_BODY = 8192 + 256;
const MAX_HEADERS = 4096;
const LOCAL_AUTHORITY = '127.0.0.1:18081';
const CREDENTIAL_PATH = '/v1/credentials';

export type ContainerCredentialErrorCode =
  | 'missing_environment'
  | 'invalid_discovery'
  | 'invalid_target'
  | 'denied'
  | 'unavailable'
  | 'transport'
  | 'timeout'
  | 'aborted'
  | 'invalid_response';

/** Errors contain no discovery values, response bodies or credentials. */
export class ContainerCredentialError extends Error {
  constructor(readonly code: ContainerCredentialErrorCode) {
    super(`Container credential request failed: ${code}`);
    this.name = 'ContainerCredentialError';
  }
}

type Endpoint =
  | { socketPath: string }
  | { hostname: string; port: number; host: string };

/** Discovery values injected by the container platform; these are not secrets. */
export interface ContainerDiscovery {
  databaseIdentity: string;
  serverUri: string;
  credentialBroker: string;
}

/**
 * Explicit discovery and short-lived credentials for a Node.js process.
 * No stored CLI credentials, default server, anonymous Identity, redirects or
 * proxy configuration are used. Construction performs no I/O.
 */
export class Container {
  #identity: string;
  #server: string;
  #endpoint: Endpoint;

  constructor(discovery: ContainerDiscovery) {
    this.#identity = identity(discovery.databaseIdentity, 'invalid_discovery');
    const server = parseUrl(discovery.serverUri);
    if (!['http:', 'https:', 'ws:', 'wss:'].includes(server.protocol)) {
      throw new ContainerCredentialError('invalid_discovery');
    }
    if (!server.hostname) {
      throw new ContainerCredentialError('invalid_discovery');
    }
    this.#server = server.href;
    this.#endpoint = endpoint(discovery.credentialBroker);
  }

  /** Read only the three platform discovery variables. */
  static fromEnvironment(
    environment: Readonly<Record<string, string | undefined>> = process.env
  ): Container {
    const read = (name: string): string => {
      const value = environment[name];
      if (value === undefined) {
        throw new ContainerCredentialError('missing_environment');
      }
      return value;
    };
    return new Container({
      databaseIdentity: read('SPACETIMEDB_DATABASE_IDENTITY'),
      serverUri: read('SPACETIMEDB_SERVER_URI'),
      credentialBroker: read('SPACETIMEDB_CREDENTIAL_BROKER'),
    });
  }

  get databaseIdentity(): Identity {
    return new Identity(this.#identity);
  }

  get serverUri(): string {
    return this.#server;
  }

  /**
   * Request a fresh token for an exact target Identity, defaulting to this
   * container's database. The broker authenticates the process independently
   * of these discovery values. Send the token only to the target's trusted
   * server. This does not reconnect a database connection or replay calls.
   */
  async tokenFor(
    target: Identity = this.databaseIdentity,
    signal?: AbortSignal
  ): Promise<ContainerToken> {
    let targetHex: string;
    try {
      targetHex = identity(target.toHexString(), 'invalid_target');
    } catch {
      throw new ContainerCredentialError('invalid_target');
    }
    const response = await readToken(this.#endpoint, targetHex, signal);
    return ContainerToken.fromResponse(response, targetHex);
  }

  [inspect.custom](): string {
    return 'Container { discovery: <configured> }';
  }
}

/** An in-memory bearer token. Inspection and JSON serialization are redacted. */
export class ContainerToken {
  #token: string;
  #target: string;
  #expiry: number;
  #deadline: number;

  private constructor(
    token: string,
    target: string,
    expiry: number,
    remaining: number
  ) {
    this.#token = token;
    this.#target = target;
    this.#expiry = expiry;
    this.#deadline = performance.now() + remaining;
  }

  /** @internal */
  static fromResponse(bytes: Buffer, target: string): ContainerToken {
    try {
      const value: unknown = JSON.parse(
        new TextDecoder('utf-8', { fatal: true }).decode(bytes)
      );
      if (!value || typeof value !== 'object' || Array.isArray(value))
        throw new Error();
      const fields = value as Record<string, unknown>;
      const token = fields.token;
      const seconds = fields.expires_unix_seconds;
      if (
        Object.keys(fields).length !== 2 ||
        typeof token !== 'string' ||
        !/^[\x21-\x7e]{1,8192}$/.test(token) ||
        typeof seconds !== 'number' ||
        !Number.isSafeInteger(seconds) ||
        !Number.isSafeInteger(seconds * 1000)
      )
        throw new Error();
      const expiry = seconds * 1000;
      const remaining = expiry - Date.now();
      if (remaining <= 0 || remaining > 30_000) throw new Error();
      return new ContainerToken(token, target, expiry, remaining);
    } catch {
      throw new ContainerCredentialError('invalid_response');
    }
  }

  /** Do not log, persist, or place this value in a URL or environment variable. */
  get value(): string {
    return this.#token;
  }

  get target(): Identity {
    return new Identity(this.#target);
  }

  get expiresAt(): Date {
    return new Date(this.#expiry);
  }

  /** A monotonic cap prevents a backward wall-clock jump from extending it. */
  get remainingLifetimeMs(): number {
    return Math.max(
      0,
      Math.min(this.#deadline - performance.now(), this.#expiry - Date.now())
    );
  }

  toJSON(): object {
    return {
      target: this.#target,
      expiresAt: this.expiresAt,
      token: '<redacted>',
    };
  }

  [inspect.custom](): object {
    return this.toJSON();
  }
}

function identity(value: string, code: ContainerCredentialErrorCode): string {
  if (typeof value !== 'string' || !/^[0-9a-fA-F]{64}$/.test(value)) {
    throw new ContainerCredentialError(code);
  }
  return value.toLowerCase();
}

function parseUrl(value: string): URL {
  try {
    if (
      !value ||
      Buffer.byteLength(value) > 4096 ||
      [...value].some(c => c.charCodeAt(0) <= 32 || c.charCodeAt(0) === 127)
    )
      throw new Error();
    const url = new URL(value);
    if (
      url.username ||
      url.password ||
      value.includes('?') ||
      value.includes('#')
    )
      throw new Error();
    return url;
  } catch {
    throw new ContainerCredentialError('invalid_discovery');
  }
}

function endpoint(value: string): Endpoint {
  const url = parseUrl(value);
  if (url.protocol === 'unix:') {
    const path = value.slice('unix://'.length);
    if (
      !value.startsWith('unix:///') ||
      url.host ||
      !path.startsWith('/') ||
      Buffer.byteLength(path) > 103 ||
      path.includes('%') ||
      path.includes('\\') ||
      path.split('/').some(part => part === '.' || part === '..')
    )
      throw new ContainerCredentialError('invalid_discovery');
    return { socketPath: path };
  }
  // Validate the original authority before URL normalization can turn a DNS
  // spelling or an abbreviated/hexadecimal IPv4 address into loopback.
  const match =
    /^http:\/\/(\[[0-9a-fA-F:]+\]|[0-9.]+)(?::([0-9]+))?\/v1\/credentials$/.exec(
      value
    );
  if (!match) throw new ContainerCredentialError('invalid_discovery');
  const hostname = match[1].replace(/^\[|\]$/g, '');
  const version = isIP(hostname);
  if (
    (version !== 4 || hostname.split('.')[0] !== '127') &&
    (version !== 6 || new URL(`http://[${hostname}]/`).hostname !== '[::1]')
  )
    throw new ContainerCredentialError('invalid_discovery');
  const port = match[2] === undefined ? 80 : Number(match[2]);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new ContainerCredentialError('invalid_discovery');
  }
  return { hostname, port, host: url.host };
}

function responseLength(response: IncomingMessage): number {
  if (response.statusCode === 401 || response.statusCode === 403) {
    throw new ContainerCredentialError('denied');
  }
  if (response.statusCode === 503)
    throw new ContainerCredentialError('unavailable');
  if (response.statusCode !== 200)
    throw new ContainerCredentialError('invalid_response');
  const count = (name: string): number =>
    response.rawHeaders.filter(
      (v, i) => i % 2 === 0 && v.toLowerCase() === name
    ).length;
  const headers = response.headers;
  if (
    count('content-type') !== 1 ||
    headers['content-type'] !== 'application/json' ||
    count('cache-control') !== 1 ||
    headers['cache-control'] !== 'no-store' ||
    count('content-length') !== 1 ||
    count('transfer-encoding') ||
    count('content-encoding') ||
    typeof headers['content-length'] !== 'string' ||
    !/^[0-9]+$/.test(headers['content-length'])
  )
    throw new ContainerCredentialError('invalid_response');
  const length = Number(headers['content-length']);
  if (!Number.isInteger(length) || length <= 0 || length > MAX_BODY) {
    throw new ContainerCredentialError('invalid_response');
  }
  return length;
}

/** Own the request and its socket through terminal close, on every outcome. */
async function readToken(
  endpoint: Endpoint,
  target: string,
  signal?: AbortSignal
): Promise<Buffer> {
  if (signal?.aborted) throw new ContainerCredentialError('aborted');
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ target_database: target });
    let req: ClientRequest | undefined;
    let socket: Socket | undefined;
    let requestClosed = false;
    let socketClosed = false;
    let outcome:
      | { error?: ContainerCredentialError; bytes?: Buffer }
      | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const complete = (): void => {
      if (!outcome || !requestClosed || (socket && !socketClosed)) return;
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
      if (outcome.error) reject(outcome.error);
      else resolve(outcome.bytes!);
    };
    const finish = (error?: ContainerCredentialError, bytes?: Buffer): void => {
      if (!outcome) outcome = { error, bytes };
      req?.destroy();
      socket?.destroy();
      complete();
    };
    const fail = (code: ContainerCredentialErrorCode): void =>
      finish(new ContainerCredentialError(code));
    const abort = (): void => fail('aborted');
    try {
      req = request({
        ...endpoint,
        // A fresh private agent owns one connection; ambient proxy and HTTP
        // pool settings cannot redirect this credential request.
        agent: false,
        method: 'POST',
        path: CREDENTIAL_PATH,
        maxHeaderSize: MAX_HEADERS,
        headers: {
          Host: 'socketPath' in endpoint ? LOCAL_AUTHORITY : endpoint.host,
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(body),
          Connection: 'close',
        },
      });
      req.once('socket', owned => {
        socket = owned;
        owned.once('close', () => {
          socketClosed = true;
          complete();
        });
        if (outcome) owned.destroy();
      });
      req.once('error', () => fail('transport'));
      req.once('close', () => {
        requestClosed = true;
        if (!outcome) fail('transport');
        complete();
      });
      req.once('response', response => {
        response.on('error', () => fail('transport'));
        let length: number;
        try {
          length = responseLength(response);
        } catch (error) {
          finish(
            error instanceof ContainerCredentialError
              ? error
              : new ContainerCredentialError('invalid_response')
          );
          return;
        }
        const chunks: Buffer[] = [];
        let received = 0;
        response.on('data', (chunk: Buffer) => {
          received += chunk.length;
          if (received > length) {
            fail('invalid_response');
            return;
          }
          chunks.push(chunk);
        });
        response.once('aborted', () => fail('transport'));
        response.once('end', () => {
          if (received !== length) fail('invalid_response');
          else finish(undefined, Buffer.concat(chunks, received));
        });
      });
      // Upgrades never become an untracked or credential-bearing connection.
      req.once('upgrade', (_response, upgraded) => {
        upgraded.destroy();
        fail('invalid_response');
      });
      timer = setTimeout(() => fail('timeout'), REQUEST_MS);
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) abort();
      else req.end(body);
    } catch {
      if (req) fail('transport');
      else {
        clearTimeout(timer);
        signal?.removeEventListener('abort', abort);
        reject(new ContainerCredentialError('transport'));
      }
    }
  });
}
