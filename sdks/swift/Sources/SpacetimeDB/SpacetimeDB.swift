import Foundation

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

@MainActor
public final class SubscriptionHandle: @unchecked Sendable {
    public let queries: [String]
    public private(set) var state: SubscriptionState = .pending

    var querySetId: UInt32?
    var requestId: UInt32?
    weak var client: SpacetimeClient?
    var onApplied: (() -> Void)?
    var onError: ((String) -> Void)?

    init(queries: [String], client: SpacetimeClient, onApplied: (() -> Void)?, onError: ((String) -> Void)?) {
        self.queries = queries
        self.client = client
        self.onApplied = onApplied
        self.onError = onError
    }

    public func unsubscribe(sendDroppedRows: Bool = false) {
        client?.unsubscribe(self, sendDroppedRows: sendDroppedRows)
    }

    func markPending(requestId: UInt32, querySetId: UInt32) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.state = .pending
    }

    func markApplied(querySetId: UInt32) {
        self.requestId = nil
        self.querySetId = querySetId
        self.state = .active
        onApplied?()
    }

    func markError(_ message: String) {
        self.state = .ended
        onError?(message)
        onApplied = nil
        onError = nil
    }

    func markEnded() {
        self.state = .ended
        self.requestId = nil
        self.querySetId = nil
        onApplied = nil
        onError = nil
    }
}

public enum SpacetimeClientQueryError: Error, Equatable {
    case serverError(String)
    case disconnected
    case timeout
}
