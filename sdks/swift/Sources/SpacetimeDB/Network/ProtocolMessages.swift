import Foundation

// These represent the standard `v2.bsatn.spacetimedb` wire protocol messages.
// Derived from `crates/client-api-messages/src/websocket/v2.rs`.

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
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            self = .initialConnection(try InitialConnection(fromBSATN: decoder))
        case 1:
            self = .subscribeApplied(try SubscribeApplied(fromBSATN: decoder))
        case 2:
            self = .unsubscribeApplied(try UnsubscribeApplied(fromBSATN: decoder))
        case 3:
            self = .subscriptionError(try SubscriptionError(fromBSATN: decoder))
        case 4:
            self = .transactionUpdate(try TransactionUpdate(fromBSATN: decoder))
        case 5:
            self = .oneOffQueryResult(try OneOffQueryResult(fromBSATN: decoder))
        case 6:
            self = .reducerResult(try ReducerResult(fromBSATN: decoder))
        case 7:
            self = .procedureResult(try ProcedureResult(fromBSATN: decoder))
        default:
            self = .other(tag)
            _ = try? decoder.storage.readBytes(count: decoder.storage.remaining)
        }
    }
}

// MARK: - Connection / Subscription

/// Rust: `InitialConnection { identity: Identity, connection_id: ConnectionId, token: Box<str> }`
public struct InitialConnection: BSATNSpecialDecodable {
    public var identity: Data      // 32 bytes
    public var connectionId: Data  // 16 bytes
    public var token: String

    public init(identity: Data, connectionId: Data, token: String) {
        self.identity = identity
        self.connectionId = connectionId
        self.token = token
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.identity = try decoder.storage.readBytes(count: 32)
        self.connectionId = try decoder.storage.readBytes(count: 16)
        let container = try decoder.singleValueContainer()
        self.token = try container.decode(String.self)
    }
}

/// Rust: `SubscribeApplied { request_id: u32, query_set_id: QuerySetId, rows: QueryRows }`
public struct SubscribeApplied: BSATNSpecialDecodable {
    public var requestId: UInt32
    public var querySetId: UInt32
    public var rows: QueryRows

    public init(requestId: UInt32, querySetId: UInt32, rows: QueryRows) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try decoder.storage.read(UInt32.self)
        self.querySetId = try decoder.storage.read(UInt32.self)
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
public struct UnsubscribeApplied: BSATNSpecialDecodable {
    public var requestId: UInt32
    public var querySetId: UInt32
    public var rows: QueryRows?

    public init(requestId: UInt32, querySetId: UInt32, rows: QueryRows?) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try decoder.storage.read(UInt32.self)
        self.querySetId = try decoder.storage.read(UInt32.self)
        let rowsTag = try decoder.storage.read(UInt8.self)
        switch rowsTag {
        case 0:
            self.rows = try QueryRows(fromBSATN: decoder)
        case 1:
            self.rows = nil
        default:
            throw BSATNDecodingError.invalidType
        }
    }
}

/// Rust: `SubscriptionError { request_id: Option<u32>, query_set_id: QuerySetId, error: Box<str> }`
public struct SubscriptionError: BSATNSpecialDecodable {
    public var requestId: UInt32?
    public var querySetId: UInt32
    public var error: String

    public init(requestId: UInt32?, querySetId: UInt32, error: String) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.error = error
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let requestTag = try decoder.storage.read(UInt8.self)
        switch requestTag {
        case 0:
            self.requestId = try decoder.storage.read(UInt32.self)
        case 1:
            self.requestId = nil
        default:
            throw BSATNDecodingError.invalidType
        }
        self.querySetId = try decoder.storage.read(UInt32.self)
        let container = try decoder.singleValueContainer()
        self.error = try container.decode(String.self)
    }
}

// MARK: - Transaction Updates

public struct TransactionUpdate: BSATNSpecialDecodable, Sendable, Decodable {
    public var querySets: [QuerySetUpdate]

    public init(querySets: [QuerySetUpdate]) {
        self.querySets = querySets
    }

    public init(from decoder: Decoder) throws {
        fatalError("Handled by BSATNSpecialDecodable")
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let count = try decoder.storage.read(UInt32.self)
        var sets: [QuerySetUpdate] = []
        sets.reserveCapacity(Int(count))
        for _ in 0..<count {
            sets.append(try QuerySetUpdate(fromBSATN: decoder))
        }
        self.querySets = sets
    }
}

public struct QuerySetUpdate: BSATNSpecialDecodable, Sendable {
    public var querySetId: UInt32
    public var tables: [TableUpdate]

    public init(querySetId: UInt32, tables: [TableUpdate]) {
        self.querySetId = querySetId
        self.tables = tables
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.querySetId = try decoder.storage.read(UInt32.self)
        let tableCount = try decoder.storage.read(UInt32.self)

        var tables: [TableUpdate] = []
        tables.reserveCapacity(Int(tableCount))
        for _ in 0..<tableCount {
            tables.append(try TableUpdate(fromBSATN: decoder))
        }
        self.tables = tables
    }
}

public struct TableUpdate: Sendable, BSATNSpecialDecodable {
    public var tableName: String
    public var rows: [TableUpdateRows]

    public init(tableName: String, rows: [TableUpdateRows]) {
        self.tableName = tableName
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let container = try decoder.singleValueContainer()
        self.tableName = try container.decode(String.self)

        let rowCount = try decoder.storage.read(UInt32.self)
        var rows: [TableUpdateRows] = []
        rows.reserveCapacity(Int(rowCount))
        for _ in 0..<rowCount {
            rows.append(try TableUpdateRows(fromBSATN: decoder))
        }
        self.rows = rows
    }
}

public enum TableUpdateRows: Sendable {
    case persistentTable(PersistentTableRows)
    case eventTable(EventTableRows)
}

extension TableUpdateRows: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            self = .persistentTable(try PersistentTableRows(fromBSATN: decoder))
        case 1:
            self = .eventTable(try EventTableRows(fromBSATN: decoder))
        default:
            throw BSATNDecodingError.unsupportedType
        }
    }
}

public struct PersistentTableRows: Sendable, BSATNSpecialDecodable {
    public var inserts: BsatnRowList
    public var deletes: BsatnRowList

    public init(inserts: BsatnRowList, deletes: BsatnRowList) {
        self.inserts = inserts
        self.deletes = deletes
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.inserts = try BsatnRowList(fromBSATN: decoder)
        self.deletes = try BsatnRowList(fromBSATN: decoder)
    }
}

public struct EventTableRows: Sendable, BSATNSpecialDecodable {
    public var events: BsatnRowList

    public init(events: BsatnRowList) {
        self.events = events
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.events = try BsatnRowList(fromBSATN: decoder)
    }
}

public struct QueryRows: BSATNSpecialDecodable, Sendable {
    public var tables: [SingleTableRows]

    public init(tables: [SingleTableRows]) {
        self.tables = tables
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let count = try decoder.storage.read(UInt32.self)
        var tables: [SingleTableRows] = []
        tables.reserveCapacity(Int(count))
        for _ in 0..<count {
            tables.append(try SingleTableRows(fromBSATN: decoder))
        }
        self.tables = tables
    }
}

public struct SingleTableRows: BSATNSpecialDecodable, Sendable {
    public var table: String
    public var rows: BsatnRowList

    public init(table: String, rows: BsatnRowList) {
        self.table = table
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let container = try decoder.singleValueContainer()
        self.table = try container.decode(String.self)
        self.rows = try BsatnRowList(fromBSATN: decoder)
    }
}

public enum RowSizeHint: BSATNSpecialDecodable, Sendable {
    case fixedSize(UInt16)
    case rowOffsets([UInt64])

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 0 {
            self = .fixedSize(try decoder.storage.read(UInt16.self))
        } else if tag == 1 {
            let count = try decoder.storage.read(UInt32.self)
            var offsets: [UInt64] = []
            offsets.reserveCapacity(Int(count))
            for _ in 0..<count {
                offsets.append(try decoder.storage.read(UInt64.self))
            }
            self = .rowOffsets(offsets)
        } else {
            throw BSATNDecodingError.unsupportedType
        }
    }
}

public struct BsatnRowList: BSATNSpecialDecodable, Sendable {
    public static let empty = BsatnRowList(sizeHint: .rowOffsets([]), rowsData: Data())

    public var sizeHint: RowSizeHint
    public var rowsData: Data

    init(sizeHint: RowSizeHint, rowsData: Data) {
        self.sizeHint = sizeHint
        self.rowsData = rowsData
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.sizeHint = try RowSizeHint(fromBSATN: decoder)
        let dataLen = try decoder.storage.read(UInt32.self)
        self.rowsData = try decoder.storage.readBytes(count: Int(dataLen))
    }
}

// MARK: - Query / Reducer / Procedure Results

public struct OneOffQueryResult: BSATNSpecialDecodable {
    public var requestId: UInt32
    public var result: QueryRowsResult

    public init(requestId: UInt32, result: QueryRowsResult) {
        self.requestId = requestId
        self.result = result
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try decoder.storage.read(UInt32.self)
        self.result = try QueryRowsResult(fromBSATN: decoder)
    }
}

public enum QueryRowsResult {
    case ok(QueryRows)
    case err(String)
}

extension QueryRowsResult: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            self = .ok(try QueryRows(fromBSATN: decoder))
        case 1:
            let container = try decoder.singleValueContainer()
            self = .err(try container.decode(String.self))
        default:
            throw BSATNDecodingError.unsupportedType
        }
    }
}

public struct ReducerResult: BSATNSpecialDecodable {
    public var requestId: UInt32
    public var timestamp: Int64
    public var result: ReducerOutcome

    public init(requestId: UInt32, timestamp: Int64, result: ReducerOutcome) {
        self.requestId = requestId
        self.timestamp = timestamp
        self.result = result
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.requestId = try decoder.storage.read(UInt32.self)
        self.timestamp = try decoder.storage.read(Int64.self)
        self.result = try ReducerOutcome(fromBSATN: decoder)
    }
}

public enum ReducerOutcome {
    case ok(ReducerOk)
    case okEmpty
    case err(Data)
    case internalError(String)
}

extension ReducerOutcome: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            self = .ok(try ReducerOk(fromBSATN: decoder))
        case 1:
            self = .okEmpty
        case 2:
            let len = try decoder.storage.read(UInt32.self)
            self = .err(try decoder.storage.readBytes(count: Int(len)))
        case 3:
            let container = try decoder.singleValueContainer()
            self = .internalError(try container.decode(String.self))
        default:
            throw BSATNDecodingError.unsupportedType
        }
    }
}

public struct ReducerOk: BSATNSpecialDecodable {
    public var retValue: Data
    public var transactionUpdate: TransactionUpdate

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let len = try decoder.storage.read(UInt32.self)
        self.retValue = try decoder.storage.readBytes(count: Int(len))
        self.transactionUpdate = try TransactionUpdate(fromBSATN: decoder)
    }
}

public struct ProcedureResult: BSATNSpecialDecodable {
    public var status: ProcedureStatus
    public var timestamp: Int64
    public var totalHostExecutionDuration: Int64
    public var requestId: UInt32

    public init(status: ProcedureStatus, timestamp: Int64, totalHostExecutionDuration: Int64, requestId: UInt32) {
        self.status = status
        self.timestamp = timestamp
        self.totalHostExecutionDuration = totalHostExecutionDuration
        self.requestId = requestId
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.status = try ProcedureStatus(fromBSATN: decoder)
        self.timestamp = try decoder.storage.read(Int64.self)
        self.totalHostExecutionDuration = try decoder.storage.read(Int64.self)
        self.requestId = try decoder.storage.read(UInt32.self)
    }
}

public enum ProcedureStatus {
    case returned(Data)
    case internalError(String)
}

extension ProcedureStatus: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            let len = try decoder.storage.read(UInt32.self)
            self = .returned(try decoder.storage.readBytes(count: Int(len)))
        case 1:
            let container = try decoder.singleValueContainer()
            self = .internalError(try container.decode(String.self))
        default:
            throw BSATNDecodingError.unsupportedType
        }
    }
}

// MARK: - Client Messages (Encoding)

/// Tag order for Rust `websocket::v2::ClientMessage`:
/// 0 = Subscribe
/// 1 = Unsubscribe
/// 2 = OneOffQuery
/// 3 = CallReducer
/// 4 = CallProcedure
public enum ClientMessage: Encodable {
    case subscribe(Subscribe)
    case unsubscribe(Unsubscribe)
    case oneOffQuery(OneOffQuery)
    case callReducer(CallReducer)
    case callProcedure(CallProcedure)

    public func encode(to encoder: Encoder) throws {}
}

extension ClientMessage: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        switch self {
        case .subscribe(let msg):
            encoder.storage.append(0 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .unsubscribe(let msg):
            encoder.storage.append(1 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .oneOffQuery(let msg):
            encoder.storage.append(2 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .callReducer(let msg):
            encoder.storage.append(3 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .callProcedure(let msg):
            encoder.storage.append(4 as UInt8)
            try msg.encodeBSATN(to: encoder)
        }
    }
}

/// Rust: `Subscribe { request_id: u32, query_set_id: QuerySetId, query_strings: Box<[Box<str>]> }`
public struct Subscribe: Encodable, BSATNSpecialEncodable {
    public var requestId: UInt32
    public var querySetId: UInt32
    public var queryStrings: [String]

    public init(queryStrings: [String], requestId: UInt32, querySetId: UInt32 = 1) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.queryStrings = queryStrings
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        encoder.storage.append(requestId)
        encoder.storage.append(querySetId)
        encoder.storage.append(UInt32(queryStrings.count))
        var singleVal = encoder.singleValueContainer()
        for query in queryStrings {
            try singleVal.encode(query)
        }
    }
}

/// Rust: `Unsubscribe { request_id: u32, query_set_id: QuerySetId, flags: u8 }`
public struct Unsubscribe: Encodable, BSATNSpecialEncodable {
    public var requestId: UInt32
    public var querySetId: UInt32
    public var flags: UInt8

    public init(requestId: UInt32, querySetId: UInt32, flags: UInt8 = 0) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.flags = flags
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        encoder.storage.append(requestId)
        encoder.storage.append(querySetId)
        encoder.storage.append(flags)
    }
}

/// Rust: `OneOffQuery { request_id: u32, query_string: Box<str> }`
public struct OneOffQuery: Encodable, BSATNSpecialEncodable {
    public var requestId: UInt32
    public var queryString: String

    public init(requestId: UInt32, queryString: String) {
        self.requestId = requestId
        self.queryString = queryString
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        encoder.storage.append(requestId)
        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(queryString)
    }
}

/// Rust: `CallReducer { request_id: u32, flags: u8, reducer: Box<str>, args: Bytes }`
public struct CallReducer: Encodable, BSATNSpecialEncodable {
    public var requestId: UInt32
    public var flags: UInt8
    public var reducer: String
    public var args: Data

    public init(requestId: UInt32, flags: UInt8, reducer: String, args: Data) {
        self.requestId = requestId
        self.flags = flags
        self.reducer = reducer
        self.args = args
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        encoder.storage.append(requestId)
        encoder.storage.append(flags)

        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(reducer)

        encoder.storage.append(UInt32(args.count))
        encoder.storage.append(args)
    }
}

/// Rust: `CallProcedure { request_id: u32, flags: u8, procedure: Box<str>, args: Bytes }`
public struct CallProcedure: Encodable, BSATNSpecialEncodable {
    public var requestId: UInt32
    public var flags: UInt8
    public var procedure: String
    public var args: Data

    public init(requestId: UInt32, flags: UInt8, procedure: String, args: Data) {
        self.requestId = requestId
        self.flags = flags
        self.procedure = procedure
        self.args = args
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        encoder.storage.append(requestId)
        encoder.storage.append(flags)

        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(procedure)

        encoder.storage.append(UInt32(args.count))
        encoder.storage.append(args)
    }
}
