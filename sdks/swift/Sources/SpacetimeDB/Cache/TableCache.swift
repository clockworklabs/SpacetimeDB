import Foundation
import Observation

public protocol SpacetimeTableCacheProtocol: AnyObject, Sendable {
    var tableName: String { get }
    func handleInsert(rowBytes: Data) throws
    func handleDelete(rowBytes: Data) throws
    func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws
    func clear()
}

public final class TableDeltaHandle: @unchecked Sendable {
    private let cancelAction: () -> Void
    private let lock = NSLock()
    private var isCancelled = false

    init(cancelAction: @escaping () -> Void) {
        self.cancelAction = cancelAction
    }

    public func cancel() {
        lock.lock()
        defer { lock.unlock() }
        guard !isCancelled else { return }
        isCancelled = true
        cancelAction()
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
public final class TableCache<T: Decodable & Sendable>: SpacetimeTableCacheProtocol, @unchecked Sendable {
    public let tableName: String

    // For SwiftUI observability via @Observable
    // This is only updated on the MainActor when requested or via sync()
    @MainActor
    public private(set) var rows: [T] = []

    // All internal state is @ObservationIgnored to prevent the @Observable macro
    // from intercepting every dictionary mutation with willSet/didSet tracking.
    // Only `rows` is observed for SwiftUI reactivity.
    @ObservationIgnored private let lock = NSLock()
    @ObservationIgnored private let decoder = BSATNDecoder()
    @ObservationIgnored private var entries: [HashedBytes: RowEntry<T>] = [:]
    @ObservationIgnored private var insertCallbacks: [UUID: (T) -> Void] = [:]
    @ObservationIgnored private var deleteCallbacks: [UUID: (T) -> Void] = [:]
    @ObservationIgnored private var updateCallbacks: [UUID: (T, T) -> Void] = [:]

    public init(tableName: String) {
        self.tableName = tableName
    }

    public func handleInsert(rowBytes: Data) throws {
        let key = HashedBytes(rowBytes)
        let row: T
        lock.lock()
        if let index = entries.index(forKey: key) {
            entries.values[index].count += 1
            row = entries.values[index].value
        } else {
            do {
                row = try decoder.decode(T.self, from: rowBytes)
                entries[key] = RowEntry(count: 1, value: row)
            } catch {
                lock.unlock()
                Log.cache.error("Failed to decode row for table '\(self.tableName)': \(error.localizedDescription)")
                throw error
            }
        }
        let callbacks = Array(insertCallbacks.values)
        lock.unlock()

        for callback in callbacks {
            callback(row)
        }
    }

    public func handleDelete(rowBytes: Data) throws {
        let key = HashedBytes(rowBytes)
        lock.lock()
        guard let index = entries.index(forKey: key) else {
            lock.unlock()
            return
        }
        let deletedRow = entries.values[index].value
        if entries.values[index].count <= 1 {
            entries.remove(at: index)
        } else {
            entries.values[index].count -= 1
        }
        let callbacks = Array(deleteCallbacks.values)
        lock.unlock()

        for callback in callbacks {
            callback(deletedRow)
        }
    }

    public func handleUpdate(oldRowBytes: Data, newRowBytes: Data) throws {
        let oldKey = HashedBytes(oldRowBytes)
        let newKey = HashedBytes(newRowBytes)
        lock.lock()
        
        guard let oldIndex = entries.index(forKey: oldKey) else {
            lock.unlock()
            try handleInsert(rowBytes: newRowBytes)
            return
        }

        let oldRow = entries.values[oldIndex].value
        let newRow: T
        
        if let newIndex = entries.index(forKey: newKey) {
            entries.values[newIndex].count += 1
            newRow = entries.values[newIndex].value
        } else {
            do {
                newRow = try decoder.decode(T.self, from: newRowBytes)
                entries[newKey] = RowEntry(count: 1, value: newRow)
            } catch {
                lock.unlock()
                Log.cache.error("Failed to decode row for table '\(self.tableName)': \(error.localizedDescription)")
                throw error
            }
        }

        // Re-fetch old index strictly speaking because newKey insertion might have rehashed.
        // However, we can just remove/decrement now.
        if let finalOldIndex = entries.index(forKey: oldKey) {
            if entries.values[finalOldIndex].count <= 1 {
                entries.remove(at: finalOldIndex)
            } else {
                entries.values[finalOldIndex].count -= 1
            }
        }

        let callbacks = Array(updateCallbacks.values)
        lock.unlock()

        for callback in callbacks {
            callback(oldRow, newRow)
        }
    }

    public func clear() {
        lock.lock()
        entries.removeAll()
        lock.unlock()
    }

    @discardableResult
    public func onInsert(_ callback: @escaping (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        lock.lock()
        insertCallbacks[id] = callback
        lock.unlock()
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            self.lock.lock()
            self.insertCallbacks.removeValue(forKey: id)
            self.lock.unlock()
        }
    }

    @discardableResult
    public func onDelete(_ callback: @escaping (T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        lock.lock()
        deleteCallbacks[id] = callback
        lock.unlock()
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            self.lock.lock()
            self.deleteCallbacks.removeValue(forKey: id)
            self.lock.unlock()
        }
    }

    @discardableResult
    public func onUpdate(_ callback: @escaping (T, T) -> Void) -> TableDeltaHandle {
        let id = UUID()
        lock.lock()
        updateCallbacks[id] = callback
        lock.unlock()
        return TableDeltaHandle { [weak self] in
            guard let self else { return }
            self.lock.lock()
            self.updateCallbacks.removeValue(forKey: id)
            self.lock.unlock()
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
        lock.lock()
        defer { lock.unlock() }

        var flattened: [T] = []
        flattened.reserveCapacity(entries.count) // Baseline capacity
        for entry in entries.values {
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
extension TableCache: Decodable where T: Decodable {
    public convenience init(from decoder: Decoder) throws {
        fatalError("TableCache cannot be decoded; it is a managed container.")
    }
}
