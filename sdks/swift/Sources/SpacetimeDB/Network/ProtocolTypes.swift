import Foundation

// MARK: - Shared Protocol Types

public struct SingleTableRows: BSATNSpecialDecodable, Sendable, Decodable {
    public var table: RawIdentifier
    public var rows: BsatnRowList

    public init(table: RawIdentifier, rows: BsatnRowList) {
        self.table = table
        self.rows = rows
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> SingleTableRows {
        return SingleTableRows(
            table: try RawIdentifier.decodeBSATN(from: &reader),
            rows: try BsatnRowList.decodeBSATN(from: &reader)
        )
    }
}

public struct QueryRows: BSATNSpecialDecodable, Sendable, Decodable {
    public var tables: [SingleTableRows]

    public init(tables: [SingleTableRows]) {
        self.tables = tables
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> QueryRows {
        return QueryRows(
            tables: try reader.readArray { reader in try SingleTableRows.decodeBSATN(from: &reader) }
        )
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

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> TransactionUpdate {
        return TransactionUpdate(
            querySets: try reader.readArray { reader in try QuerySetUpdate.decodeBSATN(from: &reader) }
        )
    }
}

public struct QuerySetUpdate: BSATNSpecialDecodable, Sendable, Decodable {
    public var querySetId: QuerySetId
    public var tables: [TableUpdate]

    public init(querySetId: QuerySetId, tables: [TableUpdate]) {
        self.querySetId = querySetId
        self.tables = tables
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> QuerySetUpdate {
        return QuerySetUpdate(
            querySetId: try QuerySetId.decodeBSATN(from: &reader),
            tables: try reader.readArray { reader in try TableUpdate.decodeBSATN(from: &reader) }
        )
    }
}

public struct TableUpdate: Sendable, BSATNSpecialDecodable, Decodable {
    public var tableName: RawIdentifier
    public var rows: [TableUpdateRows]

    public init(tableName: RawIdentifier, rows: [TableUpdateRows]) {
        self.tableName = tableName
        self.rows = rows
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> TableUpdate {
        return TableUpdate(
            tableName: try RawIdentifier.decodeBSATN(from: &reader),
            rows: try reader.readArray { reader in try TableUpdateRows.decodeBSATN(from: &reader) }
        )
    }
}

public enum TableUpdateRows: Sendable, Decodable, BSATNSpecialDecodable {
    case persistentTable(PersistentTableRows)
    case eventTable(EventTableRows)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> TableUpdateRows {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0: return .persistentTable(try PersistentTableRows.decodeBSATN(from: &reader))
            case 1: return .eventTable(try EventTableRows.decodeBSATN(from: &reader))
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

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> PersistentTableRows {
        return PersistentTableRows(
            inserts: try BsatnRowList.decodeBSATN(from: &reader),
            deletes: try BsatnRowList.decodeBSATN(from: &reader)
        )
    }
}

public struct EventTableRows: Sendable, BSATNSpecialDecodable, Decodable {
    public var events: BsatnRowList

    public init(events: BsatnRowList) {
        self.events = events
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> EventTableRows {
        return EventTableRows(
            events: try BsatnRowList.decodeBSATN(from: &reader)
        )
    }
}

public enum RowSizeHint: BSATNSpecialDecodable, Sendable, Decodable {
    case fixedSize(UInt16)
    case rowOffsets([UInt64])

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> RowSizeHint {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0: return .fixedSize(try reader.read(UInt16.self))
            case 1: return .rowOffsets(try reader.readArray { reader in try reader.read(UInt64.self) })
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

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> BsatnRowList {
        let sizeHint = try RowSizeHint.decodeBSATN(from: &reader)
        let dataLen = try reader.read(UInt32.self)
        let rowsData = try reader.readBytes(count: Int(dataLen))
        return BsatnRowList(sizeHint: sizeHint, rowsData: rowsData)
    }
}

public enum ReducerOutcome: Decodable, BSATNSpecialDecodable, Sendable {
    case ok(ReducerOk)
    case okEmpty
    case err(Data)
    case internalError(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ReducerOutcome {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0: return .ok(try ReducerOk.decodeBSATN(from: &reader))
            case 1: return .okEmpty
            case 2:
                let len = try reader.read(UInt32.self)
                return .err(try reader.readBytes(count: Int(len)))
            case 3:
                return .internalError(try reader.readString())
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}

public struct ReducerOk: BSATNSpecialDecodable, Decodable, Sendable {
    public var retValue: Data
    public var transactionUpdate: TransactionUpdate

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ReducerOk {
        let len = try reader.read(UInt32.self)
        let retValue = try reader.readBytes(count: Int(len))
        let transactionUpdate = try TransactionUpdate.decodeBSATN(from: &reader)
        return ReducerOk(retValue: retValue, transactionUpdate: transactionUpdate)
    }
}

public enum ProcedureStatus: Decodable, BSATNSpecialDecodable, Sendable {
    case returned(Data)
    case internalError(String)

    public init(from decoder: Decoder) throws {
        fatalError("Use BSATNSpecialDecodable")
    }

    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ProcedureStatus {
        try reader.readTaggedEnum { reader, tag in
            switch tag {
            case 0:
                let len = try reader.read(UInt32.self)
                return .returned(try reader.readBytes(count: Int(len)))
            case 1:
                return .internalError(try reader.readString())
            default: throw BSATNDecodingError.unsupportedType
            }
        }
    }
}
