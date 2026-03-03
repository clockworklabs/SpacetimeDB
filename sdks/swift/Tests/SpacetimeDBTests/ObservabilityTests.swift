import Foundation
import XCTest
@testable import SpacetimeDB

final class ObservabilityTests: XCTestCase {
    private final class CapturingLogger: @unchecked Sendable, SpacetimeLogger {
        struct Entry: Sendable {
            let level: SpacetimeLogLevel
            let category: String
            let message: String
        }

        private let lock = NSLock()
        private var entries: [Entry] = []

        func log(level: SpacetimeLogLevel, category: String, message: String) {
            lock.lock()
            entries.append(Entry(level: level, category: category, message: message))
            lock.unlock()
        }

        func snapshot() -> [Entry] {
            lock.lock()
            defer { lock.unlock() }
            return entries
        }
    }

    private final class CapturingMetrics: @unchecked Sendable, SpacetimeMetrics {
        struct Gauge: Sendable {
            let name: String
            let value: Double
            let tags: [String: String]
        }

        struct Timing: Sendable {
            let name: String
            let milliseconds: Double
            let tags: [String: String]
        }

        private let lock = NSLock()
        private var counters: [String: Int64] = [:]
        private var gauges: [Gauge] = []
        private var timings: [Timing] = []

        func incrementCounter(_ name: String, by value: Int64, tags: [String: String]) {
            lock.lock()
            counters[name, default: 0] += value
            lock.unlock()
        }

        func recordGauge(_ name: String, value: Double, tags: [String: String]) {
            lock.lock()
            gauges.append(Gauge(name: name, value: value, tags: tags))
            lock.unlock()
        }

        func recordTiming(_ name: String, milliseconds: Double, tags: [String: String]) {
            lock.lock()
            timings.append(Timing(name: name, milliseconds: milliseconds, tags: tags))
            lock.unlock()
        }

        func counterValue(_ name: String) -> Int64 {
            lock.lock()
            defer { lock.unlock() }
            return counters[name, default: 0]
        }

        func hasGauge(named name: String) -> Bool {
            lock.lock()
            defer { lock.unlock() }
            return gauges.contains(where: { $0.name == name })
        }

        func hasTiming(named name: String, callback: String) -> Bool {
            lock.lock()
            defer { lock.unlock() }
            return timings.contains(where: { $0.name == name && $0.tags["callback"] == callback })
        }
    }

    private struct Person: Codable, Sendable {
        var id: UInt32
        var name: String
    }

    private enum TestError: Error {
        case simulated
    }

    @MainActor
    private final class DelegateProbe: SpacetimeClientDelegate {
        func onConnect() {}
        func onDisconnect(error: Error?) {}
        func onConnectError(error: Error) {}
        func onConnectionStateChange(state: ConnectionState) {}
        func onIdentityReceived(identity: [UInt8], token: String) {}
        func onTransactionUpdate(message: Data?) {}
        func onReducerError(reducer: String, message: String, isInternal: Bool) {}
    }

    @MainActor
    func testCustomLoggerReceivesSDKLogEntries() {
        let oldLogger = SpacetimeObservability.logger
        defer { SpacetimeObservability.logger = oldLogger }

        let logger = CapturingLogger()
        SpacetimeObservability.logger = logger

        let cache = TableCache<Person>(tableName: "Person")
        XCTAssertThrowsError(try cache.handleInsert(rowBytes: Data([0xFF])))

        let entries = logger.snapshot()
        XCTAssertTrue(entries.contains(where: { $0.category == "Cache" && $0.level == .error }))
    }

    @MainActor
    func testCustomMetricsCaptureClientCountersAndGauges() {
        let oldMetrics = SpacetimeObservability.metrics
        defer { SpacetimeObservability.metrics = oldMetrics }

        let metrics = CapturingMetrics()
        SpacetimeObservability.metrics = metrics

        let policy = ReconnectPolicy(
            maxRetries: 0,
            initialDelaySeconds: 0.1,
            maxDelaySeconds: 0.1,
            multiplier: 1.0,
            jitterRatio: 0
        )
        let client = SpacetimeClient(
            serverUrl: URL(string: "http://localhost:3000")!,
            moduleName: "test-module",
            reconnectPolicy: policy
        )
        let delegate = DelegateProbe()
        client.delegate = delegate

        client.sendProcedure("bench_proc", Data()) { _ in }
        client._test_deliverServerMessage(
            .procedureResult(
                ProcedureResult(
                    status: .returned(Data([0x01])),
                    timestamp: 0,
                    totalHostExecutionDuration: 0,
                    requestId: RequestId(rawValue: 1)
                )
            )
        )
        client._test_setConnectionState(.connecting)
        client._test_simulateConnectionFailure(TestError.simulated)

        XCTAssertGreaterThanOrEqual(metrics.counterValue("spacetimedb.connection.failures"), 1)
        XCTAssertTrue(metrics.hasGauge(named: "spacetimedb.connection.state"))
        XCTAssertTrue(metrics.hasTiming(named: "spacetimedb.callback.latency", callback: "procedure.completion"))
        XCTAssertTrue(metrics.hasTiming(named: "spacetimedb.callback.latency", callback: "delegate.on_connection_state_change"))
        XCTAssertTrue(metrics.hasTiming(named: "spacetimedb.callback.latency", callback: "delegate.on_connect_error"))
        XCTAssertTrue(metrics.hasTiming(named: "spacetimedb.callback.latency", callback: "delegate.on_disconnect"))
    }
}
