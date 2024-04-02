"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
class WebsocketTestAdapter {
    onclose;
    onopen;
    onmessage;
    onerror;
    messageQueue;
    closed;
    constructor() {
        this.messageQueue = [];
        this.closed = false;
    }
    send(message) {
        this.messageQueue.push(message);
    }
    close() {
        this.closed = true;
    }
    acceptConnection() {
        this.onopen();
    }
    sendToClient(message) {
        if (typeof message.data !== 'string') {
            message.data = JSON.stringify(message.data);
        }
        this.onmessage(message);
    }
}
exports.default = WebsocketTestAdapter;
