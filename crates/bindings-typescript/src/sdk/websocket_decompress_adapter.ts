import { decompress } from './decompress';
import { openWebSocket, type WebSocketAdapter, type WebSocketArgs } from './ws';

export class WebsocketDecompressAdapter implements WebSocketAdapter {
  get protocol(): string {
    return this.#ws.protocol;
  }
  set onclose(handler: (ev: CloseEvent) => void) {
    this.#ws.onclose = handler;
  }
  set onopen(handler: () => void) {
    this.#ws.onopen = handler;
  }
  set onmessage(handler: (msg: { data: Uint8Array }) => void) {
    this.#ws.onmessage = async (msg: MessageEvent<ArrayBuffer>) => {
      const data = await this.#decompress(new Uint8Array(msg.data));
      handler({ data });
    };
  }
  set onerror(handler: (msg: ErrorEvent) => void) {
    this.#ws.onerror = handler as (msg: Event) => void;
  }

  #ws: WebSocket;

  async #decompress(buffer: Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
    const tag = buffer[0];
    const data = buffer.subarray(1);
    switch (tag) {
      case 0:
        return data;
      case 1:
        // Some runtimes support brotli, but it's not yet defined in `lib.dom.d.ts`.
        // We assert runtime support in `DbConnectionBuilder.withCompression`, so
        // this cast is safe.
        return await decompress(data, 'brotli' as CompressionFormat);
      case 2:
        return await decompress(data, 'gzip');
      default:
        throw new Error(
          'Unexpected Compression Algorithm. Please use `gzip` or `none`'
        );
    }
  }

  send(msg: Uint8Array<ArrayBuffer>): void {
    this.#ws.send(msg);
  }

  close(): void {
    this.#ws.close();
  }

  constructor(ws: WebSocket) {
    this.#ws = ws;
  }

  static async openWebSocket(
    args: WebSocketArgs
  ): Promise<WebsocketDecompressAdapter> {
    return new this(await openWebSocket(args));
  }
}
