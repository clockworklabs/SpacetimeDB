import * as _m0 from "protobufjs/minimal";
export declare const protobufPackage = "client_api";
/**
 * //// Generic Message //////
 * TODO: Theoretically this format could be replaced by AlgebraicValue/AlgebraicType
 * but I don't think we want to do that yet.
 * TODO: Split this up into ServerBound and ClientBound if there's no overlap
 */
export interface Message {
    /** client -> database, request a reducer run. */
    functionCall?: FunctionCall | undefined;
    /**
     * database -> client, contained in `TransactionUpdate`, informs of changes to
     * subscribed rows.
     */
    subscriptionUpdate?: SubscriptionUpdate | undefined;
    /** database -> client, contained in `TransactionUpdate`, describes a reducer run. */
    event?: Event | undefined;
    /** database -> client, upon reducer run. */
    transactionUpdate?: TransactionUpdate | undefined;
    /** database -> client, after connecting, to inform client of its identity. */
    identityToken?: IdentityToken | undefined;
    /** client -> database, register SQL queries on which to receive updates. */
    subscribe?: Subscribe | undefined;
    /** client -> database, send a one-off SQL query without establishing a subscription. */
    oneOffQuery?: OneOffQuery | undefined;
    /** database -> client, return results to a one off SQL query. */
    oneOffQueryResponse?: OneOffQueryResponse | undefined;
}
/**
 * / Received by database from client to inform of user's identity, token and client address.
 * /
 * / The database will always send an `IdentityToken` message
 * / as the first message for a new WebSocket connection.
 * / If the client is re-connecting with existing credentials,
 * / the message will include those credentials.
 * / If the client connected anonymously,
 * / the database will generate new credentials to identify it.
 */
export interface IdentityToken {
    identity: Uint8Array;
    token: string;
    address: Uint8Array;
}
/**
 * / Sent by client to database to request a reducer runs.
 * /
 * / - `reducer` is the string name of a reducer to run.
 * /
 * / - `argBytes` is the arguments to the reducer, encoded as BSATN.
 * /
 * / SpacetimeDB models reducers as taking a single `AlgebraicValue` as an argument, which
 * / generally will be a `ProductValue` containing all of the args (except the
 * / `ReducerContext`, which is injected by the host, not provided in this API).
 */
export interface FunctionCall {
    /** TODO: Maybe this should be replaced with an int identifier for performance? */
    reducer: string;
    argBytes: Uint8Array;
}
/**
 * / Sent by client to database to register a set of queries, about which the client will
 * / receive `TransactionUpdate`s.
 * /
 * / `query_strings` is a sequence of strings, each of which is a SQL query.
 * /
 * / After issuing a `Subscribe` message, the client will receive a single
 * / `SubscriptionUpdate` message containing every current row of every table which matches
 * / the subscribed queries. Then, after each reducer run which updates one or more
 * / subscribed rows, the client will receive a `TransactionUpdate` containing the updates.
 * /
 * / A `Subscribe` message sets or replaces the entire set of queries to which the client
 * / is subscribed. If the client is previously subscribed to some set of queries `A`, and
 * / then sends a `Subscribe` message to subscribe to a set `B`, afterwards, the client
 * / will be subscribed to `B` but not `A`. In this case, the client will receive a
 * / `SubscriptionUpdate` containing every existing row that matches `B`, even if some were
 * / already in `A`.
 */
export interface Subscribe {
    queryStrings: string[];
}
/**
 * / Part of a `TransactionUpdate` received by client from database upon a reducer run.
 * /
 * / - `timestamp` is the time when the reducer started,
 * /               as microseconds since the Unix epoch.
 * /
 * / - `callerIdentity` is the identity of the user who requested the reducer run.
 * /                    For event-driven and scheduled reducers,
 * /                    it is the identity of the database owner.
 * /
 * / - `functionCall` contains the name of the reducer which ran and the arguments it
 * /                  received.
 * /
 * / - `status` of `committed` means that the reducer ran successfully and its changes were
 * /                           committed to the database. The rows altered in the database
 * /                           will be recorded in the parent `TransactionUpdate`'s
 * /                           `SubscriptionUpdate`.
 * /
 * / - `status` of `failed` means that the reducer panicked, and any changes it attempted to
 * /                        make were rolled back.
 * /
 * / - `status` of `out_of_energy` means that the reducer was interrupted
 * /                               due to insufficient energy/funds,
 * /                               and any changes it attempted to make were rolled back.
 * /
 * / - `message` is the error message with which the reducer failed.
 * /             For `committed` or `out_of_energy` statuses,
 * /             it is the empty string.
 * /
 * / - `energy_quanta_used` and `host_execution_duration_micros` seem self-explanatory;
 * /   they describe the amount of energy credits consumed by running the reducer,
 * /   and how long it took to run.
 * /
 * / - `callerAddress` is the 16-byte address of the user who requested the reducer run.
 * /                   The all-zeros address is a sentinel which denotes no address.
 * /                   `init` and `update` reducers will have a `callerAddress`
 * /                   if and only if one was provided to the `publish` HTTP endpoint.
 * /                   Scheduled reducers will never have a `callerAddress`.
 * /                   Reducers invoked by HTTP will have a `callerAddress`
 * /                   if and only if one was provided to the `call` HTTP endpoint.
 * /                   Reducers invoked by WebSocket will always have a `callerAddress`.
 */
export interface Event {
    timestamp: number;
    callerIdentity: Uint8Array;
    functionCall: FunctionCall | undefined;
    /**
     * TODO: arguably these should go inside an EventStatus message
     * since success doesn't have a message
     */
    status: Event_Status;
    message: string;
    energyQuantaUsed: number;
    hostExecutionDurationMicros: number;
    callerAddress: Uint8Array;
}
export declare enum Event_Status {
    committed = 0,
    failed = 1,
    out_of_energy = 2,
    UNRECOGNIZED = -1
}
export declare function event_StatusFromJSON(object: any): Event_Status;
export declare function event_StatusToJSON(object: Event_Status): string;
/**
 * / Part of a `TransactionUpdate` received by client from database when subscribed rows in
 * / a table are altered, or received alone after a `Subscription` to initialize the
 * / client's mirror of the database.
 * /
 * / A single `SubscriptionUpdate` may contain `TableUpdate` messages for multiple
 * / tables.
 */
export interface SubscriptionUpdate {
    tableUpdates: TableUpdate[];
}
/**
 * / Part of a `SubscriptionUpdate` received by client from database for alterations to a
 * / single table.
 * /
 * / `tableId` and `tableName` identify the table. Clients should use the `tableName`, as
 * /                           it is a stable part of a module's API, whereas `tableId` may
 * /                           or may not change between runs.
 * /
 * / `tableRowOperations` are actual modified rows.
 */
export interface TableUpdate {
    tableId: number;
    tableName: string;
    tableRowOperations: TableRowOperation[];
}
/**
 * / Part of a `TableUpdate` received by client from database for alteration to a single
 * / row of a table.
 * /
 * / The table being altered is identified by the parent `TableUpdate`.
 * /
 * / - `op` of `DELETE` means that the row in question has been removed and is no longer
 * /                    resident in the table.
 * /
 * / - `op` of `INSERT` means that the row in question has been either newly inserted or
 * /                    updated, and is resident in the table.
 * /
 * / - `row` is the row itself, encoded as BSATN.
 */
export interface TableRowOperation {
    op: TableRowOperation_OperationType;
    row: Uint8Array;
}
export declare enum TableRowOperation_OperationType {
    DELETE = 0,
    INSERT = 1,
    UNRECOGNIZED = -1
}
export declare function tableRowOperation_OperationTypeFromJSON(object: any): TableRowOperation_OperationType;
export declare function tableRowOperation_OperationTypeToJSON(object: TableRowOperation_OperationType): string;
/**
 * / Received by client from database upon a reducer run.
 * /
 * / Clients receive `TransactionUpdate`s only for reducers
 * / which update at least one of their subscribed rows,
 * / or for their own `failed` or `out_of_energy` reducer invocations.
 * /
 * / - `event` contains information about the reducer.
 * /
 * / - `subscriptionUpdate` contains changes to subscribed rows.
 */
export interface TransactionUpdate {
    event: Event | undefined;
    subscriptionUpdate: SubscriptionUpdate | undefined;
}
/**
 * / A one-off query submission.
 * /
 * / Query should be a "SELECT * FROM Table WHERE ...". Other types of queries will be rejected.
 * / Multiple such semicolon-delimited queries are allowed.
 * /
 * / One-off queries are identified by a client-generated messageID.
 * / To avoid data leaks, the server will NOT cache responses to messages based on UUID!
 * / It also will not check for duplicate IDs. They are just a way to match responses to messages.
 */
export interface OneOffQuery {
    messageId: Uint8Array;
    queryString: string;
}
/**
 * / A one-off query response.
 * / Will contain either one error or multiple response rows.
 * / At most one of these messages will be sent in reply to any query.
 * /
 * / The messageId will be identical to the one sent in the original query.
 */
export interface OneOffQueryResponse {
    messageId: Uint8Array;
    error: string;
    tables: OneOffTable[];
}
/** / A table included as part of a one-off query. */
export interface OneOffTable {
    tableName: string;
    row: Uint8Array[];
}
export declare const Message: {
    encode(message: Message, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): Message;
    fromJSON(object: any): Message;
    toJSON(message: Message): unknown;
    create<I extends {
        functionCall?: {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } | undefined;
        subscriptionUpdate?: {
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } | undefined;
        event?: {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } | undefined;
        transactionUpdate?: {
            event?: {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } | undefined;
            subscriptionUpdate?: {
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } | undefined;
        } | undefined;
        identityToken?: {
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } | undefined;
        subscribe?: {
            queryStrings?: string[] | undefined;
        } | undefined;
        oneOffQuery?: {
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } | undefined;
        oneOffQueryResponse?: {
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] | undefined;
        } | undefined;
    } & {
        functionCall?: ({
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & { [K in Exclude<keyof I["functionCall"], keyof FunctionCall>]: never; }) | undefined;
        subscriptionUpdate?: ({
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } & {
            tableUpdates?: ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] & ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            } & {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] & ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & { [K_1 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_2 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_3 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_4 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"], keyof {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_5 in Exclude<keyof I["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
        event?: ({
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: ({
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & { [K_6 in Exclude<keyof I["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & { [K_7 in Exclude<keyof I["event"], keyof Event>]: never; }) | undefined;
        transactionUpdate?: ({
            event?: {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } | undefined;
            subscriptionUpdate?: {
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } | undefined;
        } & {
            event?: ({
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } & {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: ({
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } & {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } & { [K_8 in Exclude<keyof I["transactionUpdate"]["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } & { [K_9 in Exclude<keyof I["transactionUpdate"]["event"], keyof Event>]: never; }) | undefined;
            subscriptionUpdate?: ({
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } & {
                tableUpdates?: ({
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] & ({
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                } & {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: ({
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] & ({
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    } & {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    } & { [K_10 in Exclude<keyof I["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_11 in Exclude<keyof I["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[]>]: never; }) | undefined;
                } & { [K_12 in Exclude<keyof I["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_13 in Exclude<keyof I["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"], keyof {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_14 in Exclude<keyof I["transactionUpdate"]["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
        } & { [K_15 in Exclude<keyof I["transactionUpdate"], keyof TransactionUpdate>]: never; }) | undefined;
        identityToken?: ({
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } & {
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } & { [K_16 in Exclude<keyof I["identityToken"], keyof IdentityToken>]: never; }) | undefined;
        subscribe?: ({
            queryStrings?: string[] | undefined;
        } & {
            queryStrings?: (string[] & string[] & { [K_17 in Exclude<keyof I["subscribe"]["queryStrings"], keyof string[]>]: never; }) | undefined;
        } & { [K_18 in Exclude<keyof I["subscribe"], "queryStrings">]: never; }) | undefined;
        oneOffQuery?: ({
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } & {
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } & { [K_19 in Exclude<keyof I["oneOffQuery"], keyof OneOffQuery>]: never; }) | undefined;
        oneOffQueryResponse?: ({
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] | undefined;
        } & {
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: ({
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] & ({
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            } & {
                tableName?: string | undefined;
                row?: (Uint8Array[] & Uint8Array[] & { [K_20 in Exclude<keyof I["oneOffQueryResponse"]["tables"][number]["row"], keyof Uint8Array[]>]: never; }) | undefined;
            } & { [K_21 in Exclude<keyof I["oneOffQueryResponse"]["tables"][number], keyof OneOffTable>]: never; })[] & { [K_22 in Exclude<keyof I["oneOffQueryResponse"]["tables"], keyof {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_23 in Exclude<keyof I["oneOffQueryResponse"], keyof OneOffQueryResponse>]: never; }) | undefined;
    } & { [K_24 in Exclude<keyof I, keyof Message>]: never; }>(base?: I | undefined): Message;
    fromPartial<I_1 extends {
        functionCall?: {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } | undefined;
        subscriptionUpdate?: {
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } | undefined;
        event?: {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } | undefined;
        transactionUpdate?: {
            event?: {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } | undefined;
            subscriptionUpdate?: {
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } | undefined;
        } | undefined;
        identityToken?: {
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } | undefined;
        subscribe?: {
            queryStrings?: string[] | undefined;
        } | undefined;
        oneOffQuery?: {
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } | undefined;
        oneOffQueryResponse?: {
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] | undefined;
        } | undefined;
    } & {
        functionCall?: ({
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & { [K_25 in Exclude<keyof I_1["functionCall"], keyof FunctionCall>]: never; }) | undefined;
        subscriptionUpdate?: ({
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } & {
            tableUpdates?: ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] & ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            } & {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] & ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & { [K_26 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_27 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_28 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_29 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"], keyof {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_30 in Exclude<keyof I_1["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
        event?: ({
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: ({
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & { [K_31 in Exclude<keyof I_1["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & { [K_32 in Exclude<keyof I_1["event"], keyof Event>]: never; }) | undefined;
        transactionUpdate?: ({
            event?: {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } | undefined;
            subscriptionUpdate?: {
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } | undefined;
        } & {
            event?: ({
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } & {
                timestamp?: number | undefined;
                callerIdentity?: Uint8Array | undefined;
                functionCall?: ({
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } & {
                    reducer?: string | undefined;
                    argBytes?: Uint8Array | undefined;
                } & { [K_33 in Exclude<keyof I_1["transactionUpdate"]["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
                status?: Event_Status | undefined;
                message?: string | undefined;
                energyQuantaUsed?: number | undefined;
                hostExecutionDurationMicros?: number | undefined;
                callerAddress?: Uint8Array | undefined;
            } & { [K_34 in Exclude<keyof I_1["transactionUpdate"]["event"], keyof Event>]: never; }) | undefined;
            subscriptionUpdate?: ({
                tableUpdates?: {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] | undefined;
            } & {
                tableUpdates?: ({
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[] & ({
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                } & {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: ({
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] & ({
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    } & {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    } & { [K_35 in Exclude<keyof I_1["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_36 in Exclude<keyof I_1["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[]>]: never; }) | undefined;
                } & { [K_37 in Exclude<keyof I_1["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_38 in Exclude<keyof I_1["transactionUpdate"]["subscriptionUpdate"]["tableUpdates"], keyof {
                    tableId?: number | undefined;
                    tableName?: string | undefined;
                    tableRowOperations?: {
                        op?: TableRowOperation_OperationType | undefined;
                        row?: Uint8Array | undefined;
                    }[] | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_39 in Exclude<keyof I_1["transactionUpdate"]["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
        } & { [K_40 in Exclude<keyof I_1["transactionUpdate"], keyof TransactionUpdate>]: never; }) | undefined;
        identityToken?: ({
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } & {
            identity?: Uint8Array | undefined;
            token?: string | undefined;
            address?: Uint8Array | undefined;
        } & { [K_41 in Exclude<keyof I_1["identityToken"], keyof IdentityToken>]: never; }) | undefined;
        subscribe?: ({
            queryStrings?: string[] | undefined;
        } & {
            queryStrings?: (string[] & string[] & { [K_42 in Exclude<keyof I_1["subscribe"]["queryStrings"], keyof string[]>]: never; }) | undefined;
        } & { [K_43 in Exclude<keyof I_1["subscribe"], "queryStrings">]: never; }) | undefined;
        oneOffQuery?: ({
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } & {
            messageId?: Uint8Array | undefined;
            queryString?: string | undefined;
        } & { [K_44 in Exclude<keyof I_1["oneOffQuery"], keyof OneOffQuery>]: never; }) | undefined;
        oneOffQueryResponse?: ({
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] | undefined;
        } & {
            messageId?: Uint8Array | undefined;
            error?: string | undefined;
            tables?: ({
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[] & ({
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            } & {
                tableName?: string | undefined;
                row?: (Uint8Array[] & Uint8Array[] & { [K_45 in Exclude<keyof I_1["oneOffQueryResponse"]["tables"][number]["row"], keyof Uint8Array[]>]: never; }) | undefined;
            } & { [K_46 in Exclude<keyof I_1["oneOffQueryResponse"]["tables"][number], keyof OneOffTable>]: never; })[] & { [K_47 in Exclude<keyof I_1["oneOffQueryResponse"]["tables"], keyof {
                tableName?: string | undefined;
                row?: Uint8Array[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_48 in Exclude<keyof I_1["oneOffQueryResponse"], keyof OneOffQueryResponse>]: never; }) | undefined;
    } & { [K_49 in Exclude<keyof I_1, keyof Message>]: never; }>(object: I_1): Message;
};
export declare const IdentityToken: {
    encode(message: IdentityToken, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): IdentityToken;
    fromJSON(object: any): IdentityToken;
    toJSON(message: IdentityToken): unknown;
    create<I extends {
        identity?: Uint8Array | undefined;
        token?: string | undefined;
        address?: Uint8Array | undefined;
    } & {
        identity?: Uint8Array | undefined;
        token?: string | undefined;
        address?: Uint8Array | undefined;
    } & { [K in Exclude<keyof I, keyof IdentityToken>]: never; }>(base?: I | undefined): IdentityToken;
    fromPartial<I_1 extends {
        identity?: Uint8Array | undefined;
        token?: string | undefined;
        address?: Uint8Array | undefined;
    } & {
        identity?: Uint8Array | undefined;
        token?: string | undefined;
        address?: Uint8Array | undefined;
    } & { [K_1 in Exclude<keyof I_1, keyof IdentityToken>]: never; }>(object: I_1): IdentityToken;
};
export declare const FunctionCall: {
    encode(message: FunctionCall, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): FunctionCall;
    fromJSON(object: any): FunctionCall;
    toJSON(message: FunctionCall): unknown;
    create<I extends {
        reducer?: string | undefined;
        argBytes?: Uint8Array | undefined;
    } & {
        reducer?: string | undefined;
        argBytes?: Uint8Array | undefined;
    } & { [K in Exclude<keyof I, keyof FunctionCall>]: never; }>(base?: I | undefined): FunctionCall;
    fromPartial<I_1 extends {
        reducer?: string | undefined;
        argBytes?: Uint8Array | undefined;
    } & {
        reducer?: string | undefined;
        argBytes?: Uint8Array | undefined;
    } & { [K_1 in Exclude<keyof I_1, keyof FunctionCall>]: never; }>(object: I_1): FunctionCall;
};
export declare const Subscribe: {
    encode(message: Subscribe, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): Subscribe;
    fromJSON(object: any): Subscribe;
    toJSON(message: Subscribe): unknown;
    create<I extends {
        queryStrings?: string[] | undefined;
    } & {
        queryStrings?: (string[] & string[] & { [K in Exclude<keyof I["queryStrings"], keyof string[]>]: never; }) | undefined;
    } & { [K_1 in Exclude<keyof I, "queryStrings">]: never; }>(base?: I | undefined): Subscribe;
    fromPartial<I_1 extends {
        queryStrings?: string[] | undefined;
    } & {
        queryStrings?: (string[] & string[] & { [K_2 in Exclude<keyof I_1["queryStrings"], keyof string[]>]: never; }) | undefined;
    } & { [K_3 in Exclude<keyof I_1, "queryStrings">]: never; }>(object: I_1): Subscribe;
};
export declare const Event: {
    encode(message: Event, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): Event;
    fromJSON(object: any): Event;
    toJSON(message: Event): unknown;
    create<I extends {
        timestamp?: number | undefined;
        callerIdentity?: Uint8Array | undefined;
        functionCall?: {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } | undefined;
        status?: Event_Status | undefined;
        message?: string | undefined;
        energyQuantaUsed?: number | undefined;
        hostExecutionDurationMicros?: number | undefined;
        callerAddress?: Uint8Array | undefined;
    } & {
        timestamp?: number | undefined;
        callerIdentity?: Uint8Array | undefined;
        functionCall?: ({
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & { [K in Exclude<keyof I["functionCall"], keyof FunctionCall>]: never; }) | undefined;
        status?: Event_Status | undefined;
        message?: string | undefined;
        energyQuantaUsed?: number | undefined;
        hostExecutionDurationMicros?: number | undefined;
        callerAddress?: Uint8Array | undefined;
    } & { [K_1 in Exclude<keyof I, keyof Event>]: never; }>(base?: I | undefined): Event;
    fromPartial<I_1 extends {
        timestamp?: number | undefined;
        callerIdentity?: Uint8Array | undefined;
        functionCall?: {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } | undefined;
        status?: Event_Status | undefined;
        message?: string | undefined;
        energyQuantaUsed?: number | undefined;
        hostExecutionDurationMicros?: number | undefined;
        callerAddress?: Uint8Array | undefined;
    } & {
        timestamp?: number | undefined;
        callerIdentity?: Uint8Array | undefined;
        functionCall?: ({
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & {
            reducer?: string | undefined;
            argBytes?: Uint8Array | undefined;
        } & { [K_2 in Exclude<keyof I_1["functionCall"], keyof FunctionCall>]: never; }) | undefined;
        status?: Event_Status | undefined;
        message?: string | undefined;
        energyQuantaUsed?: number | undefined;
        hostExecutionDurationMicros?: number | undefined;
        callerAddress?: Uint8Array | undefined;
    } & { [K_3 in Exclude<keyof I_1, keyof Event>]: never; }>(object: I_1): Event;
};
export declare const SubscriptionUpdate: {
    encode(message: SubscriptionUpdate, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): SubscriptionUpdate;
    fromJSON(object: any): SubscriptionUpdate;
    toJSON(message: SubscriptionUpdate): unknown;
    create<I extends {
        tableUpdates?: {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[] | undefined;
    } & {
        tableUpdates?: ({
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[] & ({
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        } & {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: ({
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] & ({
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            } & {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            } & { [K in Exclude<keyof I["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_1 in Exclude<keyof I["tableUpdates"][number]["tableRowOperations"], keyof {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_2 in Exclude<keyof I["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_3 in Exclude<keyof I["tableUpdates"], keyof {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_4 in Exclude<keyof I, "tableUpdates">]: never; }>(base?: I | undefined): SubscriptionUpdate;
    fromPartial<I_1 extends {
        tableUpdates?: {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[] | undefined;
    } & {
        tableUpdates?: ({
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[] & ({
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        } & {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: ({
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] & ({
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            } & {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            } & { [K_5 in Exclude<keyof I_1["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_6 in Exclude<keyof I_1["tableUpdates"][number]["tableRowOperations"], keyof {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_7 in Exclude<keyof I_1["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_8 in Exclude<keyof I_1["tableUpdates"], keyof {
            tableId?: number | undefined;
            tableName?: string | undefined;
            tableRowOperations?: {
                op?: TableRowOperation_OperationType | undefined;
                row?: Uint8Array | undefined;
            }[] | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_9 in Exclude<keyof I_1, "tableUpdates">]: never; }>(object: I_1): SubscriptionUpdate;
};
export declare const TableUpdate: {
    encode(message: TableUpdate, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): TableUpdate;
    fromJSON(object: any): TableUpdate;
    toJSON(message: TableUpdate): unknown;
    create<I extends {
        tableId?: number | undefined;
        tableName?: string | undefined;
        tableRowOperations?: {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[] | undefined;
    } & {
        tableId?: number | undefined;
        tableName?: string | undefined;
        tableRowOperations?: ({
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[] & ({
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        } & {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        } & { [K in Exclude<keyof I["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_1 in Exclude<keyof I["tableRowOperations"], keyof {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_2 in Exclude<keyof I, keyof TableUpdate>]: never; }>(base?: I | undefined): TableUpdate;
    fromPartial<I_1 extends {
        tableId?: number | undefined;
        tableName?: string | undefined;
        tableRowOperations?: {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[] | undefined;
    } & {
        tableId?: number | undefined;
        tableName?: string | undefined;
        tableRowOperations?: ({
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[] & ({
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        } & {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        } & { [K_3 in Exclude<keyof I_1["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_4 in Exclude<keyof I_1["tableRowOperations"], keyof {
            op?: TableRowOperation_OperationType | undefined;
            row?: Uint8Array | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_5 in Exclude<keyof I_1, keyof TableUpdate>]: never; }>(object: I_1): TableUpdate;
};
export declare const TableRowOperation: {
    encode(message: TableRowOperation, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): TableRowOperation;
    fromJSON(object: any): TableRowOperation;
    toJSON(message: TableRowOperation): unknown;
    create<I extends {
        op?: TableRowOperation_OperationType | undefined;
        row?: Uint8Array | undefined;
    } & {
        op?: TableRowOperation_OperationType | undefined;
        row?: Uint8Array | undefined;
    } & { [K in Exclude<keyof I, keyof TableRowOperation>]: never; }>(base?: I | undefined): TableRowOperation;
    fromPartial<I_1 extends {
        op?: TableRowOperation_OperationType | undefined;
        row?: Uint8Array | undefined;
    } & {
        op?: TableRowOperation_OperationType | undefined;
        row?: Uint8Array | undefined;
    } & { [K_1 in Exclude<keyof I_1, keyof TableRowOperation>]: never; }>(object: I_1): TableRowOperation;
};
export declare const TransactionUpdate: {
    encode(message: TransactionUpdate, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): TransactionUpdate;
    fromJSON(object: any): TransactionUpdate;
    toJSON(message: TransactionUpdate): unknown;
    create<I extends {
        event?: {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } | undefined;
        subscriptionUpdate?: {
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } | undefined;
    } & {
        event?: ({
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: ({
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & { [K in Exclude<keyof I["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & { [K_1 in Exclude<keyof I["event"], keyof Event>]: never; }) | undefined;
        subscriptionUpdate?: ({
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } & {
            tableUpdates?: ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] & ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            } & {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] & ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & { [K_2 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_3 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_4 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_5 in Exclude<keyof I["subscriptionUpdate"]["tableUpdates"], keyof {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_6 in Exclude<keyof I["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
    } & { [K_7 in Exclude<keyof I, keyof TransactionUpdate>]: never; }>(base?: I | undefined): TransactionUpdate;
    fromPartial<I_1 extends {
        event?: {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } | undefined;
        subscriptionUpdate?: {
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } | undefined;
    } & {
        event?: ({
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & {
            timestamp?: number | undefined;
            callerIdentity?: Uint8Array | undefined;
            functionCall?: ({
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & {
                reducer?: string | undefined;
                argBytes?: Uint8Array | undefined;
            } & { [K_8 in Exclude<keyof I_1["event"]["functionCall"], keyof FunctionCall>]: never; }) | undefined;
            status?: Event_Status | undefined;
            message?: string | undefined;
            energyQuantaUsed?: number | undefined;
            hostExecutionDurationMicros?: number | undefined;
            callerAddress?: Uint8Array | undefined;
        } & { [K_9 in Exclude<keyof I_1["event"], keyof Event>]: never; }) | undefined;
        subscriptionUpdate?: ({
            tableUpdates?: {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] | undefined;
        } & {
            tableUpdates?: ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[] & ({
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            } & {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] & ({
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                } & { [K_10 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"][number], keyof TableRowOperation>]: never; })[] & { [K_11 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number]["tableRowOperations"], keyof {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[]>]: never; }) | undefined;
            } & { [K_12 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"][number], keyof TableUpdate>]: never; })[] & { [K_13 in Exclude<keyof I_1["subscriptionUpdate"]["tableUpdates"], keyof {
                tableId?: number | undefined;
                tableName?: string | undefined;
                tableRowOperations?: {
                    op?: TableRowOperation_OperationType | undefined;
                    row?: Uint8Array | undefined;
                }[] | undefined;
            }[]>]: never; }) | undefined;
        } & { [K_14 in Exclude<keyof I_1["subscriptionUpdate"], "tableUpdates">]: never; }) | undefined;
    } & { [K_15 in Exclude<keyof I_1, keyof TransactionUpdate>]: never; }>(object: I_1): TransactionUpdate;
};
export declare const OneOffQuery: {
    encode(message: OneOffQuery, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): OneOffQuery;
    fromJSON(object: any): OneOffQuery;
    toJSON(message: OneOffQuery): unknown;
    create<I extends {
        messageId?: Uint8Array | undefined;
        queryString?: string | undefined;
    } & {
        messageId?: Uint8Array | undefined;
        queryString?: string | undefined;
    } & { [K in Exclude<keyof I, keyof OneOffQuery>]: never; }>(base?: I | undefined): OneOffQuery;
    fromPartial<I_1 extends {
        messageId?: Uint8Array | undefined;
        queryString?: string | undefined;
    } & {
        messageId?: Uint8Array | undefined;
        queryString?: string | undefined;
    } & { [K_1 in Exclude<keyof I_1, keyof OneOffQuery>]: never; }>(object: I_1): OneOffQuery;
};
export declare const OneOffQueryResponse: {
    encode(message: OneOffQueryResponse, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): OneOffQueryResponse;
    fromJSON(object: any): OneOffQueryResponse;
    toJSON(message: OneOffQueryResponse): unknown;
    create<I extends {
        messageId?: Uint8Array | undefined;
        error?: string | undefined;
        tables?: {
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[] | undefined;
    } & {
        messageId?: Uint8Array | undefined;
        error?: string | undefined;
        tables?: ({
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[] & ({
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        } & {
            tableName?: string | undefined;
            row?: (Uint8Array[] & Uint8Array[] & { [K in Exclude<keyof I["tables"][number]["row"], keyof Uint8Array[]>]: never; }) | undefined;
        } & { [K_1 in Exclude<keyof I["tables"][number], keyof OneOffTable>]: never; })[] & { [K_2 in Exclude<keyof I["tables"], keyof {
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_3 in Exclude<keyof I, keyof OneOffQueryResponse>]: never; }>(base?: I | undefined): OneOffQueryResponse;
    fromPartial<I_1 extends {
        messageId?: Uint8Array | undefined;
        error?: string | undefined;
        tables?: {
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[] | undefined;
    } & {
        messageId?: Uint8Array | undefined;
        error?: string | undefined;
        tables?: ({
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[] & ({
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        } & {
            tableName?: string | undefined;
            row?: (Uint8Array[] & Uint8Array[] & { [K_4 in Exclude<keyof I_1["tables"][number]["row"], keyof Uint8Array[]>]: never; }) | undefined;
        } & { [K_5 in Exclude<keyof I_1["tables"][number], keyof OneOffTable>]: never; })[] & { [K_6 in Exclude<keyof I_1["tables"], keyof {
            tableName?: string | undefined;
            row?: Uint8Array[] | undefined;
        }[]>]: never; }) | undefined;
    } & { [K_7 in Exclude<keyof I_1, keyof OneOffQueryResponse>]: never; }>(object: I_1): OneOffQueryResponse;
};
export declare const OneOffTable: {
    encode(message: OneOffTable, writer?: _m0.Writer): _m0.Writer;
    decode(input: _m0.Reader | Uint8Array, length?: number): OneOffTable;
    fromJSON(object: any): OneOffTable;
    toJSON(message: OneOffTable): unknown;
    create<I extends {
        tableName?: string | undefined;
        row?: Uint8Array[] | undefined;
    } & {
        tableName?: string | undefined;
        row?: (Uint8Array[] & Uint8Array[] & { [K in Exclude<keyof I["row"], keyof Uint8Array[]>]: never; }) | undefined;
    } & { [K_1 in Exclude<keyof I, keyof OneOffTable>]: never; }>(base?: I | undefined): OneOffTable;
    fromPartial<I_1 extends {
        tableName?: string | undefined;
        row?: Uint8Array[] | undefined;
    } & {
        tableName?: string | undefined;
        row?: (Uint8Array[] & Uint8Array[] & { [K_2 in Exclude<keyof I_1["row"], keyof Uint8Array[]>]: never; }) | undefined;
    } & { [K_3 in Exclude<keyof I_1, keyof OneOffTable>]: never; }>(object: I_1): OneOffTable;
};
type Builtin = Date | Function | Uint8Array | string | number | boolean | undefined;
export type DeepPartial<T> = T extends Builtin ? T : T extends Array<infer U> ? Array<DeepPartial<U>> : T extends ReadonlyArray<infer U> ? ReadonlyArray<DeepPartial<U>> : T extends {} ? {
    [K in keyof T]?: DeepPartial<T[K]>;
} : Partial<T>;
type KeysOfUnion<T> = T extends T ? keyof T : never;
export type Exact<P, I extends P> = P extends Builtin ? P : P & {
    [K in keyof P]: Exact<P[K], I[K]>;
} & {
    [K in Exclude<keyof I, KeysOfUnion<P>>]: never;
};
export {};
