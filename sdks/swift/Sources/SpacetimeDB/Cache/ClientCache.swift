import Foundation
import Synchronization

/// Holds the local state of all SpacetimeDB tables, routing updates from the WebSocket down to each table.
public final class ClientCache: Sendable {
    private struct State {
        var tables: [String: any SpacetimeTableCacheProtocol] = [:]
    }

    private let state: Mutex<State> = Mutex(State())

    public var registeredTableNames: [String] {
        state.withLock { state in
            Array(state.tables.keys)
        }
    }

    public init() {}

    /// Registers a new table cache for a given table name.
    public func registerTable<T: Decodable & Sendable>(tableName: String, rowType: T.Type) {
        state.withLock { state in
            if let existing = state.tables[tableName] {
                if existing is TableCache<T> {
                    // Idempotent re-registration: keep the existing cache instance so
                    // any replicated rows already loaded are preserved.
                    return
                }
                fatalError("Table \(tableName) already registered with a different row type.")
            }
            let cache = TableCache<T>(tableName: tableName)
            state.tables[tableName] = cache
        }
    }

    /// Registers a new table cache for a given table name.
    public func registerTable<T>(name: String, cache: TableCache<T>) {
        state.withLock { state in
            if state.tables[name] != nil {
                // Preserve the first registration to avoid replacing a live cache.
                return
            }
            state.tables[name] = cache
        }
    }

    public func getTable(name: String) -> (any SpacetimeTableCacheProtocol)? {
        state.withLock { state in
            state.tables[name]
        }
    }

    public func getTableCache<T: Decodable & Sendable>(tableName: String) -> TableCache<T> {
        guard let table = state.withLock({ state in
            state.tables[tableName] as? TableCache<T>
        }) else {
            fatalError("Table \(tableName) not registered or of wrong type.")
        }
        return table
    }

    /// Processes a TransactionUpdate payload from the network.
    public func applyTransactionUpdate(_ update: TransactionUpdate) {
        var modifiedTables = Set<String>()

        let tablesSnapshot = state.withLock { state in
            state.tables
        }

        for querySet in update.querySets {
            for tableUpdate in querySet.tables {
                let tableName = tableUpdate.tableName.rawValue
                guard let tableCache = tablesSnapshot[tableName] else {
                    continue
                }
                
                modifiedTables.insert(tableName)

                for rowUpdate in tableUpdate.rows {
                    switch rowUpdate {
                    case .persistentTable(let persistent):
                        let deleteRows = self.extractRows(from: persistent.deletes)
                        let insertRows = self.extractRows(from: persistent.inserts)
                        let pairedCount = min(deleteRows.count, insertRows.count)

                        
                        if pairedCount > 0 {
                            do {
                                try tableCache.handleBulkUpdate(
                                    oldRowBytesList: Array(deleteRows[..<pairedCount]),
                                    newRowBytesList: Array(insertRows[..<pairedCount])
                                )
                            } catch {
                                Log.cache.error("Failed to bulk update rows for table '\(tableName)': \(error.localizedDescription)")
                            }
                        }

                        if deleteRows.count > pairedCount {
                            do {
                                try tableCache.handleBulkDelete(rowBytesList: Array(deleteRows[pairedCount...]))
                            } catch {
                                Log.cache.error("Failed to bulk delete rows for table '\(tableName)': \(error.localizedDescription)")
                            }
                        }
                        if insertRows.count > pairedCount {
                            do {
                                try tableCache.handleBulkInsert(rowBytesList: Array(insertRows[pairedCount...]))
                            } catch {
                                Log.cache.error("Failed to bulk insert rows for table '\(tableName)': \(error.localizedDescription)")
                            }
                        }
                    case .eventTable:
                        break
                    }
                }
            }
        }
        
        // After all background processing is done, sync modified tables to MainActor for UI observers
        if !modifiedTables.isEmpty {
            Task { @MainActor in
                for tableName in modifiedTables {
                    if let table = tablesSnapshot[tableName] as? (any ThreadSafeSyncable) {
                        table.sync()
                    }
                }
            }
        }
    }

    private func extractRows(from rowList: BsatnRowList) -> [Data] {
        let sizeHint = rowList.sizeHint
        let data = rowList.rowsData
        var rows: [Data] = []

        switch sizeHint {
        case .fixedSize(let size):
            let rowSize = Int(size)
            if rowSize == 0 { return rows }

            var offset = 0
            while offset < data.count {
                let end = min(offset + rowSize, data.count)
                rows.append(data[offset..<end])
                offset += rowSize
            }

        case .rowOffsets(let offsets):
            for i in 0..<offsets.count {
                let start = Int(offsets[i])
                let end = (i + 1 < offsets.count) ? Int(offsets[i + 1]) : data.count
                rows.append(data[start..<end])
            }
        }

        return rows
    }

    private func processRows<S: Sequence>(_ rows: S, tableCache: any SpacetimeTableCacheProtocol, isInsert: Bool)
    where S.Element == Data {
        for rowData in rows {
            self.applyRow(data: rowData, tableCache: tableCache, isInsert: isInsert)
        }
    }

    private func applyRow(data: Data, tableCache: any SpacetimeTableCacheProtocol, isInsert: Bool) {
        do {
            if isInsert {
                try tableCache.handleInsert(rowBytes: data)
            } else {
                try tableCache.handleDelete(rowBytes: data)
            }
        } catch {
            Log.cache.error("Failed to decode row for table '\(tableCache.tableName)': \(error.localizedDescription)")
        }
    }

    private func applyRowUpdate(oldData: Data, newData: Data, tableCache: any SpacetimeTableCacheProtocol) {
        do {
            try tableCache.handleUpdate(oldRowBytes: oldData, newRowBytes: newData)
        } catch {
            Log.cache.error("Failed to decode updated row for table '\(tableCache.tableName)': \(error.localizedDescription)")
        }
    }
}

/// Helper protocol to allow ClientCache to call sync() without knowing the concrete type T
private protocol ThreadSafeSyncable {
    @MainActor func sync()
}

extension TableCache: ThreadSafeSyncable {}
