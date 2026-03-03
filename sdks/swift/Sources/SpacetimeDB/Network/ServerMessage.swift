import Foundation

// MARK: - Server Messages (Decoding)

/// Tag order for Rust `websocket::v2::ServerMessage`:
/// 0 = InitialConnection
/// 1 = SubscribeApplied
/// 2 = UnsubscribeApplied
/// 3 = SubscriptionError
/// 4 = TransactionUpdate
/// 5 = OneOffQueryResult
/// 6 = ReducerResult
/// 7 = ProcedureResult
public enum ServerMessage: Decodable {
    case initialConnection(InitialConnection)
    case subscribeApplied(SubscribeApplied)
    case unsubscribeApplied(UnsubscribeApplied)
    case subscriptionError(SubscriptionError)
    case transactionUpdate(TransactionUpdate)
    case oneOffQueryResult(OneOffQueryResult)
    case reducerResult(ReducerResult)
    case procedureResult(ProcedureResult)
    case other(UInt8)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNDecoder for ServerMessage")
    }
}

extension ServerMessage: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0: return .initialConnection(try InitialConnection(fromBSATN: decoder))
            case 1: return .subscribeApplied(try SubscribeApplied(fromBSATN: decoder))
            case 2: return .unsubscribeApplied(try UnsubscribeApplied(fromBSATN: decoder))
            case 3: return .subscriptionError(try SubscriptionError(fromBSATN: decoder))
            case 4: return .transactionUpdate(try TransactionUpdate(fromBSATN: decoder))
            case 5: return .oneOffQueryResult(try OneOffQueryResult(fromBSATN: decoder))
            case 6: return .reducerResult(try ReducerResult(fromBSATN: decoder))
            case 7: return .procedureResult(try ProcedureResult(fromBSATN: decoder))
            default:
                _ = try? decoder.storage.readBytes(count: decoder.storage.remaining)
                return .other(tag)
            }
        }
    }
}

// MARK: - Connection / Subscription

/// Rust: `InitialConnection { identity: Identity, connection_id: ConnectionId, token: Box<str> }`
public struct InitialConnection: BSATNSpecialDecodable, Decodable {
    public var identity: Identity
    public var connectionId: ClientConnectionId
    public var token: String

    public init(identity: Identity, connectionId: ClientConnectionId, token: String) {
        self.identity = identity
        self.connectionId = connectionId
        self.token = token
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.identity = try Identity(fromBSATN: decoder)
        self.connectionId = try ClientConnectionId(fromBSATN: decoder)
        let container = try decoder.singleValueContainer()
        self.token = try container.decode(String.self)
    }
}

/// Rust: `SubscribeApplied { request_id: u32, query_set_id: QuerySetId, rows: QueryRows }`
public struct SubscribeApplied: BSATNSpecialDecodable, Decodable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var rows: QueryRows

    public init(requestId: RequestId, querySetId: QuerySetId, rows: QueryRows) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try RequestId(fromBSATN: decoder)
        self.querySetId = try QuerySetId(fromBSATN: decoder)
        self.rows = try QueryRows(fromBSATN: decoder)
    }

    func asTransactionUpdate() -> TransactionUpdate {
        let tables = rows.tables.map {
            TableUpdate(
                tableName: $0.table,
                rows: [.persistentTable(PersistentTableRows(
                    inserts: $0.rows,
                    deletes: BsatnRowList.empty
                ))]
            )
        }
        return TransactionUpdate(querySets: [QuerySetUpdate(querySetId: querySetId, tables: tables)])
    }
}

/// Rust: `UnsubscribeApplied { request_id: u32, query_set_id: QuerySetId, rows: Option<QueryRows> }`
public struct UnsubscribeApplied: BSATNSpecialDecodable, Decodable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var rows: QueryRows?

    public init(requestId: RequestId, querySetId: QuerySetId, rows: QueryRows?) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try RequestId(fromBSATN: decoder)
        self.querySetId = try QuerySetId(fromBSATN: decoder)
        self.rows = try Optional<QueryRows>(fromBSATN: decoder)
    }
}

/// Rust: `SubscriptionError { request_id: Option<u32>, query_set_id: QuerySetId, error: Box<str> }`
public struct SubscriptionError: BSATNSpecialDecodable, Decodable {
    public var requestId: RequestId?
    public var querySetId: QuerySetId
    public var error: String

    public init(requestId: RequestId?, querySetId: QuerySetId, error: String) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.error = error
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try Optional<RequestId>(fromBSATN: decoder)
        self.querySetId = try QuerySetId(fromBSATN: decoder)
        let container = try decoder.singleValueContainer()
        self.error = try container.decode(String.self)
    }
}

// MARK: - Query / Reducer / Procedure Results

public struct OneOffQueryResult: BSATNSpecialDecodable, Decodable {
    public var requestId: RequestId
    public var result: QueryRowsResult

    public init(requestId: RequestId, result: QueryRowsResult) {
        self.requestId = requestId
        self.result = result
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try RequestId(fromBSATN: decoder)
        self.result = try QueryRowsResult(fromBSATN: decoder)
    }
}

public enum QueryRowsResult: Decodable {
    case ok(QueryRows)
    case err(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }
}

extension QueryRowsResult: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0: return .ok(try QueryRows(fromBSATN: decoder))
            case 1:
                let container = try decoder.singleValueContainer()
                return .err(try container.decode(String.self))
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct ReducerResult: BSATNSpecialDecodable, Decodable {
    public var requestId: RequestId
    public var timestamp: Int64
    public var result: ReducerOutcome

    public init(requestId: RequestId, timestamp: Int64, result: ReducerOutcome) {
        self.requestId = requestId
        self.timestamp = timestamp
        self.result = result
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try RequestId(fromBSATN: decoder)
        self.timestamp = try decoder.storage.read(Int64.self)
        self.result = try ReducerOutcome(fromBSATN: decoder)
    }
}

public struct ProcedureResult: BSATNSpecialDecodable, Decodable {
    public var status: ProcedureStatus
    public var timestamp: Int64
    public var totalHostExecutionDuration: Int64
    public var requestId: RequestId

    public init(status: ProcedureStatus, timestamp: Int64, totalHostExecutionDuration: Int64, requestId: RequestId) {
        self.status = status
        self.timestamp = timestamp
        self.totalHostExecutionDuration = totalHostExecutionDuration
        self.requestId = requestId
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.status = try ProcedureStatus(fromBSATN: decoder)
        self.timestamp = try decoder.storage.read(Int64.self)
        self.totalHostExecutionDuration = try decoder.storage.read(Int64.self)
        self.requestId = try RequestId(fromBSATN: decoder)
    }
}
