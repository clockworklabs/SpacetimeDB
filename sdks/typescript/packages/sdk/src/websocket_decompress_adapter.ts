import decompress from 'brotli/decompress';
import { Buffer } from 'buffer';

export class WebsocketDecompressAdapter {
  onclose?: (...ev: any[]) => void;
  onopen?: (...ev: any[]) => void;
  onmessage?: (msg: { data: Uint8Array }) => void;
  onerror?: (msg: ErrorEvent) => void;

  #ws: WebSocket;

  #handleOnMessage(msg: MessageEvent) {
    const buffer = new Uint8Array(msg.data);
    let decompressed: Uint8Array;
    switch (buffer[0]) {
      case 0:
        decompressed = msg.data.slice(1);
        break;
      case 1:
        decompressed = decompress(new Buffer(buffer.slice(1)));
        break;
      default:
        throw new Error('Invalid message type');
    }

    this.onmessage?.({ data: decompressed });
  }

  #handleOnOpen(msg: any) {
    this.onopen?.(msg);
  }

  #handleOnError(msg: any) {
    this.onerror?.(msg);
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
    ws.onclose = this.#handleOnError.bind(this);
    ws.onopen = this.#handleOnOpen.bind(this);

    ws.binaryType = 'arraybuffer';

    this.#ws = ws;
  }

  static async createWebSocketFn({
    url,
    wsProtocol,
    authToken,
  }: {
    url: URL;
    wsProtocol: string;
    authToken?: string;
  }): Promise<WebsocketDecompressAdapter> {
    const headers = new Headers();
    if (authToken) {
      headers.set('Authorization', `Basic ${btoa('token:' + authToken)}`);
    }

    let WS: typeof WebSocket;

    // @ts-ignore
    if (import.meta.env.BROWSER === 'false') {
      WS =
        'WebSocket' in globalThis
          ? WebSocket
          : ((await import('undici')).WebSocket as unknown as typeof WebSocket);
    } else {
      WS = WebSocket;
    }

    const tokenUrl = new URL('identity/websocket_token', url);
    tokenUrl.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';

    const response = await fetch(tokenUrl, { method: 'POST', headers });
    if (response.ok) {
      const { token } = await response.json();
      url.searchParams.set('token', btoa('token:' + token));
    }
    const ws = new WS(url, wsProtocol);

    return new WebsocketDecompressAdapter(ws);
  }
}
