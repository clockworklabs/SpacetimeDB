import { BinarySerializer } from "./serializer";
import { ServerMessage } from "./client_api";
import type { CreateWSFnType } from "./spacetimedb";

class WebsocketTestAdapter {
  public onclose: any;
  public onopen!: Function;
  public onmessage: any;
  public onerror: any;

  public messageQueue: any[];
  public closed: boolean;

  constructor() {
    this.messageQueue = [];
    this.closed = false;
  }

  public send(message: any) {
    this.messageQueue.push(message);
  }

  public close() {
    this.closed = true;
  }

  public acceptConnection() {
    this.onopen();
  }

  public sendToClient(message: ServerMessage) {
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

  public async createWebSocketFn(
    _url,
    _protocol,
    _params
  ): Promise<WebsocketTestAdapter> {
    return this;
  }
}

export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
