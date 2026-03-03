import XCTest
@testable import SpacetimeDB

final class KeychainTests: XCTestCase {

    private let store = KeychainTokenStore(service: "com.spacetimedb.test.keychain")
    private let testModule = "test-module-\(UUID().uuidString)"

    override func tearDown() {
        store.delete(forModule: testModule)
        super.tearDown()
    }

    func testSaveAndLoad() {
        let token = "test-token-\(UUID().uuidString)"
        let saved = store.save(token: token, forModule: testModule)
        XCTAssertTrue(saved)

        let loaded = store.load(forModule: testModule)
        XCTAssertEqual(loaded, token)
    }

    func testLoadReturnsNilForMissingKey() {
        let loaded = store.load(forModule: "nonexistent-module-\(UUID().uuidString)")
        XCTAssertNil(loaded)
    }

    func testOverwriteReturnsLatestValue() {
        let first = "first-token"
        let second = "second-token"
        store.save(token: first, forModule: testModule)
        store.save(token: second, forModule: testModule)

        let loaded = store.load(forModule: testModule)
        XCTAssertEqual(loaded, second)
    }

    func testDeleteRemovesToken() {
        store.save(token: "ephemeral", forModule: testModule)
        let deleted = store.delete(forModule: testModule)
        XCTAssertTrue(deleted)

        let loaded = store.load(forModule: testModule)
        XCTAssertNil(loaded)
    }

    func testDeleteNonexistentKeySucceeds() {
        let deleted = store.delete(forModule: "never-existed-\(UUID().uuidString)")
        XCTAssertTrue(deleted)
    }
}
