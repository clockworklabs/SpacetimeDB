import Foundation
import Synchronization

public enum CompressionMode: String, Sendable {
    case none = "None"
    case gzip = "Gzip"
    case brotli = "Brotli"

    var queryValue: String {
        rawValue
    }
}

public struct ReconnectPolicy: Sendable, Equatable {
    public var maxRetries: Int?
    public var initialDelaySeconds: TimeInterval
    public var maxDelaySeconds: TimeInterval
    public var multiplier: Double
    public var jitterRatio: Double

    public init(
        maxRetries: Int? = nil,
        initialDelaySeconds: TimeInterval = 1.0,
        maxDelaySeconds: TimeInterval = 30.0,
        multiplier: Double = 2.0,
        jitterRatio: Double = 0.2
    ) {
        self.maxRetries = maxRetries
        self.initialDelaySeconds = initialDelaySeconds
        self.maxDelaySeconds = maxDelaySeconds
        self.multiplier = multiplier
        self.jitterRatio = jitterRatio
    }

    func delaySeconds(forAttempt attempt: Int, randomUnit: Double = Double.random(in: 0...1)) -> TimeInterval? {
        guard attempt > 0 else { return nil }
        if let maxRetries, attempt > maxRetries {
            return nil
        }

        let boundedInitial = max(0, initialDelaySeconds)
        let boundedMax = max(boundedInitial, maxDelaySeconds)
        let boundedMultiplier = max(1.0, multiplier)
        let exponential = boundedInitial * pow(boundedMultiplier, Double(attempt - 1))
        let baseDelay = min(boundedMax, exponential)

        let boundedJitter = min(max(jitterRatio, 0), 1)
        guard boundedJitter > 0 else {
            return baseDelay
        }

        let boundedRandom = min(max(randomUnit, 0), 1)
        let jitterRange = baseDelay * boundedJitter
        let jitterOffset = ((boundedRandom * 2) - 1) * jitterRange
        return max(0, baseDelay + jitterOffset)
    }

    func delay(forAttempt attempt: Int) -> Duration? {
        guard let seconds = delaySeconds(forAttempt: attempt) else { return nil }
        return .milliseconds(Int64((seconds * 1000).rounded()))
    }
}

public enum SubscriptionState: Sendable {
    case pending
    case active
    case ended
}

public enum ConnectionState: Sendable, Equatable {
    case disconnected
    case connecting
    case connected
    case reconnecting
}

public final class SubscriptionHandle: @unchecked Sendable {
    private struct UnsafeSendable<Value>: @unchecked Sendable {
        var value: Value
    }

    private struct State {
        var state: SubscriptionState = .pending
        var querySetId: QuerySetId?
        var requestId: RequestId?
        var onApplied: UnsafeSendable<(() -> Void)?> = UnsafeSendable(value: nil)
        var onError: UnsafeSendable<((String) -> Void)?> = UnsafeSendable(value: nil)
    }

    public let queries: [String]
    private let stateLock: Mutex<State> = Mutex(State())

    public var state: SubscriptionState {
        stateLock.withLock { state in
            state.state
        }
    }

    var querySetId: QuerySetId? {
        stateLock.withLock { state in
            state.querySetId
        }
    }
    
    var requestId: RequestId? {
        stateLock.withLock { state in
            state.requestId
        }
    }
    
    weak var client: SpacetimeClient?

    init(
        queries: [String],
        client: SpacetimeClient,
        onApplied: (() -> Void)?,
        onError: ((String) -> Void)?
    ) {
        self.queries = queries
        self.client = client
        stateLock.withLock { state in
            state.onApplied.value = onApplied
            state.onError.value = onError
        }
    }

    public func unsubscribe(sendDroppedRows: Bool = false) {
        client?.unsubscribe(self, sendDroppedRows: sendDroppedRows)
    }

    func markPending(requestId: RequestId, querySetId: QuerySetId) {
        stateLock.withLock { state in
            state.requestId = requestId
            state.querySetId = querySetId
            state.state = .pending
        }
    }

    func markApplied(querySetId: QuerySetId) {
        let applied = stateLock.withLock { state in
            state.requestId = nil
            state.querySetId = querySetId
            state.state = .active
            return state.onApplied.value
        }
        
        if let applied {
            Task { @MainActor in
                applied()
            }
        }
    }

    func markError(_ message: String) {
        let error = stateLock.withLock { state in
            state.state = .ended
            let callback = state.onError.value
            state.onApplied.value = nil
            state.onError.value = nil
            return callback
        }
        
        if let error {
            Task { @MainActor in
                error(message)
            }
        }
    }

    func markEnded() {
        stateLock.withLock { state in
            state.state = .ended
            state.requestId = nil
            state.querySetId = nil
            state.onApplied.value = nil
            state.onError.value = nil
        }
    }
}

public enum SpacetimeClientQueryError: Error, Equatable {
    case serverError(String)
    case disconnected
    case timeout
}
