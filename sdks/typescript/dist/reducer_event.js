"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ReducerEvent = void 0;
class ReducerEvent {
    callerIdentity;
    callerAddress;
    reducerName;
    status;
    message;
    args;
    constructor(callerIdentity, callerAddress, reducerName, status, message, args) {
        this.callerIdentity = callerIdentity;
        this.callerAddress = callerAddress;
        this.reducerName = reducerName;
        this.status = status;
        this.message = message;
        this.args = args;
    }
}
exports.ReducerEvent = ReducerEvent;
