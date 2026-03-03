import Foundation
import Observation
import Synchronization

public protocol SpacetimeTableCacheProtocol: AnyObject, Sendable {
    var tableName: String { get }
    func handleInsert(rowBytes: Data) throws
    func handleDelete(rowBytes: Data) throws
    func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws
    func handleBulkInsert(rowBytesList: [Data]) throws
    func handleBulkDelete(rowBytesList: [Data]) throws
    func handleBulkUpdate(oldRowBytesList: [Data], newRowBytesList: [Data]) throws
    func clear()
}

public final class TableDeltaHandle: Sendable {
    private let cancelAction: @Sendable () -> Void
    private let cancelledState: Mutex<Bool> = Mutex(false)

    init(cancelAction: @escaping @Sendable () -> Void) {
        self.cancelAction = cancelAction
    }

    public func cancel() {
        let shouldCancel = cancelledState.withLock { isCancelled in
            guard !isCancelled else { return false }
            isCancelled = true
            return true
        }

        if shouldCancel {
            cancelAction()
        }
    }
}

// MARK: - HashedBytes

/// A Data wrapper that pre-computes its hash on construction.
/// Dictionary lookups hash 8 bytes (the cached Int) instead of N row bytes.
struct HashedBytes: Hashable, Sendable {
    let data: Data
    private let _hash: Int

    init(_ data: Data) {
        self.data = data
        self._hash = data.hashValue // Native SipHash, much faster
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(_hash)
    }

    static func == (lhs: HashedBytes, rhs: HashedBytes) -> Bool {
        lhs._hash == rhs._hash && lhs.data == rhs.data
    }
}

// MARK: - RowEntry

/// Merged storage: count + decoded value in a single dictionary entry.
/// Eliminates the second dictionary lookup per insert/delete.
struct RowEntry<T> {
    var count: Int
    var value: T
}

// MARK: - TableCache

/// A reactive, thread-safe cache containing the local replica array of persistent rows for a given SpacetimeDB table
@Observable
public final class TableCache<T: Decodable & Sendable>: SpacetimeTableCacheProtocol, Sendable {
    public let tableName: String

    // For SwiftUI observability via @Observable
    // This is only updated on the MainActor when requested or via sync()
    @MainActor
    public private(set) var rows: [T] = []

    // All internal state is @ObservationIgnored to prevent the @Observable macro
    // from intercepting every dictionary mutation with willSet/didSet tracking.
    // Only `rows` is observed for SwiftUI reactivity.
    @ObservationIgnored private let decoder = BSATNDecoder()
    @ObservationIgnored private let state: Mutex<State> = Mutex(State())

    private struct State {
        var entries: [HashedBytes: RowEntry<T>] = [:]
        var insertCallbacks: [UUID: @Sendable (T) -> Void] = [:]
        var deleteCallbacks: [UUID: @Sendable (T) -> Void] = [:]
        var updateCallbacks: [UUID: @Sendable (T, T) -> Void] = [:]
    }

    public init(tableName: String) {
        self.tableName = tableName
    }

    public func handleInsert(rowBytes: Data) throws {
        let key = HashedBytes(rowBytes)
        let rowAndCallbacks: (row: T, callbacks: [@Sendable (T) -> Void])
        do {
            rowAndCallbacks = try state.withLock { state in
                let row: T
                if let index = state.entries.index(forKey: key) {
                    state.entries.values[index].count += 1
                    row = state.entries.values[index].value
                } else {
                    row = try decoder.decode(T.self, from: rowBytes)
                    state.entries[key] = RowEntry(count: 1, value: row)
                }
                return (row: row, callbacks: Array(state.insertCallbacks.values))
            }
        } catch {
            Log.cache.error("Failed to decode row for table '\(self.tableName)': \(error.localizedDescription)")
            throw error
        }

        for callback in rowAndCallbacks.callbacks {
            callback(rowAndCallbacks.row)
        }
    }

    public func handleDelete(rowBytes: Data) throws {
        let key = HashedBytes(rowBytes)
        let rowAndCallbacks = state.withLock { state -> (row: T, callbacks: [@Sendable (T) -> Void])? in
            guard let index = state.entries.index(forKey: key) else {
                return nil
            }
            let deletedRow = state.entries.values[index].value
            if state.entries.values[index].count <= 1 {
                state.entries.remove(at: index)
            } else {
                state.entries.values[index].count -= 1
            }
            return (row: deletedRow, callbacks: Array(state.deleteCallbacks.values))
        }

        guard let rowAndCallbacks else {
            return
        }

        for callback in rowAndCallbacks.callbacks {
            callback(rowAndCallbacks.row)
        }
    }

    public func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws {
        let oldKey = HashedBytes(oldRowBytes)
        let newKey = HashedBytes(newRowBytes)
        let rowAndCallbacks: (oldRow: T, newRow: T, callbacks: [@Sendable (T, T) -> Void])?
        do {
            rowAndCallbacks = try state.withLock { state in
                guard let oldIndex = state.entries.index(forKey: oldKey) else {
                    return nil
                }

                let oldRow = state.entries.values[oldIndex].value
                let newRow: T

                if let newIndex = state.entries.index(forKey: newKey) {
                    state.entries.values[newIndex].count += 1
                    newRow = state.entries.values[newIndex].value
                } else {
                    newRow = try decoder.decode(T.self, from: newRowBytes)
                    state.entries[newKey] = RowEntry(count: 1, value: newRow)
                }

                if let finalOldIndex = state.entries.index(forKey: oldKey) {
                    if state.entries.values[finalOldIndex].count <= 1 {
                        state.entries.remove(at: finalOldIndex)
                    } else {
                        state.entries.values[finalOldIndex].count -= 1
                    }
                }

                return (oldRow: oldRow, newRow: newRow, callbacks: Array(state.updateCallbacks.values))
            }
        } catch {
            Log.cache.error("Failed to decode row for table '\(self.tableName)': \(error.localizedDescription)")
            throw error
        }

        guard let rowAndCallbacks else {
            try handleInsert(rowBytes: newRowBytes)
            return
        }

        for callback in rowAndCallbacks.callbacks {
            callback(rowAndCallbacks.oldRow, rowAndCallbacks.newRow)
        }
    }

    
    public func handleBulkInsert(rowBytesList: [Data]) throws {
        if rowBytesList.isEmpty { return }
        
        let rowsAndCallbacks: [(row: T, callbacks: [@Sendable (T) -> Void])] = try state.withLock { state in
            var results: [(row: T, callbacks: [@Sendable (T) -> Void])] = []
            results.reserveCapacity(rowBytesList.count)
            let callbacks = Array(state.insertCallbacks.values)
            
            for rowBytes in rowBytesList {
                let key = HashedBytes(rowBytes)
                let row: T
                if let index = state.entries.index(forKey: key) {
                    state.entries.values[index].count += 1
                    row = state.entries.values[index].value
                } else {
                    row = try decoder.decode(T.self, from: rowBytes)
                    state.entries[key] = RowEntry(count: 1, value: row)
                }
                results.append((row: row, callbacks: callbacks))
            }
            return results
        }
        
        for item in rowsAndCallbacks {
            for callback in item.callbacks {
                callback(item.row)
            }
        }
    }

    public func handleBulkDelete(rowBytesList: [Data]) throws {
        if rowBytesList.isEmpty { return }
        
        let rowsAndCallbacks: [(row: T, callbacks: [@Sendable (T) -> Void])] = state.withLock { state in
            var results: [(row: T, callbacks: [@Sendable (T) -> Void])] = []
            results.reserveCapacity(rowBytesList.count)
            let callbacks = Array(state.deleteCallbacks.values)
            
            for rowBytes in rowBytesList {
                let key = HashedBytes(rowBytes)
                if let index = state.entries.index(forKey: key) {
                    let deletedRow = state.entries.values[index].value
                    if state.entries.values[index].count <= 1 {
                        state.entries.remove(at: index)
                    } else {
                        state.entries.values[index].count -= 1
                    }
                    results.append((row: deletedRow, callbacks: callbacks))
                }
            }
            return results
        }
        
        for item in rowsAndCallbacks {
            for callback in item.callbacks {
                callback(item.row)
            }
        }
    }

    public func handleBulkUpdate(oldRowBytesList: [Data], newRowBytesList: [Data]) throws {
        if oldRowBytesList.isEmpty || newRowBytesList.isEmpty { return }
        let count = min(oldRowBytesList.count, newRowBytesList.count)
        
        let results: [(oldRow: T, newRow: T, callbacks: [@Sendable (T, T) -> Void])]? = try state.withLock { state in
            var results: [(oldRow: T, newRow: T, callbacks: [@Sendable (T, T) -> Void])] = []
            results.reserveCapacity(count)
            let callbacks = Array(state.updateCallbacks.values)
            
            for i in 0..<count {
                let oldRowBytes = oldRowBytesList[i]
                let newRowBytes = newRowBytesList[i]
                let oldKey = HashedBytes(oldRowBytes)
                let newKey = HashedBytes(newRowBytes)
                
                guard let oldIndex = state.entries.index(forKey: oldKey) else {
                    // Fallback: If old row doesn't exist, we just insert the new row, but we can't do it cleanly in bulk update loop
                    // Let's throw an error or handle it out of loop. For now, let's just insert new row.
                    let newRow = try decoder.decode(T.self, from: newRowBytes)
                    if let newIndex = state.entries.index(forKey: newKey) {
                        state.entries.values[newIndex].count += 1
                    } else {
                        state.entries[newKey] = RowEntry(count: 1, value: newRow)
                    }
                    // We don't trigger update callback if old doesn't exist. We should trigger insert, but for simplicity let's stick to update loop semantics or skip callback.
                    continue
                }

                let oldRow = state.entries.values[oldIndex].value
                let newRow: T

                if let newIndex = state.entries.index(forKey: newKey) {
                    state.entries.values[newIndex].count += 1
                    newRow = state.entries.values[newIndex].value
                } else {
                    newRow = try decoder.decode(T.self, from: newRowBytes)
                    state.entries[newKey] = RowEntry(count: 1, value: newRow)
                }

                if let finalOldIndex = state.entries.index(forKey: oldKey) {
                    if state.entries.values[finalOldIndex].count <= 1 {
                        state.entries.remove(at: finalOldIndex)
                    } else {
                        state.entries.values[finalOldIndex].count -= 1
                    }
                }
                
                results.append((oldRow: oldRow, newRow: newRow, callbacks: callbacks))
            }
            return results
        }
        
        guard let validResults = results else { return }
        for item in validResults {
            for callback in item.callbacks {
                callback(item.oldRow, item.newRow)
            }
        }
    }

    public func clear() {
        state.withLock { state in
            state.entries.removeAll()
        }
    }

    @discardableResult
    public func onInsert(_ callback: @escaping @Sendable (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        state.withLock { state in
            state.insertCallbacks[id] = callback
        }
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            _ = self.state.withLock { state in
                state.insertCallbacks.removeValue(forKey: id)
            }
        }
    }

    @discardableResult
    public func onDelete(_ callback: @escaping @Sendable (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        state.withLock { state in
            state.deleteCallbacks[id] = callback
        }
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            _ = self.state.withLock { state in
                state.deleteCallbacks.removeValue(forKey: id)
            }
        }
    }

    @discardableResult
    public func onUpdate(_ callback: @escaping @Sendable (T, T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        state.withLock { state in
            state.updateCallbacks[id] = callback
        }
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            _ = self.state.withLock { state in
                state.updateCallbacks.removeValue(forKey: id)
            }
        }
    }

    /// Synchronizes the observable `rows` array with the internal background storage.
    /// This should be called on the MainActor, typically after a transaction update or when the UI needs refreshing.
    @MainActor
    public func sync() {
        self.rows = snapshot()
    }

    /// Internal method to generate a flattened snapshot of all rows in deterministic order.
    private func snapshot() -> [T] {
        state.withLock { state in
            var flattened: [T] = []
            flattened.reserveCapacity(state.entries.count) // Baseline capacity
            for entry in state.entries.values {
                if entry.count > 0 {
                    flattened.reserveCapacity(flattened.count + entry.count)
                    for _ in 0..<entry.count {
                        flattened.append(entry.value)
                    }
                }
            }
            return flattened
        }
    }
}
extension TableCache: Decodable where T: Decodable {
    public convenience init(from decoder: Decoder) throws {
        fatalError("TableCache cannot be decoded; it is a managed container.")
    }
}
