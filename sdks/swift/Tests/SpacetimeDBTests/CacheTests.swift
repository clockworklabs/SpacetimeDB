import XCTest
@testable import SpacetimeDB

final class CacheTests: XCTestCase {
    
    struct Person: Codable, Identifiable, Equatable, Sendable {
        var id: UInt32
        var name: String
    }
    
    @MainActor
    func testClientCacheRouting() throws {
        let clientCache = ClientCache()
        let personCache = TableCache<Person>(tableName: "Person")
        clientCache.registerTable(name: "Person", cache: personCache)
        
        let encoder = BSATNEncoder()
        let personBytes = try encoder.encode(Person(id: 42, name: "Alice"))
        
        // Build a v2 TransactionUpdate payload with one QuerySetUpdate / table row insert.
        var rawPayload = Data()
        rawPayload.append(1 as UInt32) // query_sets count
        rawPayload.append(1 as UInt32) // query_set_id
        rawPayload.append(1 as UInt32) // tables count

        rawPayload.append(try encoder.encode("Person")) // table_name
        rawPayload.append(1 as UInt32) // rows count
        rawPayload.append(0 as UInt8) // TableUpdateRows::PersistentTable

        rawPayload.append(0 as UInt8) // inserts.size_hint FixedSize
        rawPayload.append(UInt16(personBytes.count)) // insert row size
        rawPayload.append(UInt32(personBytes.count)) // inserts rowsData length
        rawPayload.append(personBytes)

        rawPayload.append(0 as UInt8) // deletes.size_hint FixedSize
        rawPayload.append(0 as UInt16) // delete row size
        rawPayload.append(0 as UInt32) // deletes rowsData length
        
        let decoder = BSATNDecoder()
        let transactionUpdate = try decoder.decode(TransactionUpdate.self, from: rawPayload)

        clientCache.applyTransactionUpdate(transactionUpdate)

        XCTAssertEqual(personCache.rows.count, 1)
        XCTAssertEqual(personCache.rows[0].id, 42)
        XCTAssertEqual(personCache.rows[0].name, "Alice")
    }

    @MainActor
    func testRegisterTableIsIdempotentForSameType() throws {
        let clientCache = ClientCache()
        clientCache.registerTable(tableName: "Person", rowType: Person.self)

        let firstCache: TableCache<Person> = clientCache.getTableCache(tableName: "Person")
        let rowBytes = try BSATNEncoder().encode(Person(id: 7, name: "Bob"))
        try firstCache.handleInsert(rowBytes: rowBytes)

        // Re-registering the same table/type must keep the existing cache instance.
        clientCache.registerTable(tableName: "Person", rowType: Person.self)

        let secondCache: TableCache<Person> = clientCache.getTableCache(tableName: "Person")
        XCTAssertTrue(firstCache === secondCache)
        XCTAssertEqual(secondCache.rows.count, 1)
        XCTAssertEqual(secondCache.rows[0].id, 7)
        XCTAssertEqual(secondCache.rows[0].name, "Bob")
    }
}

extension Data {
    mutating func append<T: FixedWidthInteger>(_ value: T) {
        var copy = value.littleEndian
        self.append(Swift.withUnsafeBytes(of: &copy) { Data($0) })
    }
}
