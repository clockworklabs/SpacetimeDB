import Foundation
import Observation

@MainActor
public protocol SpacetimeTableCacheProtocol: AnyObject, Sendable {
    var tableName: String { get }
    func handleInsert(rowBytes: Data) throws
    func handleDelete(rowBytes: Data) throws
    func clear()
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
    
    public init(tableName: String) {
        self.tableName = tableName
    }
    
    public func handleInsert(rowBytes: Data) throws {
        do {
            let row = try decoder.decode(T.self, from: rowBytes)
            rowValueByBytes[rowBytes] = row
            rowCountsByBytes[rowBytes, default: 0] += 1
            updatePublishedRows()
        } catch {
            print("[TableCache] Failed to decode row for table '\(tableName)': \(error)")
            print("[TableCache] Raw bytes (\(rowBytes.count)): \(rowBytes.map { String(format: "%02x", $0) }.joined())")
            throw error
        }
    }
    
    public func handleDelete(rowBytes: Data) throws {
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
    }
    
    public func clear() {
        rowCountsByBytes.removeAll()
        rowValueByBytes.removeAll()
        updatePublishedRows()
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
