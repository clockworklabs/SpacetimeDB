import BinaryReader from '../lib/binary_reader.ts';
import BinaryWriter from '../lib/binary_writer.ts';
import { ClientMessage, ServerMessage } from './client_api/types';
import type { WebsocketAdapter } from './websocket_decompress_adapter';
import { PREFERRED_WS_PROTOCOLS, V3_WS_PROTOCOL } from './websocket_protocols';
import {
  decodeClientMessagesV3,
  encodeServerMessagesV3,
} from './websocket_v3_frames.ts';

class WebsocketTestAdapter implements WebsocketAdapter {
  protocol: string = '';

  messageQueue: Uint8Array[];
  outgoingMessages: ClientMessage[];
  closed: boolean;
  supportedProtocols: string[];

  #onclose: (ev: CloseEvent) => void = () => {};
  #onopen: () => void = () => {};
  #onmessage: (msg: { data: Uint8Array }) => void = () => {};

  constructor() {
    this.messageQueue = [];
    this.outgoingMessages = [];
    this.closed = false;
    this.supportedProtocols = [...PREFERRED_WS_PROTOCOLS];
  }

  set onclose(handler: (ev: CloseEvent) => void) {
    this.#onclose = handler;
  }

  set onopen(handler: () => void) {
    this.#onopen = handler;
  }

  set onmessage(handler: (msg: { data: Uint8Array }) => void) {
    this.#onmessage = handler;
  }

  set onerror(_handler: (msg: ErrorEvent) => void) {}

  send(message: Uint8Array): void {
    const rawMessage = message.slice();
    const outgoingMessages =
      this.protocol === V3_WS_PROTOCOL
        ? decodeClientMessagesV3(rawMessage)
        : [rawMessage];

    for (const outgoingMessage of outgoingMessages) {
      this.outgoingMessages.push(
        ClientMessage.deserialize(new BinaryReader(outgoingMessage))
      );
    }
    this.messageQueue.push(rawMessage);
  }

  close(): void {
    this.closed = true;
    this.#onclose({
      code: 1000,
      reason: 'normal closure',
      wasClean: true,
    } as CloseEvent);
  }

  acceptConnection(): void {
    this.#onopen();
  }

  sendToClient(message: ServerMessage): void {
    const writer = new BinaryWriter(1024);
    ServerMessage.serialize(writer, message);
    const rawBytes = writer.getBuffer().slice();
    // The brotli library's `compress` is somehow broken: it returns `null` for some inputs.
    // See https://github.com/foliojs/brotli.js/issues/36, which is closed but not actually fixed.
    // So we send the uncompressed data here, and in `spacetimedb.ts`,
    // if compression fails, we treat the raw message as having been uncompressed all along.
    // const data = compress(rawBytes);
    const outboundData =
      this.protocol === V3_WS_PROTOCOL
        ? encodeServerMessagesV3(writer, [rawBytes]).slice()
        : rawBytes;
    this.#onmessage({ data: outboundData });
  }

  async createWebSocketFn(_args: {
    url: URL;
    wsProtocol: string | string[];
    nameOrAddress: string;
    authToken?: string;
    compression: 'gzip' | 'none';
    lightMode: boolean;
    confirmedReads?: boolean;
  }): Promise<WebsocketTestAdapter> {
    const requestedProtocols = Array.isArray(_args.wsProtocol)
      ? _args.wsProtocol
      : [_args.wsProtocol];
    const negotiatedProtocol = requestedProtocols.find(protocol =>
      this.supportedProtocols.includes(protocol)
    );
    if (!negotiatedProtocol) {
      return Promise.reject(new Error('No compatible websocket protocol'));
    }
    this.protocol = negotiatedProtocol;
    return this;
  }
}

export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
