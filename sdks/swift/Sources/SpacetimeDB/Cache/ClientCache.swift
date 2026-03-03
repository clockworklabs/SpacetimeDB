import Foundation

/// Holds the local state of all SpacetimeDB tables, routing updates from the WebSocket down to each table.
public final class ClientCache: @unchecked Sendable {
    private let lock = UnfairLock()
    private var tables: [String: any SpacetimeTableCacheProtocol] = [:]

    public var registeredTableNames: [String] {
        lock.lock()
        defer { lock.unlock() }
        return Array(tables.keys)
    }

    public init() {}

    /// Registers a new table cache for a given table name.
    public func registerTable<T: Decodable & Sendable>(tableName: String, rowType: T.Type) {
        lock.lock()
        defer { lock.unlock() }
        if let existing = self.tables[tableName] {
            if existing is TableCache<T> {
                // Idempotent re-registration: keep the existing cache instance so
                // any replicated rows already loaded are preserved.
                return
            }
            fatalError("Table \(tableName) already registered with a different row type.")
        }
        let cache = TableCache<T>(tableName: tableName)
        self.tables[tableName] = cache
    }

    /// Registers a new table cache for a given table name.
    public func registerTable<T>(name: String, cache: TableCache<T>) {
        lock.lock()
        defer { lock.unlock() }
        if self.tables[name] != nil {
            // Preserve the first registration to avoid replacing a live cache.
            return
        }
        self.tables[name] = cache
    }

    public func getTable(name: String) -> (any SpacetimeTableCacheProtocol)? {
        lock.lock()
        defer { lock.unlock() }
        return self.tables[name]
    }

    public func getTableCache<T: Decodable & Sendable>(tableName: String) -> TableCache<T> {
        lock.lock()
        defer { lock.unlock() }
        guard let table = self.tables[tableName] as? TableCache<T> else {
            fatalError("Table \(tableName) not registered or of wrong type.")
        }
        return table
    }

    /// Processes a TransactionUpdate payload from the network.
    public func applyTransactionUpdate(_ update: TransactionUpdate) {
        var modifiedTables = Set<String>()
        
        lock.lock()
        let tablesSnapshot = tables
        lock.unlock()

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
                            for idx in 0..<pairedCount {
                                self.applyRowUpdate(
                                    oldData: deleteRows[idx],
                                    newData: insertRows[idx],
                                    tableCache: tableCache
                                )
                            }
                        }

                        if deleteRows.count > pairedCount {
                            self.processRows(deleteRows[pairedCount...], tableCache: tableCache, isInsert: false)
                        }
                        if insertRows.count > pairedCount {
                            self.processRows(insertRows[pairedCount...], tableCache: tableCache, isInsert: true)
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
