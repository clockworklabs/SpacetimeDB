import { BinaryReader, BinaryWriter } from '../';
import { ClientMessage, ServerMessage } from './client_api/types';
import type { WebsocketAdapter } from './websocket_decompress_adapter';

class WebsocketTestAdapter implements WebsocketAdapter {
  onclose: any;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  onopen!: () => void;
  onmessage: any;
  onerror: any;

  messageQueue: any[];
  outgoingMessages: ClientMessage[];
  closed: boolean;

  constructor() {
    this.messageQueue = [];
    this.outgoingMessages = [];
    this.closed = false;
  }

  send(message: any): void {
    const parsedMessage = ClientMessage.deserialize(new BinaryReader(message));
    this.outgoingMessages.push(parsedMessage);
    // console.ClientMessageSerde.deserialize(message);
    this.messageQueue.push(message);
  }

  close(): void {
    this.closed = true;
    this.onclose?.({ code: 1000, reason: 'normal closure', wasClean: true });
  }

  acceptConnection(): void {
    this.onopen();
  }

  sendToClient(message: ServerMessage): void {
    const writer = new BinaryWriter(1024);
    ServerMessage.serialize(writer, message);
    const rawBytes = writer.getBuffer();
    // The brotli library's `compress` is somehow broken: it returns `null` for some inputs.
    // See https://github.com/foliojs/brotli.js/issues/36, which is closed but not actually fixed.
    // So we send the uncompressed data here, and in `spacetimedb.ts`,
    // if compression fails, we treat the raw message as having been uncompressed all along.
    // const data = compress(rawBytes);
    this.onmessage({ data: rawBytes });
  }

  async createWebSocketFn(_args: {
    url: URL;
    wsProtocol: string;
    nameOrAddress: string;
    authToken?: string;
    compression: 'gzip' | 'none';
    lightMode: boolean;
    confirmedReads?: boolean;
  }): Promise<WebsocketTestAdapter> {
    return this;
  }
}

export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
