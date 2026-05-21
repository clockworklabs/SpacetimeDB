import { stdbLogger } from './logger';

async function resolveWS(): Promise<typeof WebSocket> {
  // Browser or Node >= 22 (or any env that exposes global WebSocket)
  if (typeof WebSocket !== 'undefined') {
    return WebSocket;
  }

  // Node without a global WebSocket: lazily load undici's polyfill.
  // Use an unstatable dynamic import so bundlers don't prebundle it.
  const dynamicImport = new Function('m', 'return import(m)') as (
    m: string
  ) => Promise<any>;

  try {
    const { WebSocket: UndiciWS } = await dynamicImport('undici');
    return UndiciWS as unknown as typeof WebSocket;
  } catch (err) {
    stdbLogger(
      'warn',
      '[spacetimedb-sdk] No global WebSocket found. ' +
        'On Node 18–21, please install `undici` (npm install undici) ' +
        'to enable WebSocket support.'
    );
    throw err;
  }
}

export interface WebSocketAdapter {
  readonly protocol: string;
  send(msg: Uint8Array<ArrayBuffer>): void;
  close(): void;

  set onclose(handler: (ev: CloseEvent) => void);
  set onopen(handler: () => void);
  set onmessage(handler: (msg: { data: Uint8Array }) => void);
  set onerror(handler: (msg: ErrorEvent) => void);
}

export interface WebSocketArgs {
  url: URL;
  wsProtocol: string[];
  nameOrAddress: string;
  authToken?: string;
  compression: 'gzip' | 'brotli' | 'none';
  lightMode: boolean;
  confirmedReads?: boolean;
}
export type WebSocketFactory = (
  args: WebSocketArgs
) => Promise<WebSocketAdapter>;

/**
 * Open a WebSocket to the database specified by the given `WebSocketArgs`.
 * @returns a WebSocket with `binaryType` set to `arraybuffer`.
 */
export async function openWebSocket({
  url,
  nameOrAddress,
  wsProtocol,
  authToken,
  compression,
  lightMode,
  confirmedReads,
}: WebSocketArgs): Promise<WebSocket> {
  const headers = new Headers();

  const WS = await resolveWS();

  // We swap our original token to a shorter-lived token
  // to avoid sending the original via query params.
  let temporaryAuthToken: string | undefined;
  if (authToken) {
    headers.set('Authorization', `Bearer ${authToken}`);
    const tokenUrl = new URL('v1/identity/websocket-token', url);
    tokenUrl.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';

    const response = await fetch(tokenUrl, { method: 'POST', headers });
    if (response.ok) {
      const { token } = await response.json();
      temporaryAuthToken = token;
    } else {
      throw new Error(`Failed to verify token: ${response.statusText}`);
    }
  }

  const databaseUrl = new URL(`v1/database/${nameOrAddress}/subscribe`, url);
  if (temporaryAuthToken) {
    databaseUrl.searchParams.set('token', temporaryAuthToken);
  }
  databaseUrl.searchParams.set(
    'compression',
    { gzip: 'Gzip', brotli: 'Brotli', none: 'None' }[compression] ?? 'None'
  );
  if (lightMode) {
    databaseUrl.searchParams.set('light', 'true');
  }
  if (confirmedReads !== undefined) {
    databaseUrl.searchParams.set('confirmed', confirmedReads.toString());
  }

  const ws = new WS(databaseUrl.toString(), wsProtocol);
  ws.binaryType = 'arraybuffer';
  return ws;
}
