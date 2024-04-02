"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Reducer = void 0;
class Reducer {
    static reducerName;
    call(..._args) {
        throw "not implemented";
    }
    on(..._args) {
        throw "not implemented";
    }
    client;
    static with(client) {
        return new this(client);
    }
    static reducer;
    static getReducer() {
        if (!this.reducer && __SPACETIMEDB__.spacetimeDBClient) {
            this.reducer = new this(__SPACETIMEDB__.spacetimeDBClient);
        }
        if (this.reducer) {
            return this.reducer;
        }
        else {
            throw "You need to instantiate a client in order to use reducers.";
        }
    }
    constructor(client) {
        this.client = client;
    }
}
exports.Reducer = Reducer;
