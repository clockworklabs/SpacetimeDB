"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.IdentityTokenMessage = exports.TransactionUpdateMessage = exports.TransactionUpdateEvent = exports.SubscriptionUpdateMessage = void 0;
class SubscriptionUpdateMessage {
    tableUpdates;
    constructor(tableUpdates) {
        this.tableUpdates = tableUpdates;
    }
}
exports.SubscriptionUpdateMessage = SubscriptionUpdateMessage;
class TransactionUpdateEvent {
    identity;
    address;
    originalReducerName;
    reducerName;
    args;
    status;
    message;
    constructor(identity, address, originalReducerName, reducerName, args, status, message) {
        this.identity = identity;
        this.address = address;
        this.originalReducerName = originalReducerName;
        this.reducerName = reducerName;
        this.args = args;
        this.status = status;
        this.message = message;
    }
}
exports.TransactionUpdateEvent = TransactionUpdateEvent;
class TransactionUpdateMessage {
    tableUpdates;
    event;
    constructor(tableUpdates, event) {
        this.tableUpdates = tableUpdates;
        this.event = event;
    }
}
exports.TransactionUpdateMessage = TransactionUpdateMessage;
class IdentityTokenMessage {
    identity;
    token;
    address;
    constructor(identity, token, address) {
        this.identity = identity;
        this.token = token;
        this.address = address;
    }
}
exports.IdentityTokenMessage = IdentityTokenMessage;
