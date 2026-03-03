import Compression
import XCTest
import zlib
@testable import SpacetimeDB // This allows us to test internal types

final class NetworkTests: XCTestCase {
    private enum TestError: Error {
        case simulated
    }
    
    // We mock the delegate to ensure the client is calling back properly
    class MockDelegate: SpacetimeClientDelegate {
        var didConnect = false
        var didDisconnect = false
        var connectErrors: [Error] = []
        var stateChanges: [ConnectionState] = []
        var receivedTransaction = false
        var reducerErrorReducer = ""
        var reducerErrorMessage = ""
        var reducerErrorIsInternal = false
        var expectation: XCTestExpectation?
        
        func onConnect() {
            didConnect = true
            expectation?.fulfill()
        }
        
        func onDisconnect(error: Error?) {
            didDisconnect = true
            expectation?.fulfill()
        }

        func onConnectError(error: Error) {
            connectErrors.append(error)
        }

        func onConnectionStateChange(state: ConnectionState) {
            stateChanges.append(state)
        }
        
        func onIdentityReceived(identity: [UInt8], token: String) {}
        
        func onTransactionUpdate(message: Data?) {
            receivedTransaction = true
            expectation?.fulfill()
        }

        func onReducerError(reducer: String, message: String, isInternal: Bool) {
            reducerErrorReducer = reducer
            reducerErrorMessage = message
            reducerErrorIsInternal = isInternal
            expectation?.fulfill()
        }
    }
    
    @MainActor
    func testClientInitialization() {
        let url = URL(string: "http://localhost:3000")!
        let client = SpacetimeClient(serverUrl: url, moduleName: "test-module")
        XCTAssertEqual(client.serverUrl, url)
        XCTAssertEqual(client.moduleName, "test-module")
        XCTAssertEqual(client.connectionState, .disconnected)
    }

    @MainActor
    func testInitialConnectionTransitionsToConnectedAndNotifiesState() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let delegate = MockDelegate()
        client.delegate = delegate
        let initial = InitialConnection(
            identity: Identity(rawBytes: Data(repeating: 0xAB, count: 32)),
            connectionId: ClientConnectionId(rawBytes: Data(repeating: 0xCD, count: 16)),
            token: "token"
        )
        client._test_deliverServerMessage(.initialConnection(initial))

        XCTAssertEqual(client.connectionState, .connected)
        XCTAssertTrue(delegate.didConnect)
        XCTAssertEqual(delegate.stateChanges, [.connected])
    }

    @MainActor
    func testConnectingFailureTriggersConnectErrorCallback() {
        let policy = ReconnectPolicy(maxRetries: 0, initialDelaySeconds: 0.1, maxDelaySeconds: 0.1, multiplier: 1, jitterRatio: 0)
        let client = SpacetimeClient(
            serverUrl: URL(string: "http://localhost:3000")!,
            moduleName: "test-module",
            reconnectPolicy: policy
        )
        let delegate = MockDelegate()
        client.delegate = delegate

        client._test_setConnectionState(.connecting)
        client._test_simulateConnectionFailure(TestError.simulated)

        XCTAssertEqual(client.connectionState, .disconnected)
        XCTAssertEqual(delegate.connectErrors.count, 1)
        XCTAssertTrue(delegate.didDisconnect)
    }

    @MainActor
    func testConnectedFailureDoesNotTriggerConnectErrorCallback() {
        let policy = ReconnectPolicy(maxRetries: 0, initialDelaySeconds: 0.1, maxDelaySeconds: 0.1, multiplier: 1, jitterRatio: 0)
        let client = SpacetimeClient(
            serverUrl: URL(string: "http://localhost:3000")!,
            moduleName: "test-module",
            reconnectPolicy: policy
        )
        let delegate = MockDelegate()
        client.delegate = delegate
        let initial = InitialConnection(
            identity: Identity(rawBytes: Data(repeating: 0xAB, count: 32)),
            connectionId: ClientConnectionId(rawBytes: Data(repeating: 0xCD, count: 16)),
            token: "token"
        )
        client._test_deliverServerMessage(.initialConnection(initial))
        client._test_simulateConnectionFailure(TestError.simulated)

        XCTAssertEqual(client.connectionState, .disconnected)
        XCTAssertEqual(delegate.connectErrors.count, 0)
        XCTAssertTrue(delegate.didDisconnect)
    }

    func testCompressionModeQueryValues() {
        XCTAssertEqual(CompressionMode.none.queryValue, "None")
        XCTAssertEqual(CompressionMode.gzip.queryValue, "Gzip")
        XCTAssertEqual(CompressionMode.brotli.queryValue, "Brotli")
    }

    func testReconnectPolicyBackoffWithoutJitter() {
        let policy = ReconnectPolicy(
            maxRetries: 4,
            initialDelaySeconds: 0.5,
            maxDelaySeconds: 10,
            multiplier: 2.0,
            jitterRatio: 0
        )

        XCTAssertEqual(policy.delaySeconds(forAttempt: 1, randomUnit: 0.5) ?? -1, 0.5, accuracy: 0.0001)
        XCTAssertEqual(policy.delaySeconds(forAttempt: 2, randomUnit: 0.5) ?? -1, 1.0, accuracy: 0.0001)
        XCTAssertEqual(policy.delaySeconds(forAttempt: 3, randomUnit: 0.5) ?? -1, 2.0, accuracy: 0.0001)
        XCTAssertEqual(policy.delaySeconds(forAttempt: 4, randomUnit: 0.5) ?? -1, 4.0, accuracy: 0.0001)
        XCTAssertNil(policy.delaySeconds(forAttempt: 5, randomUnit: 0.5))
    }

    func testReconnectPolicyRespectsJitterBounds() {
        let policy = ReconnectPolicy(
            maxRetries: 1,
            initialDelaySeconds: 10.0,
            maxDelaySeconds: 10.0,
            multiplier: 2.0,
            jitterRatio: 0.2
        )

        let minDelay = policy.delaySeconds(forAttempt: 1, randomUnit: 0.0) ?? -1
        let maxDelay = policy.delaySeconds(forAttempt: 1, randomUnit: 1.0) ?? -1

        XCTAssertEqual(minDelay, 8.0, accuracy: 0.0001)
        XCTAssertEqual(maxDelay, 12.0, accuracy: 0.0001)
    }
    
    func testSubscriptionMessageEncoding() throws {
        // Just verify our protocol messages compile and encode with BSATN successfully
        let subscribe = ClientMessage.subscribe(Subscribe(queryStrings: ["SELECT * FROM person"], requestId: RequestId(rawValue: 1)))
        let encoder = BSATNEncoder()
        
        let data = try encoder.encode(subscribe)
        // Message is an enum. Tag 0 for subscribe.
        XCTAssertGreaterThan(data.count, 0)
    }

    func testProcedureMessageEncoding() throws {
        let call = ClientMessage.callProcedure(
            CallProcedure(requestId: RequestId(rawValue: 42), flags: 0, procedure: "say_hello", args: Data([0x01, 0x02]))
        )
        let data = try BSATNEncoder().encode(call)

        XCTAssertGreaterThan(data.count, 0)
        XCTAssertEqual(data.first, 4) // v2 ClientMessage::CallProcedure tag
    }

    func testUnsubscribeMessageEncoding() throws {
        let msg = ClientMessage.unsubscribe(Unsubscribe(requestId: RequestId(rawValue: 1), querySetId: QuerySetId(rawValue: 7), flags: 1))
        let data = try BSATNEncoder().encode(msg)
        XCTAssertGreaterThan(data.count, 0)
        XCTAssertEqual(data.first, 1) // v2 ClientMessage::Unsubscribe tag
    }

    func testUnsubscribeAppliedOptionTagDecoding() throws {
        var somePayload = Data()
        appendLE(1 as UInt32, to: &somePayload)  // request_id
        appendLE(77 as UInt32, to: &somePayload) // query_set_id
        appendLE(0 as UInt8, to: &somePayload)   // rows: Some
        appendLE(0 as UInt32, to: &somePayload)  // QueryRows.tables count

        var someServerMessage = Data([2]) // ServerMessage::UnsubscribeApplied
        someServerMessage.append(somePayload)
        let decodedSomeMessage = try BSATNDecoder().decode(ServerMessage.self, from: someServerMessage)
        guard case .unsubscribeApplied(let decodedSome) = decodedSomeMessage else {
            return XCTFail("Expected unsubscribeApplied message")
        }
        XCTAssertEqual(decodedSome.requestId, RequestId(rawValue: 1))
        XCTAssertEqual(decodedSome.querySetId, QuerySetId(rawValue: 77))
        XCTAssertEqual(decodedSome.rows?.tables.count, 0)

        var nonePayload = Data()
        appendLE(1 as UInt32, to: &nonePayload)  // request_id
        appendLE(77 as UInt32, to: &nonePayload) // query_set_id
        appendLE(1 as UInt8, to: &nonePayload)   // rows: None

        var noneServerMessage = Data([2]) // ServerMessage::UnsubscribeApplied
        noneServerMessage.append(nonePayload)
        let decodedNoneMessage = try BSATNDecoder().decode(ServerMessage.self, from: noneServerMessage)
        guard case .unsubscribeApplied(let decodedNone) = decodedNoneMessage else {
            return XCTFail("Expected unsubscribeApplied message")
        }
        XCTAssertNil(decodedNone.rows)

        var invalidPayload = Data()
        appendLE(1 as UInt32, to: &invalidPayload)
        appendLE(77 as UInt32, to: &invalidPayload)
        appendLE(2 as UInt8, to: &invalidPayload) // invalid option tag
        var invalidServerMessage = Data([2]) // ServerMessage::UnsubscribeApplied
        invalidServerMessage.append(invalidPayload)
        XCTAssertThrowsError(try BSATNDecoder().decode(ServerMessage.self, from: invalidServerMessage))
    }

    func testSubscriptionErrorOptionTagDecoding() throws {
        let messageBytes = try BSATNEncoder().encode("bad query")

        var somePayload = Data()
        appendLE(0 as UInt8, to: &somePayload)   // request_id: Some
        appendLE(9 as UInt32, to: &somePayload)  // request_id value
        appendLE(42 as UInt32, to: &somePayload) // query_set_id
        somePayload.append(messageBytes)

        var someServerMessage = Data([3]) // ServerMessage::SubscriptionError
        someServerMessage.append(somePayload)
        let decodedSomeMessage = try BSATNDecoder().decode(ServerMessage.self, from: someServerMessage)
        guard case .subscriptionError(let decodedSome) = decodedSomeMessage else {
            return XCTFail("Expected subscriptionError message")
        }
        XCTAssertEqual(decodedSome.requestId, RequestId(rawValue: 9))
        XCTAssertEqual(decodedSome.querySetId, QuerySetId(rawValue: 42))
        XCTAssertEqual(decodedSome.error, "bad query")

        var nonePayload = Data()
        appendLE(1 as UInt8, to: &nonePayload)   // request_id: None
        appendLE(42 as UInt32, to: &nonePayload) // query_set_id
        nonePayload.append(messageBytes)

        var noneServerMessage = Data([3]) // ServerMessage::SubscriptionError
        noneServerMessage.append(nonePayload)
        let decodedNoneMessage = try BSATNDecoder().decode(ServerMessage.self, from: noneServerMessage)
        guard case .subscriptionError(let decodedNone) = decodedNoneMessage else {
            return XCTFail("Expected subscriptionError message")
        }
        XCTAssertNil(decodedNone.requestId)
        XCTAssertEqual(decodedNone.querySetId, QuerySetId(rawValue: 42))
        XCTAssertEqual(decodedNone.error, "bad query")

        var invalidPayload = Data()
        appendLE(2 as UInt8, to: &invalidPayload) // invalid option tag
        appendLE(42 as UInt32, to: &invalidPayload)
        invalidPayload.append(messageBytes)
        var invalidServerMessage = Data([3]) // ServerMessage::SubscriptionError
        invalidServerMessage.append(invalidPayload)
        XCTAssertThrowsError(try BSATNDecoder().decode(ServerMessage.self, from: invalidServerMessage))
    }

    func testOneOffQueryMessageEncoding() throws {
        let msg = ClientMessage.oneOffQuery(OneOffQuery(requestId: RequestId(rawValue: 1), queryString: "SELECT * FROM player"))
        let data = try BSATNEncoder().encode(msg)
        XCTAssertGreaterThan(data.count, 0)
        XCTAssertEqual(data.first, 2) // v2 ClientMessage::OneOffQuery tag
    }

    @MainActor
    func testOneOffQueryCallbackSuccessAndError() throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let success = XCTestExpectation(description: "One-off query success callback")
        let failure = XCTestExpectation(description: "One-off query error callback")

        client.oneOffQuery("SELECT * FROM player") { result in
            switch result {
            case .success(let rows):
                XCTAssertEqual(rows.tables.count, 0)
                success.fulfill()
            case .failure(let error):
                XCTFail("Unexpected one-off query failure: \(error)")
            }
        }

        client.handleOneOffQueryResult(
            OneOffQueryResult(requestId: RequestId(rawValue: 1), result: .ok(QueryRows(tables: [])))
        )

        client.oneOffQuery("SELECT * FROM nope") { result in
            switch result {
            case .success:
                XCTFail("Expected one-off query error")
            case .failure(let error):
                XCTAssertEqual(error as? SpacetimeClientQueryError, .serverError("bad query"))
                failure.fulfill()
            }
        }

        client.handleOneOffQueryResult(
            OneOffQueryResult(requestId: RequestId(rawValue: 2), result: .err("bad query"))
        )

        wait(for: [success, failure], timeout: 1.0)
    }

    @MainActor
    func testManagedSubscriptionLifecycle() throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let onApplied = XCTestExpectation(description: "Managed subscription becomes active")

        let handle = client.subscribe(queries: ["SELECT * FROM player"], onApplied: {
            onApplied.fulfill()
        })

        XCTAssertEqual(handle.state, .pending)

        let applied = SubscribeApplied(requestId: RequestId(rawValue: 1), querySetId: QuerySetId(rawValue: 77), rows: QueryRows(tables: []))
        client.handleSubscribeApplied(applied)

        wait(for: [onApplied], timeout: 1.0)
        XCTAssertEqual(handle.state, .active)

        client.unsubscribe(handle)
        let unapplied = UnsubscribeApplied(requestId: RequestId(rawValue: 2), querySetId: QuerySetId(rawValue: 77), rows: nil)
        client.handleUnsubscribeApplied(unapplied)

        XCTAssertEqual(handle.state, .ended)
    }

    @MainActor
    func testSubscriptionErrorEndsHandleAndCallsErrorCallback() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let onError = XCTestExpectation(description: "Managed subscription error callback called")
        var received = ""

        let handle = client.subscribe(
            queries: ["SELECT * FROM invalid_table"],
            onError: { message in
                received = message
                onError.fulfill()
            }
        )

        client.handleSubscriptionError(
            SubscriptionError(requestId: RequestId(rawValue: 1), querySetId: QuerySetId(rawValue: 1), error: "bad query syntax")
        )

        wait(for: [onError], timeout: 1.0)
        XCTAssertEqual(handle.state, .ended)
        XCTAssertEqual(received, "bad query syntax")
    }

    @MainActor
    func testUnsubscribeIsIdempotentAfterEnd() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let handle = client.subscribe(queries: ["SELECT * FROM player"])

        client.handleSubscribeApplied(SubscribeApplied(requestId: RequestId(rawValue: 1), querySetId: QuerySetId(rawValue: 9), rows: QueryRows(tables: [])))
        XCTAssertEqual(handle.state, .active)

        client.unsubscribe(handle)
        client.handleUnsubscribeApplied(UnsubscribeApplied(requestId: RequestId(rawValue: 2), querySetId: QuerySetId(rawValue: 9), rows: nil))
        XCTAssertEqual(handle.state, .ended)

        // A second call should no-op and stay stable.
        client.unsubscribe(handle)
        XCTAssertEqual(handle.state, .ended)
    }

    @MainActor
    func testDisconnectFailsPendingOneOffQueryCallbacks() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let failure = XCTestExpectation(description: "Pending one-off query fails on disconnect")

        client.oneOffQuery("SELECT * FROM player") { result in
            switch result {
            case .success:
                XCTFail("Expected disconnect failure for pending one-off query")
            case .failure(let error):
                XCTAssertEqual(error as? SpacetimeClientQueryError, .disconnected)
                failure.fulfill()
            }
        }

        client.disconnect()
        wait(for: [failure], timeout: 1.0)
    }

    @MainActor
    func testOneOffQueryAsyncTimeout() async {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")

        do {
            _ = try await client.oneOffQuery("SELECT * FROM player", timeout: .milliseconds(10))
            XCTFail("Expected async one-off query timeout.")
        } catch {
            XCTAssertEqual(error as? SpacetimeClientQueryError, .timeout)
        }
    }

    @MainActor
    func testOneOffQueryAsyncCancellationClearsPendingCallback() async {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")

        let task = Task {
            try await client.oneOffQuery("SELECT * FROM player", timeout: .seconds(5))
        }

        await Task.yield()
        task.cancel()

        do {
            _ = try await task.value
            XCTFail("Expected cancellation to throw.")
        } catch is CancellationError {
            // expected
        } catch {
            XCTFail("Expected CancellationError, got: \(error)")
        }

        XCTAssertEqual(client._test_pendingOneOffQueryCallbackCount(), 0)
    }

    @MainActor
    func testProcedureCallbackSuccess() throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let expectation = XCTestExpectation(description: "Procedure callback receives decoded return value")

        client.sendProcedure("say_hello", Data(), responseType: String.self) { result in
            switch result {
            case .success(let value):
                XCTAssertEqual(value, "hello")
                expectation.fulfill()
            case .failure(let error):
                XCTFail("Unexpected failure: \(error)")
            }
        }

        let returned = try BSATNEncoder().encode("hello")
        let procedureResult = ProcedureResult(
            status: .returned(returned),
            timestamp: 0,
            totalHostExecutionDuration: 0,
            requestId: RequestId(rawValue: 1)
        )
        client.handleProcedureResult(procedureResult)

        wait(for: [expectation], timeout: 1.0)
    }

    @MainActor
    func testProcedureAsyncRawReturn() async throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let expectedData = Data([0x01, 0x02, 0x03])

        let task = Task {
            try await client.sendProcedure("raw_echo", Data())
        }

        await Task.yield()
        client.handleProcedureResult(
            ProcedureResult(
                status: .returned(expectedData),
                timestamp: 0,
                totalHostExecutionDuration: 0,
                requestId: RequestId(rawValue: 1)
            )
        )

        let returned = try await task.value
        XCTAssertEqual(returned, expectedData)
    }

    @MainActor
    func testProcedureAsyncTimeout() async {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")

        do {
            _ = try await client.sendProcedure("slow", Data(), timeout: .milliseconds(10))
            XCTFail("Expected async procedure timeout.")
        } catch {
            XCTAssertEqual(error as? SpacetimeClientProcedureError, .timeout)
        }
    }

    @MainActor
    func testProcedureAsyncCancellationClearsPendingCallback() async {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")

        let task = Task {
            try await client.sendProcedure("slow", Data(), timeout: .seconds(5))
        }

        await Task.yield()
        task.cancel()

        do {
            _ = try await task.value
            XCTFail("Expected cancellation to throw.")
        } catch is CancellationError {
            // expected
        } catch {
            XCTFail("Expected CancellationError, got: \(error)")
        }

        XCTAssertEqual(client._test_pendingProcedureCallbackCount(), 0)
    }

    @MainActor
    func testProcedureAsyncDecodedReturn() async throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let encoded = try BSATNEncoder().encode("hello")

        let task = Task {
            try await client.sendProcedure("say_hello", Data(), responseType: String.self)
        }

        await Task.yield()
        client.handleProcedureResult(
            ProcedureResult(
                status: .returned(encoded),
                timestamp: 0,
                totalHostExecutionDuration: 0,
                requestId: RequestId(rawValue: 1)
            )
        )

        let value = try await task.value
        XCTAssertEqual(value, "hello")
    }

    @MainActor
    func testProcedureAsyncDecodedReturnWithTimeoutParameter() async throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let encoded = try BSATNEncoder().encode("hello")

        let task = Task {
            try await client.sendProcedure(
                "say_hello",
                Data(),
                responseType: String.self,
                timeout: .seconds(2)
            )
        }

        await Task.yield()
        client.handleProcedureResult(
            ProcedureResult(
                status: .returned(encoded),
                timestamp: 0,
                totalHostExecutionDuration: 0,
                requestId: RequestId(rawValue: 1)
            )
        )

        let value = try await task.value
        XCTAssertEqual(value, "hello")
    }

    @MainActor
    func testProcedureCallbackInternalError() throws {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let expectation = XCTestExpectation(description: "Procedure callback receives internal error")

        client.sendProcedure("say_hello", Data(), responseType: String.self) { result in
            switch result {
            case .success(let value):
                XCTFail("Unexpected success: \(value)")
            case .failure(let error):
                guard case SpacetimeClientProcedureError.internalError(let message) = error else {
                    XCTFail("Unexpected error type: \(error)")
                    return
                }
                XCTAssertEqual(message, "boom")
                expectation.fulfill()
            }
        }

        let procedureResult = ProcedureResult(
            status: .internalError("boom"),
            timestamp: 0,
            totalHostExecutionDuration: 0,
            requestId: RequestId(rawValue: 1)
        )
        client.handleProcedureResult(procedureResult)

        wait(for: [expectation], timeout: 1.0)
    }

    @MainActor
    func testReducerInternalErrorCallbackIncludesReducerName() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let delegate = MockDelegate()
        let expectation = XCTestExpectation(description: "Reducer internal error callback receives reducer name")
        delegate.expectation = expectation
        client.delegate = delegate

        // Registers request_id=1 -> "add" in pending reducer map.
        client.send("add", Data())

        client.handleReducerResult(
            ReducerResult(requestId: RequestId(rawValue: 1), timestamp: 0, result: .internalError("no such reducer"))
        )

        wait(for: [expectation], timeout: 1.0)
        XCTAssertEqual(delegate.reducerErrorReducer, "add")
        XCTAssertEqual(delegate.reducerErrorMessage, "no such reducer")
        XCTAssertTrue(delegate.reducerErrorIsInternal)
    }

    @MainActor
    func testReducerErrPayloadFallsBackToUTF8AndUnknownReducer() {
        let client = SpacetimeClient(serverUrl: URL(string: "http://localhost:3000")!, moduleName: "test-module")
        let delegate = MockDelegate()
        let expectation = XCTestExpectation(description: "Reducer err payload reports utf8 message")
        delegate.expectation = expectation
        client.delegate = delegate

        client.handleReducerResult(
            ReducerResult(requestId: RequestId(rawValue: 4242), timestamp: 0, result: .err(Data("plain utf8 error".utf8)))
        )

        wait(for: [expectation], timeout: 1.0)
        XCTAssertEqual(delegate.reducerErrorReducer, "<unknown>")
        XCTAssertEqual(delegate.reducerErrorMessage, "plain utf8 error")
        XCTAssertFalse(delegate.reducerErrorIsInternal)
    }

    func testFrameDecoderNoneCompression() throws {
        let payload = Data("hello-spacetimedb".utf8)
        var frame = Data([0])
        frame.append(payload)
        let decoded = try ServerMessageFrameDecoder.decodePayload(from: frame)
        XCTAssertEqual(decoded, payload)
    }

    func testFrameDecoderBrotliCompression() throws {
        let payload = Data(repeating: 0x2A, count: 4096)
        let compressed = try compressWithCompressionStream(payload, algorithm: COMPRESSION_BROTLI)
        var frame = Data([1])
        frame.append(compressed)
        let decoded = try ServerMessageFrameDecoder.decodePayload(from: frame)
        XCTAssertEqual(decoded, payload)
    }

    func testFrameDecoderGzipCompression() throws {
        let payload = Data(repeating: 0x7F, count: 4096)
        let compressed = try compressGzip(payload)
        var frame = Data([2])
        frame.append(compressed)
        let decoded = try ServerMessageFrameDecoder.decodePayload(from: frame)
        XCTAssertEqual(decoded, payload)
    }

    func testFrameDecoderUnsupportedCompressionTag() {
        let frame = Data([99, 0x00, 0x01])

        do {
            _ = try ServerMessageFrameDecoder.decodePayload(from: frame)
            XCTFail("Expected unsupported compression error")
        } catch let error as ServerMessageFrameDecodingError {
            guard case .unsupportedCompression(let tag) = error else {
                XCTFail("Unexpected frame decoder error: \(error)")
                return
            }
            XCTAssertEqual(tag, 99)
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    private func compressWithCompressionStream(_ payload: Data, algorithm: compression_algorithm) throws -> Data {
        enum CompressionError: Error {
            case initializationFailed
            case compressionFailed
        }

        if payload.isEmpty {
            return Data()
        }

        let destinationBufferSize = 64 * 1024
        let bootstrapPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
        defer { bootstrapPtr.deallocate() }
        var stream = compression_stream(
            dst_ptr: bootstrapPtr,
            dst_size: 0,
            src_ptr: UnsafePointer(bootstrapPtr),
            src_size: 0,
            state: nil
        )
        let initStatus = compression_stream_init(&stream, COMPRESSION_STREAM_ENCODE, algorithm)
        guard initStatus != COMPRESSION_STATUS_ERROR else {
            throw CompressionError.initializationFailed
        }
        defer { compression_stream_destroy(&stream) }

        return try payload.withUnsafeBytes { rawBuffer in
            guard let srcBase = rawBuffer.bindMemory(to: UInt8.self).baseAddress else {
                return Data()
            }

            stream.src_ptr = srcBase
            stream.src_size = payload.count

            let destinationBuffer = UnsafeMutablePointer<UInt8>.allocate(capacity: destinationBufferSize)
            defer { destinationBuffer.deallocate() }

            var output = Data()
            while true {
                stream.dst_ptr = destinationBuffer
                stream.dst_size = destinationBufferSize

                let status = compression_stream_process(&stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
                let produced = destinationBufferSize - stream.dst_size
                if produced > 0 {
                    output.append(destinationBuffer, count: produced)
                }

                switch status {
                case COMPRESSION_STATUS_OK:
                    continue
                case COMPRESSION_STATUS_END:
                    return output
                default:
                    throw CompressionError.compressionFailed
                }
            }
        }
    }

    private func compressGzip(_ payload: Data) throws -> Data {
        enum GzipError: Error {
            case invalidInputSize
            case initializationFailed
            case compressionFailed
        }

        if payload.isEmpty {
            return Data()
        }

        guard payload.count <= Int(UInt32.max) else {
            throw GzipError.invalidInputSize
        }

        return try payload.withUnsafeBytes { rawBuffer in
            guard let srcBase = rawBuffer.bindMemory(to: Bytef.self).baseAddress else {
                return Data()
            }

            var stream = z_stream()
            stream.next_in = UnsafeMutablePointer<Bytef>(mutating: srcBase)
            stream.avail_in = uInt(payload.count)

            let initStatus = deflateInit2_(
                &stream,
                Z_BEST_SPEED,
                Z_DEFLATED,
                31, // gzip wrapper
                8,
                Z_DEFAULT_STRATEGY,
                ZLIB_VERSION,
                Int32(MemoryLayout<z_stream>.size)
            )
            guard initStatus == Z_OK else {
                throw GzipError.initializationFailed
            }
            defer { deflateEnd(&stream) }

            let destinationBufferSize = 64 * 1024
            var destinationBuffer = [UInt8](repeating: 0, count: destinationBufferSize)
            var output = Data()

            while true {
                let deflateStatus: Int32 = destinationBuffer.withUnsafeMutableBytes { outRaw in
                    stream.next_out = outRaw.bindMemory(to: Bytef.self).baseAddress
                    stream.avail_out = uInt(destinationBufferSize)
                    return deflate(&stream, Z_FINISH)
                }

                let produced = destinationBufferSize - Int(stream.avail_out)
                if produced > 0 {
                    output.append(contentsOf: destinationBuffer[0..<produced])
                }

                switch deflateStatus {
                case Z_OK:
                    continue
                case Z_STREAM_END:
                    return output
                default:
                    throw GzipError.compressionFailed
                }
            }
        }
    }

    private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
        var littleEndian = value.littleEndian
        data.append(Swift.withUnsafeBytes(of: &littleEndian) { Data($0) })
    }
}
