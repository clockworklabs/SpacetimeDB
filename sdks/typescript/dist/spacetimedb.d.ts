/// <reference types="node" />
import { EventEmitter } from "events";
import WebSocket from "isomorphic-ws";
import type { WebsocketTestAdapter } from "./websocket_test_adapter";
import { ProductValue, AlgebraicValue, ValueAdapter, ReducerArgsAdapter } from "./algebraic_value";
import { Serializer, BinarySerializer } from "./serializer";
import { AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType } from "./algebraic_type";
import { EventType } from "./types";
import { Identity } from "./identity";
import { Address } from "./address";
import { ReducerEvent } from "./reducer_event";
import { DatabaseTable, DatabaseTableClass } from "./database_table";
import { Reducer, ReducerClass } from "./reducer";
import { ClientDB } from "./client_db";
import { SpacetimeDBGlobals } from "./global";
export { ProductValue, AlgebraicValue, AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType, BinarySerializer, ReducerEvent, Reducer, ReducerClass, DatabaseTable, DatabaseTableClass, };
export type { ValueAdapter, ReducerArgsAdapter, Serializer };
type CreateWSFnType = (url: string, protocol: string) => WebSocket | WebsocketTestAdapter;
/**
 * The database client connection to a SpacetimeDB server.
 */
export declare class SpacetimeDBClient {
    /**
     * The user's public identity.
     */
    identity?: Identity;
    /**
     * The user's private authentication token.
     */
    token?: string;
    /**
     * Reference to the database of the client.
     */
    db: ClientDB;
    emitter: EventEmitter;
    /**
     * Whether the client is connected.
     */
    live: boolean;
    private ws;
    private manualTableSubscriptions;
    private queriesQueue;
    private runtime;
    private createWSFn;
    private protocol;
    private ssl;
    private clientAddress;
    private static tableClasses;
    private static reducerClasses;
    private static getTableClass;
    private static getReducerClass;
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
    constructor(host: string, name_or_address: string, auth_token?: string, protocol?: "binary" | "json");
    private defaultCreateWebSocketFn;
    /**
     * Handles WebSocket onClose event.
     * @param event CloseEvent object.
     */
    private handleOnClose;
    /**
     * Handles WebSocket onError event.
     * @param event ErrorEvent object.
     */
    private handleOnError;
    /**
     * Handles WebSocket onOpen event.
     */
    private handleOnOpen;
    /**
     * Handles WebSocket onMessage event.
     * @param wsMessage MessageEvent object.
     */
    private handleOnMessage;
    /**
     * Subscribes to a table without registering it as a component.
     *
     * @param table The table to subscribe to
     * @param query The query to subscribe to. If not provided, the default is `SELECT * FROM {table}`
     */
    registerManualTable(table: string, query?: string): void;
    /**
     * Unsubscribes from a table without unregistering it as a component.
     *
     * @param table The table to unsubscribe from
     */
    removeManualTable(table: string): void;
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
    disconnect(): void;
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
    connect(host?: string, name_or_address?: string, auth_token?: string): Promise<void>;
    private processMessage;
    /**
     * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
     *
     * @param name The name of the component to register
     * @param component The component to register
     */
    private registerTable;
    /**
     * Register a component to be used with any SpacetimeDB client. The component will be automatically registered to any
     * new clients
     * @param table Component to be registered
     */
    static registerTable(table: DatabaseTableClass): void;
    /**
     *  Register a list of components to be used with any SpacetimeDB client. The components will be automatically registered to any new clients
     * @param tables A list of tables to register globally with SpacetimeDBClient
     */
    static registerTables(...tables: DatabaseTableClass[]): void;
    /**
     * Register a reducer to be used with any SpacetimeDB client. The reducer will be automatically registered to any
     * new clients
     * @param reducer Reducer to be registered
     */
    static registerReducer(reducer: ReducerClass): void;
    /**
     * Register a list of reducers to be used with any SpacetimeDB client. The reducers will be automatically registered to any new clients
     * @param reducers A list of reducers to register globally with SpacetimeDBClient
     */
    static registerReducers(...reducers: ReducerClass[]): void;
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
    subscribe(queryOrQueries: string | string[]): void;
    /**
     * Call a reducer on your SpacetimeDB module.
     *
     * @param reducerName The name of the reducer to call
     * @param args The arguments to pass to the reducer
     */
    call(reducerName: string, serializer: Serializer): void;
    on(eventName: EventType | string, callback: (...args: any[]) => void): void;
    off(eventName: EventType | string, callback: (...args: any[]) => void): void;
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
    onConnect(callback: (token: string, identity: Identity, address: Address) => void): void;
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
    onError(callback: (...args: any[]) => void): void;
    _setCreateWSFn(fn: CreateWSFnType): void;
    getSerializer(): Serializer;
}
export declare const __SPACETIMEDB__: SpacetimeDBGlobals;
