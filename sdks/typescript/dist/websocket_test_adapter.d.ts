declare class WebsocketTestAdapter {
    onclose: any;
    onopen: Function;
    onmessage: any;
    onerror: any;
    messageQueue: any[];
    closed: boolean;
    constructor();
    send(message: any): void;
    close(): void;
    acceptConnection(): void;
    sendToClient(message: any): void;
}
export type { WebsocketTestAdapter };
export default WebsocketTestAdapter;
