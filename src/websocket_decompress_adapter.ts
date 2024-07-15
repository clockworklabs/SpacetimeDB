import WebSocket from "isomorphic-ws";
import decompress from "brotli/decompress";
import { Buffer } from "buffer";

export class WebsocketDecompressAdapter {
  public onclose: Function | undefined;
  public onopen: Function | undefined;
  public onmessage: ((msg: { data: Uint8Array }) => void) | undefined;
  public onerror: Function | undefined;

  private ws: WebSocket;

  private handleOnMessage(msg: { data: any }) {
    msg.data.arrayBuffer().then((data: Uint8Array) => {
      const decompressed = decompress(new Buffer(data));
      if (this.onmessage) {
        this.onmessage({ data: decompressed });
      }
    });
  }

  private handleOnClose(msg: any) {
    if (this.onclose !== undefined) {
      this.onclose(msg);
    }
  }

  private handleOnOpen(msg: any) {
    if (this.onopen !== undefined) {
      this.onopen(msg);
    }
  }

  private handleOnError(msg: any) {
    if (this.onerror !== undefined) {
      this.onerror(msg);
    }
  }

  public send(msg: any) {
    this.ws.send(msg);
  }

  public close() {
    this.ws.close();
  }

  constructor(ws: WebSocket) {
    this.onmessage = undefined;
    this.onopen = undefined;
    this.onmessage = undefined;
    this.onerror = undefined;

    ws.onmessage = this.handleOnMessage.bind(this);
    ws.onerror = this.handleOnError.bind(this);
    ws.onclose = this.handleOnError.bind(this);
    ws.onopen = this.handleOnOpen.bind(this);

    this.ws = ws;
  }

  public static async createWebSocketFn(
    url: string,
    protocol: string,
    params: {
      host: string;
      auth_token: string | undefined | null;
      ssl: boolean;
    }
  ): Promise<WebsocketDecompressAdapter> {
    const headers: { [key: string]: string } = {};
    if (params.auth_token) {
      headers["Authorization"] = `Basic ${btoa("token:" + params.auth_token)}`;
    }

    if (typeof window === "undefined" || !params.auth_token) {
      // NodeJS environment
      const ws = new WebSocket(url, protocol, {
        maxReceivedFrameSize: 100000000,
        maxReceivedMessageSize: 100000000,
        headers,
      });
      return new WebsocketDecompressAdapter(ws);
    } else {
      // In the browser we first have to get a short lived token and only then connect to the websocket
      let httpProtocol = params.ssl ? "https://" : "http://";
      let tokenUrl = `${httpProtocol}${params.host}/identity/websocket_token`;

      const response = await fetch(tokenUrl, { method: "POST", headers });
      if (response.ok) {
        const { token } = await response.json();
        url += "&token=" + btoa("token:" + token);
      }
      const ws = new WebSocket(url, protocol);
      return new WebsocketDecompressAdapter(ws);
    }
  }
}
