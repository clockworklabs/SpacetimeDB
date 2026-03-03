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
public enum ServerMessage: Decodable, BSATNSpecialDecodable, Sendable {
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

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ServerMessage {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0: return .initialConnection(try InitialConnection.decodeBSATN(from: &reader))
            case 1: return .subscribeApplied(try SubscribeApplied.decodeBSATN(from: &reader))
            case 2: return .unsubscribeApplied(try UnsubscribeApplied.decodeBSATN(from: &reader))
            case 3: return .subscriptionError(try SubscriptionError.decodeBSATN(from: &reader))
            case 4: return .transactionUpdate(try TransactionUpdate.decodeBSATN(from: &reader))
            case 5: return .oneOffQueryResult(try OneOffQueryResult.decodeBSATN(from: &reader))
            case 6: return .reducerResult(try ReducerResult.decodeBSATN(from: &reader))
            case 7: return .procedureResult(try ProcedureResult.decodeBSATN(from: &reader))
            default:
                _ = try? reader.readBytes(count: reader.remaining)
                return .other(tag)
            }
        }
    }
}

// MARK: - Connection / Subscription

/// Rust: `InitialConnection { identity: Identity, connection_id: ConnectionId, token: Box<str> }`
public struct InitialConnection: BSATNSpecialDecodable, Decodable, Sendable {
    public var identity: Identity
    public var connectionId: ClientConnectionId
    public var token: String

    public init(identity: Identity, connectionId: ClientConnectionId, token: String) {
        self.identity = identity
        self.connectionId = connectionId
        self.token = token
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> InitialConnection {
        let identity = try Identity.decodeBSATN(from: &reader)
        let connectionId = try ClientConnectionId.decodeBSATN(from: &reader)
        let token = try reader.readString()
        return InitialConnection(identity: identity, connectionId: connectionId, token: token)
    }
}

/// Rust: `SubscribeApplied { request_id: u32, query_set_id: QuerySetId, rows: QueryRows }`
public struct SubscribeApplied: BSATNSpecialDecodable, Decodable, Sendable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var rows: QueryRows

    public init(requestId: RequestId, querySetId: QuerySetId, rows: QueryRows) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> SubscribeApplied {
        return SubscribeApplied(
            requestId: try RequestId.decodeBSATN(from: &reader),
            querySetId: try QuerySetId.decodeBSATN(from: &reader),
            rows: try QueryRows.decodeBSATN(from: &reader)
        )
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
public struct UnsubscribeApplied: BSATNSpecialDecodable, Decodable, Sendable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var rows: QueryRows?

    public init(requestId: RequestId, querySetId: QuerySetId, rows: QueryRows?) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> UnsubscribeApplied {
        return UnsubscribeApplied(
            requestId: try RequestId.decodeBSATN(from: &reader),
            querySetId: try QuerySetId.decodeBSATN(from: &reader),
            rows: try Optional<QueryRows>.decodeBSATN(from: &reader)
        )
    }
}

/// Rust: `SubscriptionError { request_id: Option<u32>, query_set_id: QuerySetId, error: Box<str> }`
public struct SubscriptionError: BSATNSpecialDecodable, Decodable, Sendable {
    public var requestId: RequestId?
    public var querySetId: QuerySetId
    public var error: String

    public init(requestId: RequestId?, querySetId: QuerySetId, error: String) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.error = error
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> SubscriptionError {
        return SubscriptionError(
            requestId: try Optional<RequestId>.decodeBSATN(from: &reader),
            querySetId: try QuerySetId.decodeBSATN(from: &reader),
            error: try reader.readString()
        )
    }
}

// MARK: - Query / Reducer / Procedure Results

public struct OneOffQueryResult: BSATNSpecialDecodable, Decodable, Sendable {
    public var requestId: RequestId
    public var result: QueryRowsResult

    public init(requestId: RequestId, result: QueryRowsResult) {
        self.requestId = requestId
        self.result = result
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> OneOffQueryResult {
        return OneOffQueryResult(
            requestId: try RequestId.decodeBSATN(from: &reader),
            result: try QueryRowsResult.decodeBSATN(from: &reader)
        )
    }
}

public enum QueryRowsResult: Decodable, BSATNSpecialDecodable, Sendable {
    case ok(QueryRows)
    case err(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> QueryRowsResult {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0: return .ok(try QueryRows.decodeBSATN(from: &reader))
            case 1: return .err(try reader.readString())
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct ReducerResult: BSATNSpecialDecodable, Decodable, Sendable {
    public var requestId: RequestId
    public var timestamp: Int64
    public var result: ReducerOutcome

    public init(requestId: RequestId, timestamp: Int64, result: ReducerOutcome) {
        self.requestId = requestId
        self.timestamp = timestamp
        self.result = result
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ReducerResult {
        return ReducerResult(
            requestId: try RequestId.decodeBSATN(from: &reader),
            timestamp: try reader.read(Int64.self),
            result: try ReducerOutcome.decodeBSATN(from: &reader)
        )
    }
}

public struct ProcedureResult: BSATNSpecialDecodable, Decodable, Sendable {
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

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ProcedureResult {
        return ProcedureResult(
            status: try ProcedureStatus.decodeBSATN(from: &reader),
            timestamp: try reader.read(Int64.self),
            totalHostExecutionDuration: try reader.read(Int64.self),
            requestId: try RequestId.decodeBSATN(from: &reader)
        )
    }
}
