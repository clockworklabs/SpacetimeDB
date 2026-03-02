import Foundation

/// Holds the local state of all SpacetimeDB tables, routing updates from the WebSocket down to each table.
@MainActor
public final class ClientCache: @unchecked Sendable {
    private var tables: [String: any SpacetimeTableCacheProtocol] = [:]

    public var registeredTableNames: Dictionary<String, any SpacetimeTableCacheProtocol>.Keys {
        tables.keys
    }

    public init() {}

    /// Registers a new table cache for a given table name.
    public func registerTable<T: Decodable & Sendable>(tableName: String, rowType: T.Type) {
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
        if self.tables[name] != nil {
            // Preserve the first registration to avoid replacing a live cache.
            return
        }
        self.tables[name] = cache
    }

    public func getTable(name: String) -> (any SpacetimeTableCacheProtocol)? {
        return self.tables[name]
    }

    public func getTableCache<T: Decodable & Sendable>(tableName: String) -> TableCache<T> {
        guard let table = self.tables[tableName] as? TableCache<T> else {
            fatalError("Table \(tableName) not registered or of wrong type.")
        }
        return table
    }

    /// Processes a TransactionUpdate payload from the network.
    public func applyTransactionUpdate(_ update: TransactionUpdate) {
        for querySet in update.querySets {
            for tableUpdate in querySet.tables {
                guard let tableCache = self.tables[tableUpdate.tableName] else {
                    continue
                }

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
                rows.append(data.subdata(in: offset..<end))
                offset += rowSize
            }

        case .rowOffsets(let offsets):
            for i in 0..<offsets.count {
                let start = Int(offsets[i])
                let end = (i + 1 < offsets.count) ? Int(offsets[i + 1]) : data.count
                rows.append(data.subdata(in: start..<end))
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
