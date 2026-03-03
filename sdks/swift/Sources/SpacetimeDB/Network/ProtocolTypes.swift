import Foundation

// MARK: - Shared Protocol Types

public struct SingleTableRows: BSATNSpecialDecodable, Sendable, Decodable {
    public var table: RawIdentifier
    public var rows: BsatnRowList

    public init(table: RawIdentifier, rows: BsatnRowList) {
        self.table = table
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.table = try RawIdentifier(fromBSATN: decoder)
        self.rows = try BsatnRowList(fromBSATN: decoder)
    }
}

public struct QueryRows: BSATNSpecialDecodable, Sendable, Decodable {
    public var tables: [SingleTableRows]

    public init(tables: [SingleTableRows]) {
        self.tables = tables
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.tables = try decoder.storage.readArray { try SingleTableRows(fromBSATN: decoder) }
    }
}

public struct TransactionUpdate: BSATNSpecialDecodable, Sendable, Decodable {
    public var querySets: [QuerySetUpdate]

    public init(querySets: [QuerySetUpdate]) {
        self.querySets = querySets
    }

    public init(from decoder: Decoder) throws {
        fatalError("Handled by BSATNSpecialDecodable")
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.querySets = try decoder.storage.readArray { try QuerySetUpdate(fromBSATN: decoder) }
    }
}

public struct QuerySetUpdate: BSATNSpecialDecodable, Sendable, Decodable {
    public var querySetId: QuerySetId
    public var tables: [TableUpdate]

    public init(querySetId: QuerySetId, tables: [TableUpdate]) {
        self.querySetId = querySetId
        self.tables = tables
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.querySetId = try QuerySetId(fromBSATN: decoder)
        self.tables = try decoder.storage.readArray { try TableUpdate(fromBSATN: decoder) }
    }
}

public struct TableUpdate: Sendable, BSATNSpecialDecodable, Decodable {
    public var tableName: RawIdentifier
    public var rows: [TableUpdateRows]

    public init(tableName: RawIdentifier, rows: [TableUpdateRows]) {
        self.tableName = tableName
        self.rows = rows
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.tableName = try RawIdentifier(fromBSATN: decoder)
        self.rows = try decoder.storage.readArray { try TableUpdateRows(fromBSATN: decoder) }
    }
}

public enum TableUpdateRows: Sendable, Decodable {
    case persistentTable(PersistentTableRows)
    case eventTable(EventTableRows)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }
}

extension TableUpdateRows: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0: return .persistentTable(try PersistentTableRows(fromBSATN: decoder))
            case 1: return .eventTable(try EventTableRows(fromBSATN: decoder))
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct PersistentTableRows: Sendable, BSATNSpecialDecodable, Decodable {
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

public struct EventTableRows: Sendable, BSATNSpecialDecodable, Decodable {
    public var events: BsatnRowList

    public init(events: BsatnRowList) {
        self.events = events
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.events = try BsatnRowList(fromBSATN: decoder)
    }
}

public enum RowSizeHint: BSATNSpecialDecodable, Sendable, Decodable {
    case fixedSize(UInt16)
    case rowOffsets([UInt64])

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0: return .fixedSize(try decoder.storage.read(UInt16.self))
            case 1: return .rowOffsets(try decoder.storage.readArray { try decoder.storage.read(UInt64.self) })
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct BsatnRowList: BSATNSpecialDecodable, Sendable, Decodable {
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

public enum ReducerOutcome: Decodable {
    case ok(ReducerOk)
    case okEmpty
    case err(Data)
    case internalError(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }
}

extension ReducerOutcome: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0: return .ok(try ReducerOk(fromBSATN: decoder))
            case 1: return .okEmpty
            case 2:
                let len = try decoder.storage.read(UInt32.self)
                return .err(try decoder.storage.readBytes(count: Int(len)))
            case 3:
                let container = try decoder.singleValueContainer()
                return .internalError(try container.decode(String.self))
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct ReducerOk: BSATNSpecialDecodable, Decodable {
    public var retValue: Data
    public var transactionUpdate: TransactionUpdate

    init(fromBSATN decoder: _BSATNDecoder) throws {
        let len = try decoder.storage.read(UInt32.self)
        self.retValue = try decoder.storage.readBytes(count: Int(len))
        self.transactionUpdate = try TransactionUpdate(fromBSATN: decoder)
    }
}

public enum ProcedureStatus: Decodable {
    case returned(Data)
    case internalError(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }
}

extension ProcedureStatus: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        try self = decoder.storage.readTaggedEnum { tag in
            switch tag {
            case 0:
                let len = try decoder.storage.read(UInt32.self)
                return .returned(try decoder.storage.readBytes(count: Int(len)))
            case 1:
                let container = try decoder.singleValueContainer()
                return .internalError(try container.decode(String.self))
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}
