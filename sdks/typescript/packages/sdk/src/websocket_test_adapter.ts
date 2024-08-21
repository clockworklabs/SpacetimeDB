import { ServerMessage } from './client_api.ts';
import { BinarySerializer } from './serializer.ts';

class WebsocketTestAdapter {
  onclose: any;
  onopen!: Function;
  onmessage: any;
  onerror: any;

  messageQueue: any[];
  closed: boolean;

  constructor() {
    this.messageQueue = [];
    this.closed = false;
  }

  send(message: any): void {
    this.messageQueue.push(message);
  }

  close(): void {
    this.closed = true;
  }

  acceptConnection(): void {
    this.onopen();
  }

  sendToClient(message: ServerMessage): void {
    const serializer = new BinarySerializer();
    serializer.write(ServerMessage.getAlgebraicType(), message);
    const rawBytes = serializer.args();
    // The brotli library's `compress` is somehow broken: it returns `null` for some inputs.
    // See https://github.com/foliojs/brotli.js/issues/36, which is closed but not actually fixed.
    // So we send the uncompressed data here, and in `spacetimedb.ts`,
    // if compression fails, we treat the raw message as having been uncompressed all along.
    // const data = compress(rawBytes);
    this.onmessage({ data: rawBytes });
  }

  async createWebSocketFn(
    _url: string,
    _protocol: string,
    _params: any
  ): Promise<WebsocketTestAdapter> {
    return this;
  }
}

export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
