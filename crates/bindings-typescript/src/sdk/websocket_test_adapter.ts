import BinaryReader from '../lib/binary_reader.ts';
import BinaryWriter from '../lib/binary_writer.ts';
import { ClientMessage, ServerMessage } from './client_api/types';
import type { WebSocketAdapter, WebSocketArgs, WebSocketFactory } from './ws';
import { PREFERRED_WS_PROTOCOLS, V3_WS_PROTOCOL } from './websocket_protocols';
import {
  decodeClientMessagesV3,
  encodeServerMessagesV3,
} from './websocket_v3_frames.ts';

class WebsocketTestAdapter implements WebSocketAdapter {
  protocol: string = '';

  /** The arguments the connection passed to `openWebSocket`, for assertions. */
  connectArgs?: WebSocketArgs;

  // WebSocket.CLOSED (3) / WebSocket.OPEN (1). Uses literals rather than the
  // `WebSocket` global, which is not defined when these tests run under Node.
  get readyState(): number {
    return this.closed ? 3 : 1;
  }

  messageQueue: Uint8Array<ArrayBuffer>[];
  outgoingMessages: ClientMessage[];
  closed: boolean;
  supportedProtocols: string[];

  #onclose: (ev: CloseEvent) => void = () => {};
  #onopen: () => void = () => {};
  #onmessage: (msg: { data: Uint8Array }) => void = () => {};
  #onerror: (msg: ErrorEvent) => void = () => {};

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

  set onerror(handler: (msg: ErrorEvent) => void) {
    this.#onerror = handler;
  }

  error(error: Error): void {
    this.#onerror(
      Object.assign(new Event('error'), {
        error,
        message: error.message,
        filename: '',
        lineno: 0,
        colno: 0,
      })
    );
  }

  send(message: Uint8Array<ArrayBuffer>): void {
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
    this.serverClose(1000, 'normal closure', true);
  }

  /**
   * Simulate a close initiated by the server or the network, with an
   * arbitrary close code (e.g. an abnormal closure or an
   * application-specific code such as session-expired).
   */
  serverClose(
    code: number,
    reason: string = '',
    wasClean: boolean = false
  ): void {
    this.closed = true;
    this.#onclose(
      Object.assign(new Event('close'), { code, reason, wasClean })
    );
  }

  /**
   * Mark the socket as closed without delivering any event, simulating a
   * socket that died while the page was suspended (a "zombie" socket).
   */
  dieSilently(): void {
    this.closed = true;
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

  openWebSocket: WebSocketFactory = async args => {
    const negotiatedProtocol = args.wsProtocol.find(protocol =>
      this.supportedProtocols.includes(protocol)
    );
    if (!negotiatedProtocol) {
      throw new Error('No compatible websocket protocol');
    }
    this.protocol = negotiatedProtocol;
    this.connectArgs = args;
    return this;
  };
}

/**
 * A websocket factory that hands out a fresh {@link WebsocketTestAdapter} per
 * connection attempt and records them all. Used to test automatic
 * reconnection, where each attempt opens a new socket.
 */
export class WebsocketTestAdapterFactory {
  /** Every adapter created so far, in creation order. */
  sockets: WebsocketTestAdapter[] = [];
  /**
   * When set, the next `openWebSocket` calls reject with this error instead
   * of producing a socket (simulating an unreachable server or a failed
   * token exchange).
   */
  connectError?: Error;

  /** The most recently created adapter. */
  get current(): WebsocketTestAdapter {
    const socket = this.sockets[this.sockets.length - 1];
    if (!socket) {
      throw new Error('No websocket has been opened yet');
    }
    return socket;
  }

  openWebSocket: WebSocketFactory = async args => {
    if (this.connectError) {
      throw this.connectError;
    }
    const adapter = new WebsocketTestAdapter();
    await adapter.openWebSocket(args);
    this.sockets.push(adapter);
    return adapter;
  };
}

export default WebsocketTestAdapter;
