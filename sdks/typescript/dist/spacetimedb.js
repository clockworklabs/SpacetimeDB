"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null) for (var k in mod) if (k !== "default" && Object.prototype.hasOwnProperty.call(mod, k)) __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
};
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.__SPACETIMEDB__ = exports.SpacetimeDBClient = exports.DatabaseTable = exports.Reducer = exports.ReducerEvent = exports.BinarySerializer = exports.BuiltinType = exports.SumTypeVariant = exports.SumType = exports.ProductTypeElement = exports.ProductType = exports.AlgebraicType = exports.AlgebraicValue = exports.ProductValue = void 0;
const events_1 = require("events");
const isomorphic_ws_1 = __importDefault(require("isomorphic-ws"));
const algebraic_value_1 = require("./algebraic_value");
Object.defineProperty(exports, "ProductValue", { enumerable: true, get: function () { return algebraic_value_1.ProductValue; } });
Object.defineProperty(exports, "AlgebraicValue", { enumerable: true, get: function () { return algebraic_value_1.AlgebraicValue; } });
const serializer_1 = require("./serializer");
Object.defineProperty(exports, "BinarySerializer", { enumerable: true, get: function () { return serializer_1.BinarySerializer; } });
const algebraic_type_1 = require("./algebraic_type");
Object.defineProperty(exports, "AlgebraicType", { enumerable: true, get: function () { return algebraic_type_1.AlgebraicType; } });
Object.defineProperty(exports, "ProductType", { enumerable: true, get: function () { return algebraic_type_1.ProductType; } });
Object.defineProperty(exports, "ProductTypeElement", { enumerable: true, get: function () { return algebraic_type_1.ProductTypeElement; } });
Object.defineProperty(exports, "SumType", { enumerable: true, get: function () { return algebraic_type_1.SumType; } });
Object.defineProperty(exports, "SumTypeVariant", { enumerable: true, get: function () { return algebraic_type_1.SumTypeVariant; } });
Object.defineProperty(exports, "BuiltinType", { enumerable: true, get: function () { return algebraic_type_1.BuiltinType; } });
const identity_1 = require("./identity");
const address_1 = require("./address");
const reducer_event_1 = require("./reducer_event");
Object.defineProperty(exports, "ReducerEvent", { enumerable: true, get: function () { return reducer_event_1.ReducerEvent; } });
const Proto = __importStar(require("./client_api"));
const binary_reader_1 = __importDefault(require("./binary_reader"));
const table_1 = require("./table");
const utils_1 = require("./utils");
const database_table_1 = require("./database_table");
Object.defineProperty(exports, "DatabaseTable", { enumerable: true, get: function () { return database_table_1.DatabaseTable; } });
const reducer_1 = require("./reducer");
Object.defineProperty(exports, "Reducer", { enumerable: true, get: function () { return reducer_1.Reducer; } });
const client_db_1 = require("./client_db");
const message_types_1 = require("./message_types");
const logger_1 = require("./logger");
const decompress_1 = __importDefault(require("brotli/decompress"));
const buffer_1 = require("buffer");
const g = (typeof window === "undefined" ? global : window);
/**
 * The database client connection to a SpacetimeDB server.
 */
class SpacetimeDBClient {
    /**
     * The user's public identity.
     */
    identity = undefined;
    /**
     * The user's private authentication token.
     */
    token = undefined;
    /**
     * Reference to the database of the client.
     */
    db;
    emitter;
    /**
     * Whether the client is connected.
     */
    live;
    ws;
    manualTableSubscriptions = [];
    queriesQueue;
    runtime;
    createWSFn;
    protocol;
    ssl = false;
    clientAddress = address_1.Address.random();
    static tableClasses = new Map();
    static reducerClasses = new Map();
    static getTableClass(name) {
        const tableClass = this.tableClasses.get(name);
        if (!tableClass) {
            throw `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerTable() first`;
        }
        return tableClass;
    }
    static getReducerClass(name) {
        const reducerName = `${name}Reducer`;
        const reducerClass = this.reducerClasses.get(reducerName);
        if (!reducerClass) {
            (0, logger_1.stdbLogger)("warn", `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerReducer() first`);
            return;
        }
        return reducerClass;
    }
    /**
     * Creates a new `SpacetimeDBClient` database client and set the initial parameters.
     *
     * @param host The host of the SpacetimeDB server.
     * @param name_or_address The name or address of the SpacetimeDB module.
     * @param auth_token The credentials to use to connect to authenticate with SpacetimeDB.
     * @param protocol Define how encode the messages: `"binary" | "json"`. Binary is more efficient and compact, but JSON provides human-readable debug information.
     *
     * @example
     *
     * ```ts
     * const host = "ws://localhost:3000";
     * const name_or_address = "database_name"
     * const auth_token = undefined;
     * const protocol = "binary"
     *
     * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token, protocol);
     * ```
     */
    constructor(host, name_or_address, auth_token, protocol) {
        this.protocol = protocol || "binary";
        const global = g.__SPACETIMEDB__;
        if (global.spacetimeDBClient) {
            // If a client has been already created earlier it means the developer
            // wants to create multiple clients and thus let's create a new ClientDB.
            // The global ClientDB will be onl shared with the first created client
            this.db = new client_db_1.ClientDB();
        }
        else {
            // if this is the first client let's use the global ClientDB and set this instance
            // as the global instance
            this.db = global.clientDB;
            global.spacetimeDBClient = this;
        }
        // for (const [_name, reducer] of SpacetimeDBClient.reducerClasses) {
        //   this.registerReducer(reducer);
        // }
        if (SpacetimeDBClient.tableClasses.size === 0) {
            (0, logger_1.stdbLogger)("warn", "No tables were automatically registered globally, if you want to automatically register tables, you need to register them with SpacetimeDBClient.registerTable() first");
        }
        for (const [_name, table] of SpacetimeDBClient.tableClasses) {
            this.registerTable(table);
        }
        this.live = false;
        this.emitter = new events_1.EventEmitter();
        this.queriesQueue = [];
        this.runtime = {
            host,
            name_or_address,
            auth_token,
            global,
        };
        this.createWSFn = this.defaultCreateWebSocketFn;
    }
    async defaultCreateWebSocketFn(url, protocol) {
        const headers = {};
        if (this.runtime.auth_token) {
            headers["Authorization"] = `Basic ${btoa("token:" + this.runtime.auth_token)}`;
        }
        if (typeof window === "undefined" || !this.runtime.auth_token) {
            // NodeJS environment
            const ws = new isomorphic_ws_1.default(url, protocol, {
                maxReceivedFrameSize: 100000000,
                maxReceivedMessageSize: 100000000,
                headers,
            });
            return ws;
        }
        else {
            // In the browser we first have to get a short lived token and only then connect to the websocket
            let httpProtocol = this.ssl ? "https://" : "http://";
            let tokenUrl = `${httpProtocol}${this.runtime.host}/identity/websocket_token`;
            const response = await fetch(tokenUrl, { method: "POST", headers });
            if (response.ok) {
                const { token } = await response.json();
                url += "&token=" + btoa("token:" + token);
            }
            return new isomorphic_ws_1.default(url, protocol);
        }
    }
    /**
     * Handles WebSocket onClose event.
     * @param event CloseEvent object.
     */
    handleOnClose(event) {
        (0, logger_1.stdbLogger)("warn", "Closed: " + event);
        this.emitter.emit("disconnected");
        this.emitter.emit("client_error", event);
    }
    /**
     * Handles WebSocket onError event.
     * @param event ErrorEvent object.
     */
    handleOnError(event) {
        (0, logger_1.stdbLogger)("warn", "WS Error: " + event);
        this.emitter.emit("disconnected");
        this.emitter.emit("client_error", event);
    }
    /**
     * Handles WebSocket onOpen event.
     */
    handleOnOpen() {
        this.live = true;
        if (this.queriesQueue.length > 0) {
            this.subscribe(this.queriesQueue);
            this.queriesQueue = [];
        }
    }
    /**
     * Handles WebSocket onMessage event.
     * @param wsMessage MessageEvent object.
     */
    handleOnMessage(wsMessage) {
        this.emitter.emit("receiveWSMessage", wsMessage);
        this.processMessage(wsMessage, (message) => {
            if (message instanceof message_types_1.SubscriptionUpdateMessage) {
                for (let tableUpdate of message.tableUpdates) {
                    const tableName = tableUpdate.tableName;
                    const entityClass = SpacetimeDBClient.getTableClass(tableName);
                    const table = this.db.getOrCreateTable(tableUpdate.tableName, undefined, entityClass);
                    table.applyOperations(this.protocol, tableUpdate.operations, undefined);
                }
                if (this.emitter) {
                    this.emitter.emit("initialStateSync");
                }
            }
            else if (message instanceof message_types_1.TransactionUpdateMessage) {
                const reducerName = message.event.reducerName;
                const reducer = reducerName
                    ? SpacetimeDBClient.getReducerClass(reducerName)
                    : undefined;
                let reducerEvent;
                let reducerArgs;
                if (reducer && message.event.status === "committed") {
                    let adapter;
                    if (this.protocol === "binary") {
                        adapter = new algebraic_value_1.BinaryReducerArgsAdapter(new algebraic_value_1.BinaryAdapter(new binary_reader_1.default(message.event.args)));
                    }
                    else {
                        adapter = new algebraic_value_1.JSONReducerArgsAdapter(message.event.args);
                    }
                    reducerArgs = reducer.deserializeArgs(adapter);
                }
                reducerEvent = new reducer_event_1.ReducerEvent(message.event.identity, message.event.address, message.event.originalReducerName, message.event.status, message.event.message, reducerArgs);
                for (let tableUpdate of message.tableUpdates) {
                    const tableName = tableUpdate.tableName;
                    const entityClass = SpacetimeDBClient.getTableClass(tableName);
                    const table = this.db.getOrCreateTable(tableUpdate.tableName, undefined, entityClass);
                    table.applyOperations(this.protocol, tableUpdate.operations, reducerEvent);
                }
                if (reducer) {
                    this.emitter.emit("reducer:" + reducerName, reducerEvent, ...(reducerArgs || []));
                }
            }
            else if (message instanceof message_types_1.IdentityTokenMessage) {
                this.identity = message.identity;
                if (this.runtime.auth_token) {
                    this.token = this.runtime.auth_token;
                }
                else {
                    this.token = message.token;
                }
                this.clientAddress = message.address;
                this.emitter.emit("connected", this.token, this.identity, this.clientAddress);
            }
        });
    }
    /**
     * Subscribes to a table without registering it as a component.
     *
     * @param table The table to subscribe to
     * @param query The query to subscribe to. If not provided, the default is `SELECT * FROM {table}`
     */
    registerManualTable(table, query) {
        this.manualTableSubscriptions.push(query ? query : `SELECT * FROM ${table}`);
        this.ws.send(JSON.stringify({
            subscribe: {
                query_strings: [...this.manualTableSubscriptions],
            },
        }));
    }
    /**
     * Unsubscribes from a table without unregistering it as a component.
     *
     * @param table The table to unsubscribe from
     */
    removeManualTable(table) {
        this.manualTableSubscriptions = this.manualTableSubscriptions.filter((val) => val !== table);
        this.ws.send(JSON.stringify({
            subscribe: {
                query_strings: this.manualTableSubscriptions.map((val) => `SELECT * FROM ${val}`),
            },
        }));
    }
    /**
     * Close the current connection.
     *
     * @example
     *
     * ```ts
     * var spacetimeDBClient = new SpacetimeDBClient("ws://localhost:3000", "database_name");
     *
     * spacetimeDBClient.disconnect()
     * ```
     */
    disconnect() {
        this.ws.close();
    }
    /**
     * Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.
     *
     * @param host The hostname of the SpacetimeDB server. Defaults to the value passed to the `constructor`.
     * @param name_or_address The name or address of the SpacetimeDB module. Defaults to the value passed to the `constructor`.
     * @param auth_token The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the `constructor`.
     *
     * @example
     *
     * ```ts
     * const host = "ws://localhost:3000";
     * const name_or_address = "database_name"
     * const auth_token = undefined;
     *
     * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token);
     * // Connect with the initial parameters
     * spacetimeDBClient.connect();
     * //Set the `auth_token`
     * spacetimeDBClient.connect(undefined, undefined, NEW_TOKEN);
     * ```
     */
    async connect(host, name_or_address, auth_token) {
        if (this.live) {
            return;
        }
        (0, logger_1.stdbLogger)("info", "Connecting to SpacetimeDB WS...");
        if (host) {
            this.runtime.host = host;
        }
        if (name_or_address) {
            this.runtime.name_or_address = name_or_address;
        }
        if (auth_token) {
            // TODO: do we need both of these
            this.runtime.auth_token = auth_token;
            this.token = auth_token;
        }
        // TODO: we should probably just accept a host and an ssl boolean flag in stead of this
        // whole dance
        let url = `${this.runtime.host}/database/subscribe/${this.runtime.name_or_address}`;
        if (!this.runtime.host.startsWith("ws://") &&
            !this.runtime.host.startsWith("wss://")) {
            url = "ws://" + url;
        }
        let clientAddress = this.clientAddress.toHexString();
        url += `?client_address=${clientAddress}`;
        this.ssl = url.startsWith("wss");
        this.runtime.host = this.runtime.host
            .replace("ws://", "")
            .replace("wss://", "");
        const stdbProtocol = this.protocol === "binary" ? "bin" : "text";
        this.ws = await this.createWSFn(url, `v1.${stdbProtocol}.spacetimedb`);
        this.ws.onclose = this.handleOnClose.bind(this);
        this.ws.onerror = this.handleOnError.bind(this);
        this.ws.onopen = this.handleOnOpen.bind(this);
        this.ws.onmessage = this.handleOnMessage.bind(this);
    }
    processMessage(wsMessage, callback) {
        if (this.protocol === "binary") {
            // Helpers for parsing message components which appear in multiple messages.
            const parseTableRowOperation = (rawTableOperation) => {
                const type = rawTableOperation.op === Proto.TableRowOperation_OperationType.INSERT
                    ? "insert"
                    : "delete";
                // Our SDKs are architected around having a hashable, equality-comparable key
                // which uniquely identifies every row.
                // This used to be a strong content-addressed hash computed by the DB,
                // but the DB no longer computes those hashes,
                // so now we just use the serialized row as the identifier.
                const rowPk = new TextDecoder().decode(rawTableOperation.row);
                return new table_1.TableOperation(type, rowPk, rawTableOperation.row);
            };
            const parseTableUpdate = (rawTableUpdate) => {
                const tableName = rawTableUpdate.tableName;
                const operations = [];
                for (const rawTableOperation of rawTableUpdate.tableRowOperations) {
                    operations.push(parseTableRowOperation(rawTableOperation));
                }
                return new table_1.TableUpdate(tableName, operations);
            };
            const parseSubscriptionUpdate = (subUpdate) => {
                const tableUpdates = [];
                for (const rawTableUpdate of subUpdate.tableUpdates) {
                    tableUpdates.push(parseTableUpdate(rawTableUpdate));
                }
                return new message_types_1.SubscriptionUpdateMessage(tableUpdates);
            };
            let data = wsMessage.data;
            if (typeof data.arrayBuffer === "undefined") {
                data = new Blob([data]);
            }
            data.arrayBuffer().then((data) => {
                // From https://github.com/foliojs/brotli.js/issues/31 :
                // use a `Buffer` rather than a `Uint8Array` because for some reason brotli requires that.
                let decompressed = (0, decompress_1.default)(new buffer_1.Buffer(data));
                const message = Proto.Message.decode(new Uint8Array(decompressed));
                if (message["subscriptionUpdate"]) {
                    const rawSubscriptionUpdate = message.subscriptionUpdate;
                    const subscriptionUpdate = parseSubscriptionUpdate(rawSubscriptionUpdate);
                    callback(subscriptionUpdate);
                }
                else if (message["transactionUpdate"]) {
                    const txUpdate = message.transactionUpdate;
                    const rawSubscriptionUpdate = txUpdate.subscriptionUpdate;
                    if (!rawSubscriptionUpdate) {
                        throw new Error("Received TransactionUpdate without SubscriptionUpdate");
                    }
                    const subscriptionUpdate = parseSubscriptionUpdate(rawSubscriptionUpdate);
                    const event = txUpdate.event;
                    if (!event) {
                        throw new Error("Received TransactionUpdate without Event");
                    }
                    const functionCall = event.functionCall;
                    if (!functionCall) {
                        throw new Error("Received TransactionUpdate with Event but no FunctionCall");
                    }
                    const identity = new identity_1.Identity(event.callerIdentity);
                    const address = address_1.Address.nullIfZero(event.callerAddress);
                    const originalReducerName = functionCall.reducer;
                    const reducerName = (0, utils_1.toPascalCase)(originalReducerName);
                    const args = functionCall.argBytes;
                    const status = Proto.event_StatusToJSON(event.status);
                    const messageStr = event.message;
                    const transactionUpdateEvent = new message_types_1.TransactionUpdateEvent(identity, address, originalReducerName, reducerName, args, status, messageStr);
                    const transactionUpdate = new message_types_1.TransactionUpdateMessage(subscriptionUpdate.tableUpdates, transactionUpdateEvent);
                    callback(transactionUpdate);
                }
                else if (message["identityToken"]) {
                    const identityToken = message.identityToken;
                    const identity = new identity_1.Identity(identityToken.identity);
                    const token = identityToken.token;
                    const address = new address_1.Address(identityToken.address);
                    const identityTokenMessage = new message_types_1.IdentityTokenMessage(identity, token, address);
                    callback(identityTokenMessage);
                }
            });
        }
        else {
            const parseTableRowOperation = (rawTableOperation) => {
                const type = rawTableOperation["op"];
                // Our SDKs are architected around having a hashable, equality-comparable key
                // which uniquely identifies every row.
                // This used to be a strong content-addressed hash computed by the DB,
                // but the DB no longer computes those hashes,
                // so now we just use the serialized row as the identifier.
                //
                // JSON.stringify may be expensive here, but if the client cared about performance
                // they'd be using the binary format anyway, so we don't care.
                const rowPk = JSON.stringify(rawTableOperation.row);
                return new table_1.TableOperation(type, rowPk, rawTableOperation.row);
            };
            const parseTableUpdate = (rawTableUpdate) => {
                const tableName = rawTableUpdate.table_name;
                const operations = [];
                for (const rawTableOperation of rawTableUpdate.table_row_operations) {
                    operations.push(parseTableRowOperation(rawTableOperation));
                }
                return new table_1.TableUpdate(tableName, operations);
            };
            const parseSubscriptionUpdate = (rawSubscriptionUpdate) => {
                const tableUpdates = [];
                for (const rawTableUpdate of rawSubscriptionUpdate.table_updates) {
                    tableUpdates.push(parseTableUpdate(rawTableUpdate));
                }
                return new message_types_1.SubscriptionUpdateMessage(tableUpdates);
            };
            const data = JSON.parse(wsMessage.data);
            if (data["SubscriptionUpdate"]) {
                const subscriptionUpdate = parseSubscriptionUpdate(data.SubscriptionUpdate);
                callback(subscriptionUpdate);
            }
            else if (data["TransactionUpdate"]) {
                const txUpdate = data.TransactionUpdate;
                const subscriptionUpdate = parseSubscriptionUpdate(txUpdate.subscription_update);
                const event = txUpdate.event;
                const functionCall = event.function_call;
                const identity = new identity_1.Identity(event.caller_identity);
                const address = address_1.Address.fromStringOrNull(event.caller_address);
                const originalReducerName = functionCall.reducer;
                const reducerName = (0, utils_1.toPascalCase)(originalReducerName);
                const args = JSON.parse(functionCall.args);
                const status = event.status;
                const message = event.message;
                const transactionUpdateEvent = new message_types_1.TransactionUpdateEvent(identity, address, originalReducerName, reducerName, args, status, message);
                const transactionUpdate = new message_types_1.TransactionUpdateMessage(subscriptionUpdate.tableUpdates, transactionUpdateEvent);
                callback(transactionUpdate);
            }
            else if (data["IdentityToken"]) {
                const identityToken = data.IdentityToken;
                const identity = new identity_1.Identity(identityToken.identity);
                const token = identityToken.token;
                const address = address_1.Address.fromString(identityToken.address);
                const identityTokenMessage = new message_types_1.IdentityTokenMessage(identity, token, address);
                callback(identityTokenMessage);
            }
        }
    }
    /**
     * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
     *
     * @param name The name of the component to register
     * @param component The component to register
     */
    registerTable(tableClass) {
        this.db.getOrCreateTable(tableClass.tableName, undefined, tableClass);
        // only set a default ClientDB on a table class if it's not set yet. This means
        // that only the first created client will be usable without the `with` method
        if (!tableClass.db) {
            tableClass.db = this.db;
        }
    }
    /**
     * Register a component to be used with any SpacetimeDB client. The component will be automatically registered to any
     * new clients
     * @param table Component to be registered
     */
    static registerTable(table) {
        this.tableClasses.set(table.tableName, table);
    }
    /**
     *  Register a list of components to be used with any SpacetimeDB client. The components will be automatically registered to any new clients
     * @param tables A list of tables to register globally with SpacetimeDBClient
     */
    static registerTables(...tables) {
        for (const table of tables) {
            this.registerTable(table);
        }
    }
    /**
     * Register a reducer to be used with any SpacetimeDB client. The reducer will be automatically registered to any
     * new clients
     * @param reducer Reducer to be registered
     */
    static registerReducer(reducer) {
        this.reducerClasses.set(reducer.reducerName + "Reducer", reducer);
    }
    /**
     * Register a list of reducers to be used with any SpacetimeDB client. The reducers will be automatically registered to any new clients
     * @param reducers A list of reducers to register globally with SpacetimeDBClient
     */
    static registerReducers(...reducers) {
        for (const reducer of reducers) {
            this.registerReducer(reducer);
        }
    }
    /**
     * Subscribe to a set of queries, to be notified when rows which match those queries are altered.
     *
     * NOTE: A new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`.
     *
     * If any rows matched the previous subscribed queries but do not match the new queries,
     * those rows will be removed from the client cache, and `{Table}.on_delete` callbacks will be invoked for them.
     *
     * @param queries A `SQL` query or list of queries.
     *
     * @example
     *
     * ```ts
     * spacetimeDBClient.subscribe(["SELECT * FROM User","SELECT * FROM Message"]);
     * ```
     */
    subscribe(queryOrQueries) {
        const queries = typeof queryOrQueries === "string" ? [queryOrQueries] : queryOrQueries;
        if (this.live) {
            const message = { subscribe: { query_strings: queries } };
            this.emitter.emit("sendWSMessage", message);
            this.ws.send(JSON.stringify(message));
        }
        else {
            this.queriesQueue = this.queriesQueue.concat(queries);
        }
    }
    /**
     * Call a reducer on your SpacetimeDB module.
     *
     * @param reducerName The name of the reducer to call
     * @param args The arguments to pass to the reducer
     */
    call(reducerName, serializer) {
        let message;
        if (this.protocol === "binary") {
            const pmessage = {
                functionCall: {
                    reducer: reducerName,
                    argBytes: serializer.args(),
                },
            };
            message = Proto.Message.encode(pmessage).finish();
        }
        else {
            message = JSON.stringify({
                call: {
                    fn: reducerName,
                    args: serializer.args(),
                },
            });
        }
        this.emitter.emit("sendWSMessage", message);
        this.ws.send(message);
    }
    on(eventName, callback) {
        this.emitter.on(eventName, callback);
    }
    off(eventName, callback) {
        this.emitter.off(eventName, callback);
    }
    /**
     * Register a callback to be invoked upon authentication with the database.
     *
     * @param token The credentials to use to authenticate with SpacetimeDB.
     * @param identity A unique public identifier for a client connected to a database.
     *
     * The callback will be invoked with the public `Identity` and private authentication `token` provided by the database to identify this connection.
     *
     * If credentials were supplied to connect, those passed to the callback will be equivalent to the ones used to connect.
     *
     * If the initial connection was anonymous, a new set of credentials will be generated by the database to identify this user.
     *
     * The credentials passed to the callback can be saved and used to authenticate the same user in future connections.
     *
     * @example
     *
     * ```ts
     * spacetimeDBClient.onConnect((token, identity) => {
     *  console.log("Connected to SpacetimeDB");
     *  console.log("Token", token);
     *  console.log("Identity", identity);
     * });
     * ```
     */
    onConnect(callback) {
        this.on("connected", callback);
    }
    /**
     * Register a callback to be invoked upon an error.
     *
     * @example
     *
     * ```ts
     * spacetimeDBClient.onError((...args: any[]) => {
     *  stdbLogger("warn","ERROR", args);
     * });
     * ```
     */
    onError(callback) {
        this.on("client_error", callback);
    }
    _setCreateWSFn(fn) {
        this.createWSFn = fn;
    }
    getSerializer() {
        if (this.protocol === "binary") {
            return new serializer_1.BinarySerializer();
        }
        else {
            return new serializer_1.JSONSerializer();
        }
    }
}
exports.SpacetimeDBClient = SpacetimeDBClient;
g.__SPACETIMEDB__ = {
    clientDB: new client_db_1.ClientDB(),
    spacetimeDBClient: undefined,
};
exports.__SPACETIMEDB__ = (typeof window === "undefined"
    ? global.__SPACETIMEDB__
    : window.__SPACETIMEDB__);
