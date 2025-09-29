import { decompress } from './decompress';
import { resolveWS } from './ws';

export class WebsocketDecompressAdapter {
  onclose?: (...ev: any[]) => void;
  onopen?: (...ev: any[]) => void;
  onmessage?: (msg: { data: Uint8Array }) => void;
  onerror?: (msg: ErrorEvent) => void;

  #ws: WebSocket;

  async #handleOnMessage(msg: MessageEvent) {
    const buffer = new Uint8Array(msg.data);
    let decompressed: Uint8Array;

    if (buffer[0] === 0) {
      decompressed = buffer.slice(1);
    } else if (buffer[0] === 1) {
      throw new Error(
        'Brotli Compression not supported. Please use gzip or none compression in withCompression method on DbConnection.'
      );
    } else if (buffer[0] === 2) {
      decompressed = await decompress(buffer.slice(1), 'gzip');
    } else {
      throw new Error(
        'Unexpected Compression Algorithm. Please use `gzip` or `none`'
      );
    }

    this.onmessage?.({ data: decompressed });
  }

  #handleOnOpen(msg: any) {
    this.onopen?.(msg);
  }

  #handleOnError(msg: any) {
    this.onerror?.(msg);
  }

  #handleOnClose(msg: any) {
    this.onclose?.(msg);
  }

  send(msg: any): void {
    this.#ws.send(msg);
  }

  close(): void {
    this.#ws.close();
  }

  constructor(ws: WebSocket) {
    this.onmessage = undefined;
    this.onopen = undefined;
    this.onmessage = undefined;
    this.onerror = undefined;

    ws.onmessage = this.#handleOnMessage.bind(this);
    ws.onerror = this.#handleOnError.bind(this);
    ws.onclose = this.#handleOnClose.bind(this);
    ws.onopen = this.#handleOnOpen.bind(this);

    ws.binaryType = 'arraybuffer';

    this.#ws = ws;
  }

  static async createWebSocketFn({
    url,
    nameOrAddress,
    wsProtocol,
    authToken,
    compression,
    lightMode,
    confirmedReads,
  }: {
    url: URL;
    wsProtocol: string;
    nameOrAddress: string;
    authToken?: string;
    compression: 'gzip' | 'none';
    lightMode: boolean;
    confirmedReads?: boolean;
  }): Promise<WebsocketDecompressAdapter> {
    const headers = new Headers();

    const WS = await resolveWS();

    // We swap our original token to a shorter-lived token
    // to avoid sending the original via query params.
    let temporaryAuthToken: string | undefined = undefined;
    if (authToken) {
      headers.set('Authorization', `Bearer ${authToken}`);
      const tokenUrl = new URL('v1/identity/websocket-token', url);
      tokenUrl.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';

      const response = await fetch(tokenUrl, { method: 'POST', headers });
      if (response.ok) {
        const { token } = await response.json();
        temporaryAuthToken = token;
      } else {
        return Promise.reject(
          new Error(`Failed to verify token: ${response.statusText}`)
        );
      }
    }

    const databaseUrl = new URL(`v1/database/${nameOrAddress}/subscribe`, url);
    if (temporaryAuthToken) {
      databaseUrl.searchParams.set('token', temporaryAuthToken);
    }
    databaseUrl.searchParams.set(
      'compression',
      compression === 'gzip' ? 'Gzip' : 'None'
    );
    if (lightMode) {
      databaseUrl.searchParams.set('light', 'true');
    }
    if (confirmedReads !== undefined) {
      databaseUrl.searchParams.set('confirmed', confirmedReads.toString());
    }

    const ws = new WS(databaseUrl.toString(), wsProtocol);

    return new WebsocketDecompressAdapter(ws);
  }
}
