/* eslint-disable */
import Long from "long";
import _m0 from "protobufjs/minimal";

export const protobufPackage = "client_api";

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

export enum Event_Status {
  committed = 0,
  failed = 1,
  out_of_energy = 2,
  UNRECOGNIZED = -1,
}

export function event_StatusFromJSON(object: any): Event_Status {
  switch (object) {
    case 0:
    case "committed":
      return Event_Status.committed;
    case 1:
    case "failed":
      return Event_Status.failed;
    case 2:
    case "out_of_energy":
      return Event_Status.out_of_energy;
    case -1:
    case "UNRECOGNIZED":
    default:
      return Event_Status.UNRECOGNIZED;
  }
}

export function event_StatusToJSON(object: Event_Status): string {
  switch (object) {
    case Event_Status.committed:
      return "committed";
    case Event_Status.failed:
      return "failed";
    case Event_Status.out_of_energy:
      return "out_of_energy";
    case Event_Status.UNRECOGNIZED:
    default:
      return "UNRECOGNIZED";
  }
}

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
 * / - `row_pk` is a hash of the row computed by the database. As of 2023-06-13, even for
 * /            tables with a `#[primarykey]` annotation on one column, the `row_pk` is not
 * /            that primary key.
 * /
 * / - `row` is the row itself, encoded as BSATN.
 */
export interface TableRowOperation {
  op: TableRowOperation_OperationType;
  rowPk: Uint8Array;
  row: Uint8Array;
}

export enum TableRowOperation_OperationType {
  DELETE = 0,
  INSERT = 1,
  UNRECOGNIZED = -1,
}

export function tableRowOperation_OperationTypeFromJSON(
  object: any
): TableRowOperation_OperationType {
  switch (object) {
    case 0:
    case "DELETE":
      return TableRowOperation_OperationType.DELETE;
    case 1:
    case "INSERT":
      return TableRowOperation_OperationType.INSERT;
    case -1:
    case "UNRECOGNIZED":
    default:
      return TableRowOperation_OperationType.UNRECOGNIZED;
  }
}

export function tableRowOperation_OperationTypeToJSON(
  object: TableRowOperation_OperationType
): string {
  switch (object) {
    case TableRowOperation_OperationType.DELETE:
      return "DELETE";
    case TableRowOperation_OperationType.INSERT:
      return "INSERT";
    case TableRowOperation_OperationType.UNRECOGNIZED:
    default:
      return "UNRECOGNIZED";
  }
}

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

function createBaseMessage(): Message {
  return {
    functionCall: undefined,
    subscriptionUpdate: undefined,
    event: undefined,
    transactionUpdate: undefined,
    identityToken: undefined,
    subscribe: undefined,
    oneOffQuery: undefined,
    oneOffQueryResponse: undefined,
  };
}

export const Message = {
  encode(
    message: Message,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.functionCall !== undefined) {
      FunctionCall.encode(
        message.functionCall,
        writer.uint32(10).fork()
      ).ldelim();
    }
    if (message.subscriptionUpdate !== undefined) {
      SubscriptionUpdate.encode(
        message.subscriptionUpdate,
        writer.uint32(18).fork()
      ).ldelim();
    }
    if (message.event !== undefined) {
      Event.encode(message.event, writer.uint32(26).fork()).ldelim();
    }
    if (message.transactionUpdate !== undefined) {
      TransactionUpdate.encode(
        message.transactionUpdate,
        writer.uint32(34).fork()
      ).ldelim();
    }
    if (message.identityToken !== undefined) {
      IdentityToken.encode(
        message.identityToken,
        writer.uint32(42).fork()
      ).ldelim();
    }
    if (message.subscribe !== undefined) {
      Subscribe.encode(message.subscribe, writer.uint32(50).fork()).ldelim();
    }
    if (message.oneOffQuery !== undefined) {
      OneOffQuery.encode(
        message.oneOffQuery,
        writer.uint32(58).fork()
      ).ldelim();
    }
    if (message.oneOffQueryResponse !== undefined) {
      OneOffQueryResponse.encode(
        message.oneOffQueryResponse,
        writer.uint32(66).fork()
      ).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Message {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseMessage();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.functionCall = FunctionCall.decode(reader, reader.uint32());
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.subscriptionUpdate = SubscriptionUpdate.decode(
            reader,
            reader.uint32()
          );
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.event = Event.decode(reader, reader.uint32());
          continue;
        case 4:
          if (tag !== 34) {
            break;
          }

          message.transactionUpdate = TransactionUpdate.decode(
            reader,
            reader.uint32()
          );
          continue;
        case 5:
          if (tag !== 42) {
            break;
          }

          message.identityToken = IdentityToken.decode(reader, reader.uint32());
          continue;
        case 6:
          if (tag !== 50) {
            break;
          }

          message.subscribe = Subscribe.decode(reader, reader.uint32());
          continue;
        case 7:
          if (tag !== 58) {
            break;
          }

          message.oneOffQuery = OneOffQuery.decode(reader, reader.uint32());
          continue;
        case 8:
          if (tag !== 66) {
            break;
          }

          message.oneOffQueryResponse = OneOffQueryResponse.decode(
            reader,
            reader.uint32()
          );
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): Message {
    return {
      functionCall: isSet(object.functionCall)
        ? FunctionCall.fromJSON(object.functionCall)
        : undefined,
      subscriptionUpdate: isSet(object.subscriptionUpdate)
        ? SubscriptionUpdate.fromJSON(object.subscriptionUpdate)
        : undefined,
      event: isSet(object.event) ? Event.fromJSON(object.event) : undefined,
      transactionUpdate: isSet(object.transactionUpdate)
        ? TransactionUpdate.fromJSON(object.transactionUpdate)
        : undefined,
      identityToken: isSet(object.identityToken)
        ? IdentityToken.fromJSON(object.identityToken)
        : undefined,
      subscribe: isSet(object.subscribe)
        ? Subscribe.fromJSON(object.subscribe)
        : undefined,
      oneOffQuery: isSet(object.oneOffQuery)
        ? OneOffQuery.fromJSON(object.oneOffQuery)
        : undefined,
      oneOffQueryResponse: isSet(object.oneOffQueryResponse)
        ? OneOffQueryResponse.fromJSON(object.oneOffQueryResponse)
        : undefined,
    };
  },

  toJSON(message: Message): unknown {
    const obj: any = {};
    if (message.functionCall !== undefined) {
      obj.functionCall = FunctionCall.toJSON(message.functionCall);
    }
    if (message.subscriptionUpdate !== undefined) {
      obj.subscriptionUpdate = SubscriptionUpdate.toJSON(
        message.subscriptionUpdate
      );
    }
    if (message.event !== undefined) {
      obj.event = Event.toJSON(message.event);
    }
    if (message.transactionUpdate !== undefined) {
      obj.transactionUpdate = TransactionUpdate.toJSON(
        message.transactionUpdate
      );
    }
    if (message.identityToken !== undefined) {
      obj.identityToken = IdentityToken.toJSON(message.identityToken);
    }
    if (message.subscribe !== undefined) {
      obj.subscribe = Subscribe.toJSON(message.subscribe);
    }
    if (message.oneOffQuery !== undefined) {
      obj.oneOffQuery = OneOffQuery.toJSON(message.oneOffQuery);
    }
    if (message.oneOffQueryResponse !== undefined) {
      obj.oneOffQueryResponse = OneOffQueryResponse.toJSON(
        message.oneOffQueryResponse
      );
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<Message>, I>>(base?: I): Message {
    return Message.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<Message>, I>>(object: I): Message {
    const message = createBaseMessage();
    message.functionCall =
      object.functionCall !== undefined && object.functionCall !== null
        ? FunctionCall.fromPartial(object.functionCall)
        : undefined;
    message.subscriptionUpdate =
      object.subscriptionUpdate !== undefined &&
      object.subscriptionUpdate !== null
        ? SubscriptionUpdate.fromPartial(object.subscriptionUpdate)
        : undefined;
    message.event =
      object.event !== undefined && object.event !== null
        ? Event.fromPartial(object.event)
        : undefined;
    message.transactionUpdate =
      object.transactionUpdate !== undefined &&
      object.transactionUpdate !== null
        ? TransactionUpdate.fromPartial(object.transactionUpdate)
        : undefined;
    message.identityToken =
      object.identityToken !== undefined && object.identityToken !== null
        ? IdentityToken.fromPartial(object.identityToken)
        : undefined;
    message.subscribe =
      object.subscribe !== undefined && object.subscribe !== null
        ? Subscribe.fromPartial(object.subscribe)
        : undefined;
    message.oneOffQuery =
      object.oneOffQuery !== undefined && object.oneOffQuery !== null
        ? OneOffQuery.fromPartial(object.oneOffQuery)
        : undefined;
    message.oneOffQueryResponse =
      object.oneOffQueryResponse !== undefined &&
      object.oneOffQueryResponse !== null
        ? OneOffQueryResponse.fromPartial(object.oneOffQueryResponse)
        : undefined;
    return message;
  },
};

function createBaseIdentityToken(): IdentityToken {
  return { identity: new Uint8Array(0), token: "", address: new Uint8Array(0) };
}

export const IdentityToken = {
  encode(
    message: IdentityToken,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.identity.length !== 0) {
      writer.uint32(10).bytes(message.identity);
    }
    if (message.token !== "") {
      writer.uint32(18).string(message.token);
    }
    if (message.address.length !== 0) {
      writer.uint32(26).bytes(message.address);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): IdentityToken {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseIdentityToken();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.identity = reader.bytes();
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.token = reader.string();
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.address = reader.bytes();
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): IdentityToken {
    return {
      identity: isSet(object.identity)
        ? bytesFromBase64(object.identity)
        : new Uint8Array(0),
      token: isSet(object.token) ? String(object.token) : "",
      address: isSet(object.address)
        ? bytesFromBase64(object.address)
        : new Uint8Array(0),
    };
  },

  toJSON(message: IdentityToken): unknown {
    const obj: any = {};
    if (message.identity.length !== 0) {
      obj.identity = base64FromBytes(message.identity);
    }
    if (message.token !== "") {
      obj.token = message.token;
    }
    if (message.address.length !== 0) {
      obj.address = base64FromBytes(message.address);
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<IdentityToken>, I>>(
    base?: I
  ): IdentityToken {
    return IdentityToken.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<IdentityToken>, I>>(
    object: I
  ): IdentityToken {
    const message = createBaseIdentityToken();
    message.identity = object.identity ?? new Uint8Array(0);
    message.token = object.token ?? "";
    message.address = object.address ?? new Uint8Array(0);
    return message;
  },
};

function createBaseFunctionCall(): FunctionCall {
  return { reducer: "", argBytes: new Uint8Array(0) };
}

export const FunctionCall = {
  encode(
    message: FunctionCall,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.reducer !== "") {
      writer.uint32(10).string(message.reducer);
    }
    if (message.argBytes.length !== 0) {
      writer.uint32(18).bytes(message.argBytes);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): FunctionCall {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseFunctionCall();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.reducer = reader.string();
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.argBytes = reader.bytes();
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): FunctionCall {
    return {
      reducer: isSet(object.reducer) ? String(object.reducer) : "",
      argBytes: isSet(object.argBytes)
        ? bytesFromBase64(object.argBytes)
        : new Uint8Array(0),
    };
  },

  toJSON(message: FunctionCall): unknown {
    const obj: any = {};
    if (message.reducer !== "") {
      obj.reducer = message.reducer;
    }
    if (message.argBytes.length !== 0) {
      obj.argBytes = base64FromBytes(message.argBytes);
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<FunctionCall>, I>>(
    base?: I
  ): FunctionCall {
    return FunctionCall.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<FunctionCall>, I>>(
    object: I
  ): FunctionCall {
    const message = createBaseFunctionCall();
    message.reducer = object.reducer ?? "";
    message.argBytes = object.argBytes ?? new Uint8Array(0);
    return message;
  },
};

function createBaseSubscribe(): Subscribe {
  return { queryStrings: [] };
}

export const Subscribe = {
  encode(
    message: Subscribe,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    for (const v of message.queryStrings) {
      writer.uint32(10).string(v!);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Subscribe {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseSubscribe();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.queryStrings.push(reader.string());
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): Subscribe {
    return {
      queryStrings: Array.isArray(object?.queryStrings)
        ? object.queryStrings.map((e: any) => String(e))
        : [],
    };
  },

  toJSON(message: Subscribe): unknown {
    const obj: any = {};
    if (message.queryStrings?.length) {
      obj.queryStrings = message.queryStrings;
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<Subscribe>, I>>(base?: I): Subscribe {
    return Subscribe.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<Subscribe>, I>>(
    object: I
  ): Subscribe {
    const message = createBaseSubscribe();
    message.queryStrings = object.queryStrings?.map((e) => e) || [];
    return message;
  },
};

function createBaseEvent(): Event {
  return {
    timestamp: 0,
    callerIdentity: new Uint8Array(0),
    functionCall: undefined,
    status: 0,
    message: "",
    energyQuantaUsed: 0,
    hostExecutionDurationMicros: 0,
    callerAddress: new Uint8Array(0),
  };
}

export const Event = {
  encode(message: Event, writer: _m0.Writer = _m0.Writer.create()): _m0.Writer {
    if (message.timestamp !== 0) {
      writer.uint32(8).uint64(message.timestamp);
    }
    if (message.callerIdentity.length !== 0) {
      writer.uint32(18).bytes(message.callerIdentity);
    }
    if (message.functionCall !== undefined) {
      FunctionCall.encode(
        message.functionCall,
        writer.uint32(26).fork()
      ).ldelim();
    }
    if (message.status !== 0) {
      writer.uint32(32).int32(message.status);
    }
    if (message.message !== "") {
      writer.uint32(42).string(message.message);
    }
    if (message.energyQuantaUsed !== 0) {
      writer.uint32(48).int64(message.energyQuantaUsed);
    }
    if (message.hostExecutionDurationMicros !== 0) {
      writer.uint32(56).uint64(message.hostExecutionDurationMicros);
    }
    if (message.callerAddress.length !== 0) {
      writer.uint32(66).bytes(message.callerAddress);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Event {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseEvent();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 8) {
            break;
          }

          message.timestamp = longToNumber(reader.uint64() as Long);
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.callerIdentity = reader.bytes();
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.functionCall = FunctionCall.decode(reader, reader.uint32());
          continue;
        case 4:
          if (tag !== 32) {
            break;
          }

          message.status = reader.int32() as any;
          continue;
        case 5:
          if (tag !== 42) {
            break;
          }

          message.message = reader.string();
          continue;
        case 6:
          if (tag !== 48) {
            break;
          }

          message.energyQuantaUsed = longToNumber(reader.int64() as Long);
          continue;
        case 7:
          if (tag !== 56) {
            break;
          }

          message.hostExecutionDurationMicros = longToNumber(
            reader.uint64() as Long
          );
          continue;
        case 8:
          if (tag !== 66) {
            break;
          }

          message.callerAddress = reader.bytes();
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): Event {
    return {
      timestamp: isSet(object.timestamp) ? Number(object.timestamp) : 0,
      callerIdentity: isSet(object.callerIdentity)
        ? bytesFromBase64(object.callerIdentity)
        : new Uint8Array(0),
      functionCall: isSet(object.functionCall)
        ? FunctionCall.fromJSON(object.functionCall)
        : undefined,
      status: isSet(object.status) ? event_StatusFromJSON(object.status) : 0,
      message: isSet(object.message) ? String(object.message) : "",
      energyQuantaUsed: isSet(object.energyQuantaUsed)
        ? Number(object.energyQuantaUsed)
        : 0,
      hostExecutionDurationMicros: isSet(object.hostExecutionDurationMicros)
        ? Number(object.hostExecutionDurationMicros)
        : 0,
      callerAddress: isSet(object.callerAddress)
        ? bytesFromBase64(object.callerAddress)
        : new Uint8Array(0),
    };
  },

  toJSON(message: Event): unknown {
    const obj: any = {};
    if (message.timestamp !== 0) {
      obj.timestamp = Math.round(message.timestamp);
    }
    if (message.callerIdentity.length !== 0) {
      obj.callerIdentity = base64FromBytes(message.callerIdentity);
    }
    if (message.functionCall !== undefined) {
      obj.functionCall = FunctionCall.toJSON(message.functionCall);
    }
    if (message.status !== 0) {
      obj.status = event_StatusToJSON(message.status);
    }
    if (message.message !== "") {
      obj.message = message.message;
    }
    if (message.energyQuantaUsed !== 0) {
      obj.energyQuantaUsed = Math.round(message.energyQuantaUsed);
    }
    if (message.hostExecutionDurationMicros !== 0) {
      obj.hostExecutionDurationMicros = Math.round(
        message.hostExecutionDurationMicros
      );
    }
    if (message.callerAddress.length !== 0) {
      obj.callerAddress = base64FromBytes(message.callerAddress);
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<Event>, I>>(base?: I): Event {
    return Event.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<Event>, I>>(object: I): Event {
    const message = createBaseEvent();
    message.timestamp = object.timestamp ?? 0;
    message.callerIdentity = object.callerIdentity ?? new Uint8Array(0);
    message.functionCall =
      object.functionCall !== undefined && object.functionCall !== null
        ? FunctionCall.fromPartial(object.functionCall)
        : undefined;
    message.status = object.status ?? 0;
    message.message = object.message ?? "";
    message.energyQuantaUsed = object.energyQuantaUsed ?? 0;
    message.hostExecutionDurationMicros =
      object.hostExecutionDurationMicros ?? 0;
    message.callerAddress = object.callerAddress ?? new Uint8Array(0);
    return message;
  },
};

function createBaseSubscriptionUpdate(): SubscriptionUpdate {
  return { tableUpdates: [] };
}

export const SubscriptionUpdate = {
  encode(
    message: SubscriptionUpdate,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    for (const v of message.tableUpdates) {
      TableUpdate.encode(v!, writer.uint32(10).fork()).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): SubscriptionUpdate {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseSubscriptionUpdate();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.tableUpdates.push(
            TableUpdate.decode(reader, reader.uint32())
          );
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): SubscriptionUpdate {
    return {
      tableUpdates: Array.isArray(object?.tableUpdates)
        ? object.tableUpdates.map((e: any) => TableUpdate.fromJSON(e))
        : [],
    };
  },

  toJSON(message: SubscriptionUpdate): unknown {
    const obj: any = {};
    if (message.tableUpdates?.length) {
      obj.tableUpdates = message.tableUpdates.map((e) => TableUpdate.toJSON(e));
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<SubscriptionUpdate>, I>>(
    base?: I
  ): SubscriptionUpdate {
    return SubscriptionUpdate.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<SubscriptionUpdate>, I>>(
    object: I
  ): SubscriptionUpdate {
    const message = createBaseSubscriptionUpdate();
    message.tableUpdates =
      object.tableUpdates?.map((e) => TableUpdate.fromPartial(e)) || [];
    return message;
  },
};

function createBaseTableUpdate(): TableUpdate {
  return { tableId: 0, tableName: "", tableRowOperations: [] };
}

export const TableUpdate = {
  encode(
    message: TableUpdate,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.tableId !== 0) {
      writer.uint32(8).uint32(message.tableId);
    }
    if (message.tableName !== "") {
      writer.uint32(18).string(message.tableName);
    }
    for (const v of message.tableRowOperations) {
      TableRowOperation.encode(v!, writer.uint32(26).fork()).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): TableUpdate {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseTableUpdate();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 8) {
            break;
          }

          message.tableId = reader.uint32();
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.tableName = reader.string();
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.tableRowOperations.push(
            TableRowOperation.decode(reader, reader.uint32())
          );
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): TableUpdate {
    return {
      tableId: isSet(object.tableId) ? Number(object.tableId) : 0,
      tableName: isSet(object.tableName) ? String(object.tableName) : "",
      tableRowOperations: Array.isArray(object?.tableRowOperations)
        ? object.tableRowOperations.map((e: any) =>
            TableRowOperation.fromJSON(e)
          )
        : [],
    };
  },

  toJSON(message: TableUpdate): unknown {
    const obj: any = {};
    if (message.tableId !== 0) {
      obj.tableId = Math.round(message.tableId);
    }
    if (message.tableName !== "") {
      obj.tableName = message.tableName;
    }
    if (message.tableRowOperations?.length) {
      obj.tableRowOperations = message.tableRowOperations.map((e) =>
        TableRowOperation.toJSON(e)
      );
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<TableUpdate>, I>>(base?: I): TableUpdate {
    return TableUpdate.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<TableUpdate>, I>>(
    object: I
  ): TableUpdate {
    const message = createBaseTableUpdate();
    message.tableId = object.tableId ?? 0;
    message.tableName = object.tableName ?? "";
    message.tableRowOperations =
      object.tableRowOperations?.map((e) => TableRowOperation.fromPartial(e)) ||
      [];
    return message;
  },
};

function createBaseTableRowOperation(): TableRowOperation {
  return { op: 0, rowPk: new Uint8Array(0), row: new Uint8Array(0) };
}

export const TableRowOperation = {
  encode(
    message: TableRowOperation,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.op !== 0) {
      writer.uint32(8).int32(message.op);
    }
    if (message.rowPk.length !== 0) {
      writer.uint32(18).bytes(message.rowPk);
    }
    if (message.row.length !== 0) {
      writer.uint32(26).bytes(message.row);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): TableRowOperation {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseTableRowOperation();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 8) {
            break;
          }

          message.op = reader.int32() as any;
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.rowPk = reader.bytes();
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.row = reader.bytes();
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): TableRowOperation {
    return {
      op: isSet(object.op)
        ? tableRowOperation_OperationTypeFromJSON(object.op)
        : 0,
      rowPk: isSet(object.rowPk)
        ? bytesFromBase64(object.rowPk)
        : new Uint8Array(0),
      row: isSet(object.row) ? bytesFromBase64(object.row) : new Uint8Array(0),
    };
  },

  toJSON(message: TableRowOperation): unknown {
    const obj: any = {};
    if (message.op !== 0) {
      obj.op = tableRowOperation_OperationTypeToJSON(message.op);
    }
    if (message.rowPk.length !== 0) {
      obj.rowPk = base64FromBytes(message.rowPk);
    }
    if (message.row.length !== 0) {
      obj.row = base64FromBytes(message.row);
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<TableRowOperation>, I>>(
    base?: I
  ): TableRowOperation {
    return TableRowOperation.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<TableRowOperation>, I>>(
    object: I
  ): TableRowOperation {
    const message = createBaseTableRowOperation();
    message.op = object.op ?? 0;
    message.rowPk = object.rowPk ?? new Uint8Array(0);
    message.row = object.row ?? new Uint8Array(0);
    return message;
  },
};

function createBaseTransactionUpdate(): TransactionUpdate {
  return { event: undefined, subscriptionUpdate: undefined };
}

export const TransactionUpdate = {
  encode(
    message: TransactionUpdate,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.event !== undefined) {
      Event.encode(message.event, writer.uint32(10).fork()).ldelim();
    }
    if (message.subscriptionUpdate !== undefined) {
      SubscriptionUpdate.encode(
        message.subscriptionUpdate,
        writer.uint32(18).fork()
      ).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): TransactionUpdate {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseTransactionUpdate();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.event = Event.decode(reader, reader.uint32());
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.subscriptionUpdate = SubscriptionUpdate.decode(
            reader,
            reader.uint32()
          );
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): TransactionUpdate {
    return {
      event: isSet(object.event) ? Event.fromJSON(object.event) : undefined,
      subscriptionUpdate: isSet(object.subscriptionUpdate)
        ? SubscriptionUpdate.fromJSON(object.subscriptionUpdate)
        : undefined,
    };
  },

  toJSON(message: TransactionUpdate): unknown {
    const obj: any = {};
    if (message.event !== undefined) {
      obj.event = Event.toJSON(message.event);
    }
    if (message.subscriptionUpdate !== undefined) {
      obj.subscriptionUpdate = SubscriptionUpdate.toJSON(
        message.subscriptionUpdate
      );
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<TransactionUpdate>, I>>(
    base?: I
  ): TransactionUpdate {
    return TransactionUpdate.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<TransactionUpdate>, I>>(
    object: I
  ): TransactionUpdate {
    const message = createBaseTransactionUpdate();
    message.event =
      object.event !== undefined && object.event !== null
        ? Event.fromPartial(object.event)
        : undefined;
    message.subscriptionUpdate =
      object.subscriptionUpdate !== undefined &&
      object.subscriptionUpdate !== null
        ? SubscriptionUpdate.fromPartial(object.subscriptionUpdate)
        : undefined;
    return message;
  },
};

function createBaseOneOffQuery(): OneOffQuery {
  return { messageId: new Uint8Array(0), queryString: "" };
}

export const OneOffQuery = {
  encode(
    message: OneOffQuery,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.messageId.length !== 0) {
      writer.uint32(10).bytes(message.messageId);
    }
    if (message.queryString !== "") {
      writer.uint32(18).string(message.queryString);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): OneOffQuery {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseOneOffQuery();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.messageId = reader.bytes();
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.queryString = reader.string();
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): OneOffQuery {
    return {
      messageId: isSet(object.messageId)
        ? bytesFromBase64(object.messageId)
        : new Uint8Array(0),
      queryString: isSet(object.queryString) ? String(object.queryString) : "",
    };
  },

  toJSON(message: OneOffQuery): unknown {
    const obj: any = {};
    if (message.messageId.length !== 0) {
      obj.messageId = base64FromBytes(message.messageId);
    }
    if (message.queryString !== "") {
      obj.queryString = message.queryString;
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<OneOffQuery>, I>>(base?: I): OneOffQuery {
    return OneOffQuery.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<OneOffQuery>, I>>(
    object: I
  ): OneOffQuery {
    const message = createBaseOneOffQuery();
    message.messageId = object.messageId ?? new Uint8Array(0);
    message.queryString = object.queryString ?? "";
    return message;
  },
};

function createBaseOneOffQueryResponse(): OneOffQueryResponse {
  return { messageId: new Uint8Array(0), error: "", tables: [] };
}

export const OneOffQueryResponse = {
  encode(
    message: OneOffQueryResponse,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.messageId.length !== 0) {
      writer.uint32(10).bytes(message.messageId);
    }
    if (message.error !== "") {
      writer.uint32(18).string(message.error);
    }
    for (const v of message.tables) {
      OneOffTable.encode(v!, writer.uint32(26).fork()).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): OneOffQueryResponse {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseOneOffQueryResponse();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          if (tag !== 10) {
            break;
          }

          message.messageId = reader.bytes();
          continue;
        case 2:
          if (tag !== 18) {
            break;
          }

          message.error = reader.string();
          continue;
        case 3:
          if (tag !== 26) {
            break;
          }

          message.tables.push(OneOffTable.decode(reader, reader.uint32()));
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): OneOffQueryResponse {
    return {
      messageId: isSet(object.messageId)
        ? bytesFromBase64(object.messageId)
        : new Uint8Array(0),
      error: isSet(object.error) ? String(object.error) : "",
      tables: Array.isArray(object?.tables)
        ? object.tables.map((e: any) => OneOffTable.fromJSON(e))
        : [],
    };
  },

  toJSON(message: OneOffQueryResponse): unknown {
    const obj: any = {};
    if (message.messageId.length !== 0) {
      obj.messageId = base64FromBytes(message.messageId);
    }
    if (message.error !== "") {
      obj.error = message.error;
    }
    if (message.tables?.length) {
      obj.tables = message.tables.map((e) => OneOffTable.toJSON(e));
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<OneOffQueryResponse>, I>>(
    base?: I
  ): OneOffQueryResponse {
    return OneOffQueryResponse.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<OneOffQueryResponse>, I>>(
    object: I
  ): OneOffQueryResponse {
    const message = createBaseOneOffQueryResponse();
    message.messageId = object.messageId ?? new Uint8Array(0);
    message.error = object.error ?? "";
    message.tables =
      object.tables?.map((e) => OneOffTable.fromPartial(e)) || [];
    return message;
  },
};

function createBaseOneOffTable(): OneOffTable {
  return { tableName: "", row: [] };
}

export const OneOffTable = {
  encode(
    message: OneOffTable,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.tableName !== "") {
      writer.uint32(18).string(message.tableName);
    }
    for (const v of message.row) {
      writer.uint32(34).bytes(v!);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): OneOffTable {
    const reader =
      input instanceof _m0.Reader ? input : _m0.Reader.create(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = createBaseOneOffTable();
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 2:
          if (tag !== 18) {
            break;
          }

          message.tableName = reader.string();
          continue;
        case 4:
          if (tag !== 34) {
            break;
          }

          message.row.push(reader.bytes());
          continue;
      }
      if ((tag & 7) === 4 || tag === 0) {
        break;
      }
      reader.skipType(tag & 7);
    }
    return message;
  },

  fromJSON(object: any): OneOffTable {
    return {
      tableName: isSet(object.tableName) ? String(object.tableName) : "",
      row: Array.isArray(object?.row)
        ? object.row.map((e: any) => bytesFromBase64(e))
        : [],
    };
  },

  toJSON(message: OneOffTable): unknown {
    const obj: any = {};
    if (message.tableName !== "") {
      obj.tableName = message.tableName;
    }
    if (message.row?.length) {
      obj.row = message.row.map((e) => base64FromBytes(e));
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<OneOffTable>, I>>(base?: I): OneOffTable {
    return OneOffTable.fromPartial(base ?? ({} as any));
  },
  fromPartial<I extends Exact<DeepPartial<OneOffTable>, I>>(
    object: I
  ): OneOffTable {
    const message = createBaseOneOffTable();
    message.tableName = object.tableName ?? "";
    message.row = object.row?.map((e) => e) || [];
    return message;
  },
};

declare const self: any | undefined;
declare const window: any | undefined;
declare const global: any | undefined;
const tsProtoGlobalThis: any = (() => {
  if (typeof globalThis !== "undefined") {
    return globalThis;
  }
  if (typeof self !== "undefined") {
    return self;
  }
  if (typeof window !== "undefined") {
    return window;
  }
  if (typeof global !== "undefined") {
    return global;
  }
  throw "Unable to locate global object";
})();

function bytesFromBase64(b64: string): Uint8Array {
  if (tsProtoGlobalThis.Buffer) {
    return Uint8Array.from(tsProtoGlobalThis.Buffer.from(b64, "base64"));
  } else {
    const bin = tsProtoGlobalThis.atob(b64);
    const arr = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; ++i) {
      arr[i] = bin.charCodeAt(i);
    }
    return arr;
  }
}

function base64FromBytes(arr: Uint8Array): string {
  if (tsProtoGlobalThis.Buffer) {
    return tsProtoGlobalThis.Buffer.from(arr).toString("base64");
  } else {
    const bin: string[] = [];
    arr.forEach((byte) => {
      bin.push(String.fromCharCode(byte));
    });
    return tsProtoGlobalThis.btoa(bin.join(""));
  }
}

type Builtin =
  | Date
  | Function
  | Uint8Array
  | string
  | number
  | boolean
  | undefined;

export type DeepPartial<T> = T extends Builtin
  ? T
  : T extends Array<infer U>
  ? Array<DeepPartial<U>>
  : T extends ReadonlyArray<infer U>
  ? ReadonlyArray<DeepPartial<U>>
  : T extends {}
  ? { [K in keyof T]?: DeepPartial<T[K]> }
  : Partial<T>;

type KeysOfUnion<T> = T extends T ? keyof T : never;
export type Exact<P, I extends P> = P extends Builtin
  ? P
  : P & { [K in keyof P]: Exact<P[K], I[K]> } & {
      [K in Exclude<keyof I, KeysOfUnion<P>>]: never;
    };

function longToNumber(long: Long): number {
  if (long.gt(Number.MAX_SAFE_INTEGER)) {
    throw new tsProtoGlobalThis.Error(
      "Value is larger than Number.MAX_SAFE_INTEGER"
    );
  }
  return long.toNumber();
}

if (_m0.util.Long !== Long) {
  _m0.util.Long = Long as any;
  _m0.configure();
}

function isSet(value: any): boolean {
  return value !== null && value !== undefined;
}
