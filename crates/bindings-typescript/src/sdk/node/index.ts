import { WebSocket } from 'ws';
import { WebsocketDecompressAdapter } from '../websocket_decompress_adapter';
import type { WebSocketArgs } from '../ws';

const IO_TIMEOUT_MS = 5_000;

/**
 * A Node.js socket with an awaited transport shutdown.
 * Closure does not establish whether an unconfirmed reducer or procedure
 * committed. Callers must never automatically replay those requests.
 */
export class NodeWebSocketAdapter extends WebsocketDecompressAdapter {
  #socket: WebSocket;
  #timer?: ReturnType<typeof setTimeout>;
  #completion: Promise<void>;
  #closing = false;

  constructor(socket: WebSocket) {
    super(socket as unknown as globalThis.WebSocket);
    this.#socket = socket;
    this.#completion =
      socket.readyState === WebSocket.CLOSED
        ? Promise.resolve()
        : new Promise(resolve => {
            socket.once('close', () => {
              clearTimeout(this.#timer);
              resolve();
            });
          });
    // An early failed handshake must be handled even before the SDK installs
    // its error callback. The callback below receives only a fixed diagnostic.
    socket.on('error', () => {});
  }

  static override async openWebSocket(
    args: WebSocketArgs
  ): Promise<NodeWebSocketAdapter> {
    return openNodeWebSocket(args);
  }

  override set onerror(handler: (event: globalThis.ErrorEvent) => void) {
    this.#socket.onerror = () =>
      handler(
        Object.assign(new Event('error'), {
          message: 'Database WebSocket transport failed',
          filename: '',
          lineno: 0,
          colno: 0,
          error: undefined,
        })
      );
  }

  override close(): void {
    if (this.#closing || this.#socket.readyState === WebSocket.CLOSED) return;
    this.#closing = true;
    if (this.#socket.readyState === WebSocket.CONNECTING) {
      this.#socket.terminate();
      return;
    }
    // terminate() owns the upgraded socket, unlike destroying an HTTP pool
    // that has already transferred socket ownership to WebSocket.
    this.#timer = setTimeout(() => this.#socket.terminate(), IO_TIMEOUT_MS);
    this.#socket.close();
  }

  /** Close the WebSocket and await its actual terminal close event. */
  async shutdown(): Promise<void> {
    this.close();
    await this.#completion;
  }
}

/**
 * Node.js WebSocket factory for `DbConnection.builder().withWSFn(...)`.
 * Sends the supplied token in Authorization without exchanging it for a
 * generic WebSocket token or placing it in the URL. Install the optional
 * `ws` peer dependency before importing `spacetimedb/sdk/node`.
 *
 * This transports a token supplied by the caller. It does not fetch or renew
 * container credentials. An expired connection must obtain fresh credentials
 * and create a new database connection and subscriptions.
 */
export async function openNodeWebSocket(
  args: WebSocketArgs
): Promise<NodeWebSocketAdapter> {
  const { url, nameOrAddress, authToken } = args;
  const query = [...url.searchParams];
  const connectionId = url.searchParams.get('connection_id');
  if (
    !['ws:', 'wss:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    (query.length !== 0 &&
      (query.length !== 1 ||
        query[0][0] !== 'connection_id' ||
        !/^[0-9a-f]{32}$/.test(connectionId ?? ''))) ||
    url.hash ||
    !nameOrAddress ||
    nameOrAddress === '.' ||
    nameOrAddress === '..' ||
    nameOrAddress.length > 256 ||
    [...nameOrAddress].some(character => {
      const code = character.charCodeAt(0);
      return code < 32 || code === 127;
    }) ||
    (authToken !== undefined &&
      (!authToken ||
        authToken.length > 8192 ||
        !/^[\x21-\x7e]+$/.test(authToken)))
  ) {
    throw new Error('Invalid Node.js database WebSocket configuration');
  }
  const target = new URL(
    `v1/database/${encodeURIComponent(nameOrAddress)}/subscribe`,
    url
  );
  if (connectionId !== null) {
    target.searchParams.set('connection_id', connectionId);
  }
  target.searchParams.set(
    'compression',
    { gzip: 'Gzip', brotli: 'Brotli', none: 'None' }[args.compression]
  );
  if (args.lightMode) target.searchParams.set('light', 'true');
  if (args.confirmedReads !== undefined) {
    target.searchParams.set('confirmed', String(args.confirmedReads));
  }
  try {
    const socket = new WebSocket(target, args.wsProtocol, {
      // A private one-use HTTP agent avoids inherited global proxy settings.
      // The WebSocket itself retains its upgraded socket for terminate().
      agent: false,
      followRedirects: false,
      // Never inherit NODE_TLS_REJECT_UNAUTHORIZED=0 for bearer credentials.
      rejectUnauthorized: true,
      handshakeTimeout: IO_TIMEOUT_MS,
      perMessageDeflate: false,
      headers:
        authToken === undefined ? {} : { Authorization: `Bearer ${authToken}` },
    });
    socket.binaryType = 'arraybuffer';
    return new NodeWebSocketAdapter(socket);
  } catch {
    throw new Error('Database WebSocket transport could not start');
  }
}

export {
  Container,
  ContainerToken,
  ContainerCredentialError,
} from './container';
export type {
  ContainerDiscovery,
  ContainerCredentialErrorCode,
} from './container';
