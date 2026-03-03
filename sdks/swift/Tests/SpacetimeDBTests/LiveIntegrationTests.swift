import Foundation
import XCTest
@testable import SpacetimeDB

final class LiveIntegrationTests: XCTestCase {
    private struct LiveConfig {
        let serverURL: URL
        let moduleName: String
        let token: String?
    }

    @MainActor
    private final class LiveDelegate: SpacetimeClientDelegate {
        var didConnect = false
        var didDisconnect = false
        var connectErrors: [Error] = []
        var reducerErrors: [(reducer: String, message: String, isInternal: Bool)] = []

        func onConnect() {
            didConnect = true
        }

        func onDisconnect(error: Error?) {
            didDisconnect = true
        }

        func onConnectError(error: Error) {
            connectErrors.append(error)
        }

        func onConnectionStateChange(state: ConnectionState) {}

        func onIdentityReceived(identity: [UInt8], token: String) {}

        func onTransactionUpdate(message: Data?) {}

        func onReducerError(reducer: String, message: String, isInternal: Bool) {
            reducerErrors.append((reducer, message, isInternal))
        }
    }

    private func requireLiveConfig() throws -> LiveConfig {
        let env = ProcessInfo.processInfo.environment
        guard env["SPACETIMEDB_SWIFT_LIVE_TESTS"] == "1" else {
            throw XCTSkip("Set SPACETIMEDB_SWIFT_LIVE_TESTS=1 to run live integration tests.")
        }

        guard let moduleName = env["SPACETIMEDB_LIVE_TEST_DB_NAME"], !moduleName.isEmpty else {
            throw XCTSkip("Set SPACETIMEDB_LIVE_TEST_DB_NAME to the published live test database name.")
        }

        let urlString = env["SPACETIMEDB_LIVE_TEST_SERVER_URL"] ?? "http://127.0.0.1:3000"
        guard let serverURL = URL(string: urlString) else {
            throw XCTSkip("Invalid SPACETIMEDB_LIVE_TEST_SERVER_URL: \(urlString)")
        }

        return LiveConfig(
            serverURL: serverURL,
            moduleName: moduleName,
            token: env["SPACETIMEDB_LIVE_TEST_TOKEN"]
        )
    }

    @MainActor
    private func connectClient(using config: LiveConfig) async throws -> (SpacetimeClient, LiveDelegate) {
        let delegate = LiveDelegate()
        let client = SpacetimeClient(serverUrl: config.serverURL, moduleName: config.moduleName)
        client.delegate = delegate
        client.connect(token: config.token)

        let connected = await waitUntil(timeoutSeconds: 15.0) {
            delegate.didConnect
        }
        XCTAssertTrue(connected, "Live client failed to connect within timeout.")
        XCTAssertTrue(delegate.connectErrors.isEmpty, "Unexpected connect errors: \(delegate.connectErrors)")
        XCTAssertEqual(client.connectionState, .connected)
        return (client, delegate)
    }

    @MainActor
    private func waitUntil(timeoutSeconds: TimeInterval, condition: @escaping @MainActor () -> Bool) async -> Bool {
        let start = Date()
        while Date().timeIntervalSince(start) < timeoutSeconds {
            if condition() {
                return true
            }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return condition()
    }

    @MainActor
    func testLiveSubscribeApplyAndUnsubscribe() async throws {
        let config = try requireLiveConfig()
        let (client, _) = try await connectClient(using: config)
        defer { client.disconnect() }

        var applied = false
        var subscriptionError: String?

        let handle = client.subscribe(
            queries: ["SELECT * FROM person"],
            onApplied: { applied = true },
            onError: { message in subscriptionError = message }
        )

        let appliedInTime = await waitUntil(timeoutSeconds: 15.0) { applied }
        XCTAssertTrue(appliedInTime)
        XCTAssertNil(subscriptionError)

        handle.unsubscribe()
        let unsubscribedInTime = await waitUntil(timeoutSeconds: 15.0) { handle.state == .ended }
        XCTAssertTrue(unsubscribedInTime)
    }

    @MainActor
    func testLiveReducerSuccessAndError() async throws {
        let config = try requireLiveConfig()
        let (client, delegate) = try await connectClient(using: config)
        defer { client.disconnect() }

        client.send("say_hello", Data())
        try? await Task.sleep(for: .milliseconds(500))
        XCTAssertTrue(delegate.reducerErrors.isEmpty, "Unexpected reducer errors after say_hello: \(delegate.reducerErrors)")

        client.send("definitely_missing_reducer_live_test", Data())
        let reducerErrorArrived = await waitUntil(timeoutSeconds: 15.0) { !delegate.reducerErrors.isEmpty }
        XCTAssertTrue(reducerErrorArrived)
    }

    @MainActor
    func testLiveProcedureSuccessAndError() async throws {
        let config = try requireLiveConfig()
        let (client, _) = try await connectClient(using: config)
        defer { client.disconnect() }

        var successResult: Result<Data, Error>?
        client.sendProcedure("sleep_one_second", Data()) { result in
            successResult = result
        }
        let successCallbackArrived = await waitUntil(timeoutSeconds: 20.0) { successResult != nil }
        XCTAssertTrue(successCallbackArrived)
        if case .failure(let error)? = successResult {
            XCTFail("Expected sleep_one_second procedure success, got error: \(error)")
        }

        var errorResult: Result<Data, Error>?
        client.sendProcedure("definitely_missing_procedure_live_test", Data()) { result in
            errorResult = result
        }
        let errorCallbackArrived = await waitUntil(timeoutSeconds: 15.0) { errorResult != nil }
        XCTAssertTrue(errorCallbackArrived)
        if case .success? = errorResult {
            XCTFail("Expected missing procedure to fail.")
        }
    }

    @MainActor
    func testLiveOneOffQuerySuccessAndError() async throws {
        let config = try requireLiveConfig()
        let (client, _) = try await connectClient(using: config)
        defer { client.disconnect() }

        let successRows = try await client.oneOffQuery("SELECT * FROM person")
        XCTAssertGreaterThanOrEqual(successRows.tables.count, 0)

        do {
            _ = try await client.oneOffQuery("SELECT * FROM definitely_missing_table_live_test")
            XCTFail("Expected missing table query to fail.")
        } catch {
            if case SpacetimeClientQueryError.serverError = error {
                // Expected.
            } else {
                XCTFail("Expected serverError for invalid live query, got: \(error)")
            }
        }
    }
}
