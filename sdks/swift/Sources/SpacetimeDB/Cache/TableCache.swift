import Foundation
import Observation

@MainActor
public protocol SpacetimeTableCacheProtocol: AnyObject, Sendable {
    var tableName: String { get }
    func handleInsert(rowBytes: Data) throws
    func handleDelete(rowBytes: Data) throws
    func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws
    func clear()
}

@MainActor
public final class TableDeltaHandle: @unchecked Sendable {
    private let cancelAction: () -> Void
    private var isCancelled = false

    init(cancelAction: @escaping () -> Void) {
        self.cancelAction = cancelAction
    }

    public func cancel() {
        guard !isCancelled else { return }
        isCancelled = true
        cancelAction()
    }
}

/// A reactive, thread-safe cache containing the local replica array of persistent rows for a given SpacetimeDB table
@MainActor
@Observable
public final class TableCache<T: Decodable & Sendable>: SpacetimeTableCacheProtocol {
    public let tableName: String
    
    // For SwiftUI observability via @Observable
    public private(set) var rows: [T] = []
    
    private let decoder = BSATNDecoder()
    private var rowCountsByBytes: [Data: Int] = [:]
    private var rowValueByBytes: [Data: T] = [:]
    private var insertCallbacks: [UUID: (T) -> Void] = [:]
    private var deleteCallbacks: [UUID: (T) -> Void] = [:]
    private var updateCallbacks: [UUID: (T, T) -> Void] = [:]
    
    public init(tableName: String) {
        self.tableName = tableName
    }
    
    public func handleInsert(rowBytes: Data) throws {
        do {
            let row = try decoder.decode(T.self, from: rowBytes)
            rowValueByBytes[rowBytes] = row
            rowCountsByBytes[rowBytes, default: 0] += 1
            updatePublishedRows()
            for callback in insertCallbacks.values {
                callback(row)
            }
        } catch {
            Log.cache.error("Failed to decode row for table '\(self.tableName)': \(error.localizedDescription)")
            Log.cache.debug("Raw bytes (\(rowBytes.count)): \(rowBytes.map { String(format: "%02x", $0) }.joined())")
            throw error
        }
    }
    
    public func handleDelete(rowBytes: Data) throws {
        let deletedRow = rowValueByBytes[rowBytes]
        guard let existing = rowCountsByBytes[rowBytes] else {
            return
        }
        if existing <= 1 {
            rowCountsByBytes.removeValue(forKey: rowBytes)
            rowValueByBytes.removeValue(forKey: rowBytes)
        } else {
            rowCountsByBytes[rowBytes] = existing - 1
        }
        updatePublishedRows()
        if let deletedRow {
            for callback in deleteCallbacks.values {
                callback(deletedRow)
            }
        }
    }

    public func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws {
        guard let existing = rowCountsByBytes[oldRowBytes], existing > 0, let oldRow = rowValueByBytes[oldRowBytes] else {
            try handleInsert(rowBytes: newRowBytes)
            return
        }

        let newRow = try decoder.decode(T.self, from: newRowBytes)

        if existing <= 1 {
            rowCountsByBytes.removeValue(forKey: oldRowBytes)
            rowValueByBytes.removeValue(forKey: oldRowBytes)
        } else {
            rowCountsByBytes[oldRowBytes] = existing - 1
        }

        rowValueByBytes[newRowBytes] = newRow
        rowCountsByBytes[newRowBytes, default: 0] += 1
        updatePublishedRows()

        for callback in updateCallbacks.values {
            callback(oldRow, newRow)
        }
    }
    
    public func clear() {
        rowCountsByBytes.removeAll()
        rowValueByBytes.removeAll()
        updatePublishedRows()
    }

    @discardableResult
    public func onInsert(_ callback: @escaping (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        insertCallbacks[id] = callback
        return TableDeltaHandle { [weak self] in
            self?.insertCallbacks.removeValue(forKey: id)
        }
    }

    @discardableResult
    public func onDelete(_ callback: @escaping (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        deleteCallbacks[id] = callback
        return TableDeltaHandle { [weak self] in
            self?.deleteCallbacks.removeValue(forKey: id)
        }
    }

    @discardableResult
    public func onUpdate(_ callback: @escaping (T, T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        updateCallbacks[id] = callback
        return TableDeltaHandle { [weak self] in
            self?.updateCallbacks.removeValue(forKey: id)
        }
    }
    
    private func updatePublishedRows() {
        let sortedKeys = rowCountsByBytes.keys.sorted { lhs, rhs in
            lhs.lexicographicallyPrecedes(rhs)
        }
        var flattened: [T] = []
        for key in sortedKeys {
            guard let count = rowCountsByBytes[key], let row = rowValueByBytes[key], count > 0 else {
                continue
            }
            flattened.reserveCapacity(flattened.count + count)
            for _ in 0..<count {
                flattened.append(row)
            }
        }
        self.rows = flattened
    }
}
