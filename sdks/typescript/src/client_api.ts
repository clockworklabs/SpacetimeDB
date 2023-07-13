/* eslint-disable */
import Long from "long";
import * as _m0 from "protobufjs/minimal";

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
}

/**
 * / Received by database from client to inform of user's identity and token.
 * /
 * / Do you receive this if you provide a token when connecting, or only if you connect
 * / anonymously? Find out and document - pgoldman 2023-06-06.
 */
export interface IdentityToken {
  identity: Uint8Array;
  token: string;
}

/**
 * / Sent by client to database to request a reducer runs.
 * /
 * / `reducer` is the string name of a reducer to run.
 * /
 * / `argBytes` is the arguments to the reducer, encoded as BSATN. (Possibly as SATN if
 * /            you're in the text API? Find out and document - pgoldman 2023-06-05)
 * /
 * / SpacetimeDB models reducers as taking a single `AlgebraicValue` as an argument, which
 * / generally will be a `ProductValue` containing all of the args (except the
 * / `ReducerContext`, which is injected by the host, not provided in this API).
 * /
 * / I don't think clients will ever receive a `FunctionCall` from the database, except
 * / wrapped in an `Event` - pgoldman 2023-06-05.
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
 * /
 * / I don't think clients will ever receive a `Subscribe` from the database - pgoldman
 * / 2023-06-05.
 */
export interface Subscribe {
  queryStrings: string[];
}

/**
 * / Part of a `TransactionUpdate` received by client from database upon a reducer run.
 * /
 * / `timestamp` is the time when the reducer ran (started? finished? Find out and document
 * /             - pgoldman 2023-06-05), as microseconds since the Unix epoch.
 * /
 * / `callerIdentity` is the identity token of the user who requested the reducer
 * /                  run. (What if it's run by the database without a client request? Is
 * /                  `callerIdentity` empty? Find out and document - pgoldman 2023-06-05).
 * /
 * / `functionCall` contains the name of the reducer which ran and the arguments it
 * /                received.
 * /
 * / `status` of `committed` means that the reducer ran successfully and its changes were
 * /                         committed to the database. The rows altered in the database
 * /                         will be recorded in the parent `TransactionUpdate`'s
 * /                         `SubscriptionUpdate`.
 * /
 * / `status` of `failed` means that the reducer panicked, and any changes it attempted to
 * /                      make were rolled back.
 * /
 * / `status` of `failed` means that the reducer was interrupted due to insufficient
 * /                      energy/funds, and any changes it attempted to make were rolled
 * /                      back. (Verify this - pgoldman 2023-06-05).
 * /
 * / `message` what does it do? Find out and document - pgoldman 2023-06-05.
 * /
 * / `energy_quanta_used` and `host_execution_duration_micros` seem self-explanatory; they
 * / describe the amount of energy credits consumed by running the reducer, and how long it
 * / took to run.
 * /
 * / Do clients receive `TransactionUpdate`s / `Event`s for reducer runs which don't touch
 * / any of the client's subscribed rows? Find out and document - pgoldman 2023-06-05.
 * /
 * / Will a client ever receive an `Event` not wrapped in a `TransactionUpdate`? Possibly
 * / when `status = failed` or `status = out_of_energy`? Find out and document - pgoldman
 * / 2023-06-05.
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
 * /
 * / Can a client send `TableUpdate`s to the database to alter the database? I don't think
 * / so, but would be good to know for sure - pgoldman 2023-06-05.
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
 * / `op` of `DELETE` means that the row in question has been removed and is no longer
 * /                  resident in the table.
 * /
 * / `op` of `INSERT` means that the row in question has been either newly inserted or
 * /                  updated, and is resident in the table.
 * /
 * / `row_pk` is a hash of the row computed by the database. As of 2023-06-13, even for
 * /          tables with a `#[primarykey]` annotation on one column, the `row_pk` is not
 * /          that primary key.
 * /
 * / `row` is the row itself, encoded as BSATN (or possibly SATN for the text api? Find out
 * /       and document - pgoldman 2023-06-05).
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
 * / Do clients receive `TransactionUpdate`s for reducer runs which do not alter any of the
 * / client's subscribed rows? Find out and document - pgoldman 2023-06-05.
 * /
 * / `event` contains information about the reducer.
 * /
 * / `subscriptionUpdate` contains changes to subscribed rows.
 */
export interface TransactionUpdate {
  event: Event | undefined;
  subscriptionUpdate: SubscriptionUpdate | undefined;
}

function createBaseMessage(): Message {
  return {
    functionCall: undefined,
    subscriptionUpdate: undefined,
    event: undefined,
    transactionUpdate: undefined,
    identityToken: undefined,
    subscribe: undefined,
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
    };
  },

  toJSON(message: Message): unknown {
    const obj: any = {};
    message.functionCall !== undefined &&
      (obj.functionCall = message.functionCall
        ? FunctionCall.toJSON(message.functionCall)
        : undefined);
    message.subscriptionUpdate !== undefined &&
      (obj.subscriptionUpdate = message.subscriptionUpdate
        ? SubscriptionUpdate.toJSON(message.subscriptionUpdate)
        : undefined);
    message.event !== undefined &&
      (obj.event = message.event ? Event.toJSON(message.event) : undefined);
    message.transactionUpdate !== undefined &&
      (obj.transactionUpdate = message.transactionUpdate
        ? TransactionUpdate.toJSON(message.transactionUpdate)
        : undefined);
    message.identityToken !== undefined &&
      (obj.identityToken = message.identityToken
        ? IdentityToken.toJSON(message.identityToken)
        : undefined);
    message.subscribe !== undefined &&
      (obj.subscribe = message.subscribe
        ? Subscribe.toJSON(message.subscribe)
        : undefined);
    return obj;
  },

  create<I extends Exact<DeepPartial<Message>, I>>(base?: I): Message {
    return Message.fromPartial(base ?? {});
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
    return message;
  },
};

function createBaseIdentityToken(): IdentityToken {
  return { identity: new Uint8Array(0), token: "" };
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
    };
  },

  toJSON(message: IdentityToken): unknown {
    const obj: any = {};
    message.identity !== undefined &&
      (obj.identity = base64FromBytes(
        message.identity !== undefined ? message.identity : new Uint8Array(0)
      ));
    message.token !== undefined && (obj.token = message.token);
    return obj;
  },

  create<I extends Exact<DeepPartial<IdentityToken>, I>>(
    base?: I
  ): IdentityToken {
    return IdentityToken.fromPartial(base ?? {});
  },

  fromPartial<I extends Exact<DeepPartial<IdentityToken>, I>>(
    object: I
  ): IdentityToken {
    const message = createBaseIdentityToken();
    message.identity = object.identity ?? new Uint8Array(0);
    message.token = object.token ?? "";
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
    message.reducer !== undefined && (obj.reducer = message.reducer);
    message.argBytes !== undefined &&
      (obj.argBytes = base64FromBytes(
        message.argBytes !== undefined ? message.argBytes : new Uint8Array(0)
      ));
    return obj;
  },

  create<I extends Exact<DeepPartial<FunctionCall>, I>>(
    base?: I
  ): FunctionCall {
    return FunctionCall.fromPartial(base ?? {});
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
    if (message.queryStrings) {
      obj.queryStrings = message.queryStrings.map((e) => e);
    } else {
      obj.queryStrings = [];
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<Subscribe>, I>>(base?: I): Subscribe {
    return Subscribe.fromPartial(base ?? {});
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
    };
  },

  toJSON(message: Event): unknown {
    const obj: any = {};
    message.timestamp !== undefined &&
      (obj.timestamp = Math.round(message.timestamp));
    message.callerIdentity !== undefined &&
      (obj.callerIdentity = base64FromBytes(
        message.callerIdentity !== undefined
          ? message.callerIdentity
          : new Uint8Array(0)
      ));
    message.functionCall !== undefined &&
      (obj.functionCall = message.functionCall
        ? FunctionCall.toJSON(message.functionCall)
        : undefined);
    message.status !== undefined &&
      (obj.status = event_StatusToJSON(message.status));
    message.message !== undefined && (obj.message = message.message);
    message.energyQuantaUsed !== undefined &&
      (obj.energyQuantaUsed = Math.round(message.energyQuantaUsed));
    message.hostExecutionDurationMicros !== undefined &&
      (obj.hostExecutionDurationMicros = Math.round(
        message.hostExecutionDurationMicros
      ));
    return obj;
  },

  create<I extends Exact<DeepPartial<Event>, I>>(base?: I): Event {
    return Event.fromPartial(base ?? {});
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
    if (message.tableUpdates) {
      obj.tableUpdates = message.tableUpdates.map((e) =>
        e ? TableUpdate.toJSON(e) : undefined
      );
    } else {
      obj.tableUpdates = [];
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<SubscriptionUpdate>, I>>(
    base?: I
  ): SubscriptionUpdate {
    return SubscriptionUpdate.fromPartial(base ?? {});
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
    message.tableId !== undefined &&
      (obj.tableId = Math.round(message.tableId));
    message.tableName !== undefined && (obj.tableName = message.tableName);
    if (message.tableRowOperations) {
      obj.tableRowOperations = message.tableRowOperations.map((e) =>
        e ? TableRowOperation.toJSON(e) : undefined
      );
    } else {
      obj.tableRowOperations = [];
    }
    return obj;
  },

  create<I extends Exact<DeepPartial<TableUpdate>, I>>(base?: I): TableUpdate {
    return TableUpdate.fromPartial(base ?? {});
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
    message.op !== undefined &&
      (obj.op = tableRowOperation_OperationTypeToJSON(message.op));
    message.rowPk !== undefined &&
      (obj.rowPk = base64FromBytes(
        message.rowPk !== undefined ? message.rowPk : new Uint8Array(0)
      ));
    message.row !== undefined &&
      (obj.row = base64FromBytes(
        message.row !== undefined ? message.row : new Uint8Array(0)
      ));
    return obj;
  },

  create<I extends Exact<DeepPartial<TableRowOperation>, I>>(
    base?: I
  ): TableRowOperation {
    return TableRowOperation.fromPartial(base ?? {});
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
    message.event !== undefined &&
      (obj.event = message.event ? Event.toJSON(message.event) : undefined);
    message.subscriptionUpdate !== undefined &&
      (obj.subscriptionUpdate = message.subscriptionUpdate
        ? SubscriptionUpdate.toJSON(message.subscriptionUpdate)
        : undefined);
    return obj;
  },

  create<I extends Exact<DeepPartial<TransactionUpdate>, I>>(
    base?: I
  ): TransactionUpdate {
    return TransactionUpdate.fromPartial(base ?? {});
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

declare var self: any | undefined;
declare var window: any | undefined;
declare var global: any | undefined;
var tsProtoGlobalThis: any = (() => {
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

// If you get a compile-error about 'Constructor<Long> and ... have no overlap',
// add '--ts_proto_opt=esModuleInterop=true' as a flag when calling 'protoc'.
if (_m0.util.Long !== Long) {
  _m0.util.Long = Long as any;
  _m0.configure();
}

function isSet(value: any): boolean {
  return value !== null && value !== undefined;
}
