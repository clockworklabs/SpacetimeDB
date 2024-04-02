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
exports.OneOffTable = exports.OneOffQueryResponse = exports.OneOffQuery = exports.TransactionUpdate = exports.TableRowOperation = exports.TableUpdate = exports.SubscriptionUpdate = exports.Event = exports.Subscribe = exports.FunctionCall = exports.IdentityToken = exports.Message = exports.tableRowOperation_OperationTypeToJSON = exports.tableRowOperation_OperationTypeFromJSON = exports.TableRowOperation_OperationType = exports.event_StatusToJSON = exports.event_StatusFromJSON = exports.Event_Status = exports.protobufPackage = void 0;
/* eslint-disable */
const _m0 = __importStar(require("protobufjs/minimal"));
const long_1 = __importDefault(require("long"));
exports.protobufPackage = "client_api";
var Event_Status;
(function (Event_Status) {
    Event_Status[Event_Status["committed"] = 0] = "committed";
    Event_Status[Event_Status["failed"] = 1] = "failed";
    Event_Status[Event_Status["out_of_energy"] = 2] = "out_of_energy";
    Event_Status[Event_Status["UNRECOGNIZED"] = -1] = "UNRECOGNIZED";
})(Event_Status = exports.Event_Status || (exports.Event_Status = {}));
function event_StatusFromJSON(object) {
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
exports.event_StatusFromJSON = event_StatusFromJSON;
function event_StatusToJSON(object) {
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
exports.event_StatusToJSON = event_StatusToJSON;
var TableRowOperation_OperationType;
(function (TableRowOperation_OperationType) {
    TableRowOperation_OperationType[TableRowOperation_OperationType["DELETE"] = 0] = "DELETE";
    TableRowOperation_OperationType[TableRowOperation_OperationType["INSERT"] = 1] = "INSERT";
    TableRowOperation_OperationType[TableRowOperation_OperationType["UNRECOGNIZED"] = -1] = "UNRECOGNIZED";
})(TableRowOperation_OperationType = exports.TableRowOperation_OperationType || (exports.TableRowOperation_OperationType = {}));
function tableRowOperation_OperationTypeFromJSON(object) {
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
exports.tableRowOperation_OperationTypeFromJSON = tableRowOperation_OperationTypeFromJSON;
function tableRowOperation_OperationTypeToJSON(object) {
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
exports.tableRowOperation_OperationTypeToJSON = tableRowOperation_OperationTypeToJSON;
function createBaseMessage() {
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
exports.Message = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.functionCall !== undefined) {
            exports.FunctionCall.encode(message.functionCall, writer.uint32(10).fork()).ldelim();
        }
        if (message.subscriptionUpdate !== undefined) {
            exports.SubscriptionUpdate.encode(message.subscriptionUpdate, writer.uint32(18).fork()).ldelim();
        }
        if (message.event !== undefined) {
            exports.Event.encode(message.event, writer.uint32(26).fork()).ldelim();
        }
        if (message.transactionUpdate !== undefined) {
            exports.TransactionUpdate.encode(message.transactionUpdate, writer.uint32(34).fork()).ldelim();
        }
        if (message.identityToken !== undefined) {
            exports.IdentityToken.encode(message.identityToken, writer.uint32(42).fork()).ldelim();
        }
        if (message.subscribe !== undefined) {
            exports.Subscribe.encode(message.subscribe, writer.uint32(50).fork()).ldelim();
        }
        if (message.oneOffQuery !== undefined) {
            exports.OneOffQuery.encode(message.oneOffQuery, writer.uint32(58).fork()).ldelim();
        }
        if (message.oneOffQueryResponse !== undefined) {
            exports.OneOffQueryResponse.encode(message.oneOffQueryResponse, writer.uint32(66).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseMessage();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if (tag !== 10) {
                        break;
                    }
                    message.functionCall = exports.FunctionCall.decode(reader, reader.uint32());
                    continue;
                case 2:
                    if (tag !== 18) {
                        break;
                    }
                    message.subscriptionUpdate = exports.SubscriptionUpdate.decode(reader, reader.uint32());
                    continue;
                case 3:
                    if (tag !== 26) {
                        break;
                    }
                    message.event = exports.Event.decode(reader, reader.uint32());
                    continue;
                case 4:
                    if (tag !== 34) {
                        break;
                    }
                    message.transactionUpdate = exports.TransactionUpdate.decode(reader, reader.uint32());
                    continue;
                case 5:
                    if (tag !== 42) {
                        break;
                    }
                    message.identityToken = exports.IdentityToken.decode(reader, reader.uint32());
                    continue;
                case 6:
                    if (tag !== 50) {
                        break;
                    }
                    message.subscribe = exports.Subscribe.decode(reader, reader.uint32());
                    continue;
                case 7:
                    if (tag !== 58) {
                        break;
                    }
                    message.oneOffQuery = exports.OneOffQuery.decode(reader, reader.uint32());
                    continue;
                case 8:
                    if (tag !== 66) {
                        break;
                    }
                    message.oneOffQueryResponse = exports.OneOffQueryResponse.decode(reader, reader.uint32());
                    continue;
            }
            if ((tag & 7) === 4 || tag === 0) {
                break;
            }
            reader.skipType(tag & 7);
        }
        return message;
    },
    fromJSON(object) {
        return {
            functionCall: isSet(object.functionCall)
                ? exports.FunctionCall.fromJSON(object.functionCall)
                : undefined,
            subscriptionUpdate: isSet(object.subscriptionUpdate)
                ? exports.SubscriptionUpdate.fromJSON(object.subscriptionUpdate)
                : undefined,
            event: isSet(object.event) ? exports.Event.fromJSON(object.event) : undefined,
            transactionUpdate: isSet(object.transactionUpdate)
                ? exports.TransactionUpdate.fromJSON(object.transactionUpdate)
                : undefined,
            identityToken: isSet(object.identityToken)
                ? exports.IdentityToken.fromJSON(object.identityToken)
                : undefined,
            subscribe: isSet(object.subscribe)
                ? exports.Subscribe.fromJSON(object.subscribe)
                : undefined,
            oneOffQuery: isSet(object.oneOffQuery)
                ? exports.OneOffQuery.fromJSON(object.oneOffQuery)
                : undefined,
            oneOffQueryResponse: isSet(object.oneOffQueryResponse)
                ? exports.OneOffQueryResponse.fromJSON(object.oneOffQueryResponse)
                : undefined,
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.functionCall !== undefined) {
            obj.functionCall = exports.FunctionCall.toJSON(message.functionCall);
        }
        if (message.subscriptionUpdate !== undefined) {
            obj.subscriptionUpdate = exports.SubscriptionUpdate.toJSON(message.subscriptionUpdate);
        }
        if (message.event !== undefined) {
            obj.event = exports.Event.toJSON(message.event);
        }
        if (message.transactionUpdate !== undefined) {
            obj.transactionUpdate = exports.TransactionUpdate.toJSON(message.transactionUpdate);
        }
        if (message.identityToken !== undefined) {
            obj.identityToken = exports.IdentityToken.toJSON(message.identityToken);
        }
        if (message.subscribe !== undefined) {
            obj.subscribe = exports.Subscribe.toJSON(message.subscribe);
        }
        if (message.oneOffQuery !== undefined) {
            obj.oneOffQuery = exports.OneOffQuery.toJSON(message.oneOffQuery);
        }
        if (message.oneOffQueryResponse !== undefined) {
            obj.oneOffQueryResponse = exports.OneOffQueryResponse.toJSON(message.oneOffQueryResponse);
        }
        return obj;
    },
    create(base) {
        return exports.Message.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseMessage();
        message.functionCall =
            object.functionCall !== undefined && object.functionCall !== null
                ? exports.FunctionCall.fromPartial(object.functionCall)
                : undefined;
        message.subscriptionUpdate =
            object.subscriptionUpdate !== undefined &&
                object.subscriptionUpdate !== null
                ? exports.SubscriptionUpdate.fromPartial(object.subscriptionUpdate)
                : undefined;
        message.event =
            object.event !== undefined && object.event !== null
                ? exports.Event.fromPartial(object.event)
                : undefined;
        message.transactionUpdate =
            object.transactionUpdate !== undefined &&
                object.transactionUpdate !== null
                ? exports.TransactionUpdate.fromPartial(object.transactionUpdate)
                : undefined;
        message.identityToken =
            object.identityToken !== undefined && object.identityToken !== null
                ? exports.IdentityToken.fromPartial(object.identityToken)
                : undefined;
        message.subscribe =
            object.subscribe !== undefined && object.subscribe !== null
                ? exports.Subscribe.fromPartial(object.subscribe)
                : undefined;
        message.oneOffQuery =
            object.oneOffQuery !== undefined && object.oneOffQuery !== null
                ? exports.OneOffQuery.fromPartial(object.oneOffQuery)
                : undefined;
        message.oneOffQueryResponse =
            object.oneOffQueryResponse !== undefined &&
                object.oneOffQueryResponse !== null
                ? exports.OneOffQueryResponse.fromPartial(object.oneOffQueryResponse)
                : undefined;
        return message;
    },
};
function createBaseIdentityToken() {
    return { identity: new Uint8Array(0), token: "", address: new Uint8Array(0) };
}
exports.IdentityToken = {
    encode(message, writer = _m0.Writer.create()) {
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
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
    fromJSON(object) {
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
    toJSON(message) {
        const obj = {};
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
    create(base) {
        return exports.IdentityToken.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseIdentityToken();
        message.identity = object.identity ?? new Uint8Array(0);
        message.token = object.token ?? "";
        message.address = object.address ?? new Uint8Array(0);
        return message;
    },
};
function createBaseFunctionCall() {
    return { reducer: "", argBytes: new Uint8Array(0) };
}
exports.FunctionCall = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.reducer !== "") {
            writer.uint32(10).string(message.reducer);
        }
        if (message.argBytes.length !== 0) {
            writer.uint32(18).bytes(message.argBytes);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
    fromJSON(object) {
        return {
            reducer: isSet(object.reducer) ? String(object.reducer) : "",
            argBytes: isSet(object.argBytes)
                ? bytesFromBase64(object.argBytes)
                : new Uint8Array(0),
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.reducer !== "") {
            obj.reducer = message.reducer;
        }
        if (message.argBytes.length !== 0) {
            obj.argBytes = base64FromBytes(message.argBytes);
        }
        return obj;
    },
    create(base) {
        return exports.FunctionCall.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseFunctionCall();
        message.reducer = object.reducer ?? "";
        message.argBytes = object.argBytes ?? new Uint8Array(0);
        return message;
    },
};
function createBaseSubscribe() {
    return { queryStrings: [] };
}
exports.Subscribe = {
    encode(message, writer = _m0.Writer.create()) {
        for (const v of message.queryStrings) {
            writer.uint32(10).string(v);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
    fromJSON(object) {
        return {
            queryStrings: Array.isArray(object?.queryStrings)
                ? object.queryStrings.map((e) => String(e))
                : [],
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.queryStrings?.length) {
            obj.queryStrings = message.queryStrings;
        }
        return obj;
    },
    create(base) {
        return exports.Subscribe.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseSubscribe();
        message.queryStrings = object.queryStrings?.map((e) => e) || [];
        return message;
    },
};
function createBaseEvent() {
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
exports.Event = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.timestamp !== 0) {
            writer.uint32(8).uint64(message.timestamp);
        }
        if (message.callerIdentity.length !== 0) {
            writer.uint32(18).bytes(message.callerIdentity);
        }
        if (message.functionCall !== undefined) {
            exports.FunctionCall.encode(message.functionCall, writer.uint32(26).fork()).ldelim();
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
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseEvent();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if (tag !== 8) {
                        break;
                    }
                    message.timestamp = longToNumber(reader.uint64());
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
                    message.functionCall = exports.FunctionCall.decode(reader, reader.uint32());
                    continue;
                case 4:
                    if (tag !== 32) {
                        break;
                    }
                    message.status = reader.int32();
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
                    message.energyQuantaUsed = longToNumber(reader.int64());
                    continue;
                case 7:
                    if (tag !== 56) {
                        break;
                    }
                    message.hostExecutionDurationMicros = longToNumber(reader.uint64());
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
    fromJSON(object) {
        return {
            timestamp: isSet(object.timestamp) ? Number(object.timestamp) : 0,
            callerIdentity: isSet(object.callerIdentity)
                ? bytesFromBase64(object.callerIdentity)
                : new Uint8Array(0),
            functionCall: isSet(object.functionCall)
                ? exports.FunctionCall.fromJSON(object.functionCall)
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
    toJSON(message) {
        const obj = {};
        if (message.timestamp !== 0) {
            obj.timestamp = Math.round(message.timestamp);
        }
        if (message.callerIdentity.length !== 0) {
            obj.callerIdentity = base64FromBytes(message.callerIdentity);
        }
        if (message.functionCall !== undefined) {
            obj.functionCall = exports.FunctionCall.toJSON(message.functionCall);
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
            obj.hostExecutionDurationMicros = Math.round(message.hostExecutionDurationMicros);
        }
        if (message.callerAddress.length !== 0) {
            obj.callerAddress = base64FromBytes(message.callerAddress);
        }
        return obj;
    },
    create(base) {
        return exports.Event.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseEvent();
        message.timestamp = object.timestamp ?? 0;
        message.callerIdentity = object.callerIdentity ?? new Uint8Array(0);
        message.functionCall =
            object.functionCall !== undefined && object.functionCall !== null
                ? exports.FunctionCall.fromPartial(object.functionCall)
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
function createBaseSubscriptionUpdate() {
    return { tableUpdates: [] };
}
exports.SubscriptionUpdate = {
    encode(message, writer = _m0.Writer.create()) {
        for (const v of message.tableUpdates) {
            exports.TableUpdate.encode(v, writer.uint32(10).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseSubscriptionUpdate();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if (tag !== 10) {
                        break;
                    }
                    message.tableUpdates.push(exports.TableUpdate.decode(reader, reader.uint32()));
                    continue;
            }
            if ((tag & 7) === 4 || tag === 0) {
                break;
            }
            reader.skipType(tag & 7);
        }
        return message;
    },
    fromJSON(object) {
        return {
            tableUpdates: Array.isArray(object?.tableUpdates)
                ? object.tableUpdates.map((e) => exports.TableUpdate.fromJSON(e))
                : [],
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.tableUpdates?.length) {
            obj.tableUpdates = message.tableUpdates.map((e) => exports.TableUpdate.toJSON(e));
        }
        return obj;
    },
    create(base) {
        return exports.SubscriptionUpdate.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseSubscriptionUpdate();
        message.tableUpdates =
            object.tableUpdates?.map((e) => exports.TableUpdate.fromPartial(e)) || [];
        return message;
    },
};
function createBaseTableUpdate() {
    return { tableId: 0, tableName: "", tableRowOperations: [] };
}
exports.TableUpdate = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.tableId !== 0) {
            writer.uint32(8).uint32(message.tableId);
        }
        if (message.tableName !== "") {
            writer.uint32(18).string(message.tableName);
        }
        for (const v of message.tableRowOperations) {
            exports.TableRowOperation.encode(v, writer.uint32(26).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
                    message.tableRowOperations.push(exports.TableRowOperation.decode(reader, reader.uint32()));
                    continue;
            }
            if ((tag & 7) === 4 || tag === 0) {
                break;
            }
            reader.skipType(tag & 7);
        }
        return message;
    },
    fromJSON(object) {
        return {
            tableId: isSet(object.tableId) ? Number(object.tableId) : 0,
            tableName: isSet(object.tableName) ? String(object.tableName) : "",
            tableRowOperations: Array.isArray(object?.tableRowOperations)
                ? object.tableRowOperations.map((e) => exports.TableRowOperation.fromJSON(e))
                : [],
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.tableId !== 0) {
            obj.tableId = Math.round(message.tableId);
        }
        if (message.tableName !== "") {
            obj.tableName = message.tableName;
        }
        if (message.tableRowOperations?.length) {
            obj.tableRowOperations = message.tableRowOperations.map((e) => exports.TableRowOperation.toJSON(e));
        }
        return obj;
    },
    create(base) {
        return exports.TableUpdate.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseTableUpdate();
        message.tableId = object.tableId ?? 0;
        message.tableName = object.tableName ?? "";
        message.tableRowOperations =
            object.tableRowOperations?.map((e) => exports.TableRowOperation.fromPartial(e)) ||
                [];
        return message;
    },
};
function createBaseTableRowOperation() {
    return { op: 0, row: new Uint8Array(0) };
}
exports.TableRowOperation = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.op !== 0) {
            writer.uint32(8).int32(message.op);
        }
        if (message.row.length !== 0) {
            writer.uint32(26).bytes(message.row);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseTableRowOperation();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if (tag !== 8) {
                        break;
                    }
                    message.op = reader.int32();
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
    fromJSON(object) {
        return {
            op: isSet(object.op)
                ? tableRowOperation_OperationTypeFromJSON(object.op)
                : 0,
            row: isSet(object.row) ? bytesFromBase64(object.row) : new Uint8Array(0),
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.op !== 0) {
            obj.op = tableRowOperation_OperationTypeToJSON(message.op);
        }
        if (message.row.length !== 0) {
            obj.row = base64FromBytes(message.row);
        }
        return obj;
    },
    create(base) {
        return exports.TableRowOperation.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseTableRowOperation();
        message.op = object.op ?? 0;
        message.row = object.row ?? new Uint8Array(0);
        return message;
    },
};
function createBaseTransactionUpdate() {
    return { event: undefined, subscriptionUpdate: undefined };
}
exports.TransactionUpdate = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.event !== undefined) {
            exports.Event.encode(message.event, writer.uint32(10).fork()).ldelim();
        }
        if (message.subscriptionUpdate !== undefined) {
            exports.SubscriptionUpdate.encode(message.subscriptionUpdate, writer.uint32(18).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseTransactionUpdate();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if (tag !== 10) {
                        break;
                    }
                    message.event = exports.Event.decode(reader, reader.uint32());
                    continue;
                case 2:
                    if (tag !== 18) {
                        break;
                    }
                    message.subscriptionUpdate = exports.SubscriptionUpdate.decode(reader, reader.uint32());
                    continue;
            }
            if ((tag & 7) === 4 || tag === 0) {
                break;
            }
            reader.skipType(tag & 7);
        }
        return message;
    },
    fromJSON(object) {
        return {
            event: isSet(object.event) ? exports.Event.fromJSON(object.event) : undefined,
            subscriptionUpdate: isSet(object.subscriptionUpdate)
                ? exports.SubscriptionUpdate.fromJSON(object.subscriptionUpdate)
                : undefined,
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.event !== undefined) {
            obj.event = exports.Event.toJSON(message.event);
        }
        if (message.subscriptionUpdate !== undefined) {
            obj.subscriptionUpdate = exports.SubscriptionUpdate.toJSON(message.subscriptionUpdate);
        }
        return obj;
    },
    create(base) {
        return exports.TransactionUpdate.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseTransactionUpdate();
        message.event =
            object.event !== undefined && object.event !== null
                ? exports.Event.fromPartial(object.event)
                : undefined;
        message.subscriptionUpdate =
            object.subscriptionUpdate !== undefined &&
                object.subscriptionUpdate !== null
                ? exports.SubscriptionUpdate.fromPartial(object.subscriptionUpdate)
                : undefined;
        return message;
    },
};
function createBaseOneOffQuery() {
    return { messageId: new Uint8Array(0), queryString: "" };
}
exports.OneOffQuery = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.messageId.length !== 0) {
            writer.uint32(10).bytes(message.messageId);
        }
        if (message.queryString !== "") {
            writer.uint32(18).string(message.queryString);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
    fromJSON(object) {
        return {
            messageId: isSet(object.messageId)
                ? bytesFromBase64(object.messageId)
                : new Uint8Array(0),
            queryString: isSet(object.queryString) ? String(object.queryString) : "",
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.messageId.length !== 0) {
            obj.messageId = base64FromBytes(message.messageId);
        }
        if (message.queryString !== "") {
            obj.queryString = message.queryString;
        }
        return obj;
    },
    create(base) {
        return exports.OneOffQuery.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseOneOffQuery();
        message.messageId = object.messageId ?? new Uint8Array(0);
        message.queryString = object.queryString ?? "";
        return message;
    },
};
function createBaseOneOffQueryResponse() {
    return { messageId: new Uint8Array(0), error: "", tables: [] };
}
exports.OneOffQueryResponse = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.messageId.length !== 0) {
            writer.uint32(10).bytes(message.messageId);
        }
        if (message.error !== "") {
            writer.uint32(18).string(message.error);
        }
        for (const v of message.tables) {
            exports.OneOffTable.encode(v, writer.uint32(26).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
                    message.tables.push(exports.OneOffTable.decode(reader, reader.uint32()));
                    continue;
            }
            if ((tag & 7) === 4 || tag === 0) {
                break;
            }
            reader.skipType(tag & 7);
        }
        return message;
    },
    fromJSON(object) {
        return {
            messageId: isSet(object.messageId)
                ? bytesFromBase64(object.messageId)
                : new Uint8Array(0),
            error: isSet(object.error) ? String(object.error) : "",
            tables: Array.isArray(object?.tables)
                ? object.tables.map((e) => exports.OneOffTable.fromJSON(e))
                : [],
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.messageId.length !== 0) {
            obj.messageId = base64FromBytes(message.messageId);
        }
        if (message.error !== "") {
            obj.error = message.error;
        }
        if (message.tables?.length) {
            obj.tables = message.tables.map((e) => exports.OneOffTable.toJSON(e));
        }
        return obj;
    },
    create(base) {
        return exports.OneOffQueryResponse.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseOneOffQueryResponse();
        message.messageId = object.messageId ?? new Uint8Array(0);
        message.error = object.error ?? "";
        message.tables =
            object.tables?.map((e) => exports.OneOffTable.fromPartial(e)) || [];
        return message;
    },
};
function createBaseOneOffTable() {
    return { tableName: "", row: [] };
}
exports.OneOffTable = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.tableName !== "") {
            writer.uint32(18).string(message.tableName);
        }
        for (const v of message.row) {
            writer.uint32(34).bytes(v);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : _m0.Reader.create(input);
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
    fromJSON(object) {
        return {
            tableName: isSet(object.tableName) ? String(object.tableName) : "",
            row: Array.isArray(object?.row)
                ? object.row.map((e) => bytesFromBase64(e))
                : [],
        };
    },
    toJSON(message) {
        const obj = {};
        if (message.tableName !== "") {
            obj.tableName = message.tableName;
        }
        if (message.row?.length) {
            obj.row = message.row.map((e) => base64FromBytes(e));
        }
        return obj;
    },
    create(base) {
        return exports.OneOffTable.fromPartial(base ?? {});
    },
    fromPartial(object) {
        const message = createBaseOneOffTable();
        message.tableName = object.tableName ?? "";
        message.row = object.row?.map((e) => e) || [];
        return message;
    },
};
const tsProtoGlobalThis = (() => {
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
function bytesFromBase64(b64) {
    if (tsProtoGlobalThis.Buffer) {
        return Uint8Array.from(tsProtoGlobalThis.Buffer.from(b64, "base64"));
    }
    else {
        const bin = tsProtoGlobalThis.atob(b64);
        const arr = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; ++i) {
            arr[i] = bin.charCodeAt(i);
        }
        return arr;
    }
}
function base64FromBytes(arr) {
    if (tsProtoGlobalThis.Buffer) {
        return tsProtoGlobalThis.Buffer.from(arr).toString("base64");
    }
    else {
        const bin = [];
        arr.forEach((byte) => {
            bin.push(String.fromCharCode(byte));
        });
        return tsProtoGlobalThis.btoa(bin.join(""));
    }
}
function longToNumber(long) {
    if (long.gt(Number.MAX_SAFE_INTEGER)) {
        throw new tsProtoGlobalThis.Error("Value is larger than Number.MAX_SAFE_INTEGER");
    }
    return long.toNumber();
}
if (_m0.util.Long !== long_1.default) {
    _m0.util.Long = long_1.default;
    _m0.configure();
}
function isSet(value) {
    return value !== null && value !== undefined;
}
