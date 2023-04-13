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

  public sendToClient(message: any) {
    if (typeof message.data !== 'string') {
      message.data = JSON.stringify(message.data);
    }
    this.onmessage(message);
  }
}

export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
