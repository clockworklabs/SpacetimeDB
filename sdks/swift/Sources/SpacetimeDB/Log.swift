import os

public enum SpacetimeLogLevel: String, Sendable {
    case debug
    case info
    case warning
    case error
}

public protocol SpacetimeLogger: Sendable {
    func log(level: SpacetimeLogLevel, category: String, message: String)
}

public protocol SpacetimeMetrics: Sendable {
    func incrementCounter(_ name: String, by value: Int64, tags: [String: String])
    func recordGauge(_ name: String, value: Double, tags: [String: String])
    func recordTiming(_ name: String, milliseconds: Double, tags: [String: String])
}

public struct NoopSpacetimeLogger: SpacetimeLogger {
    public init() {}

    public func log(level: SpacetimeLogLevel, category: String, message: String) {}
}

public struct NoopSpacetimeMetrics: SpacetimeMetrics {
    public init() {}

    public func incrementCounter(_ name: String, by value: Int64, tags: [String: String]) {}

    public func recordGauge(_ name: String, value: Double, tags: [String: String]) {}

    public func recordTiming(_ name: String, milliseconds: Double, tags: [String: String]) {}
}

public struct OSLogSpacetimeLogger: SpacetimeLogger {
    private let clientLogger: Logger
    private let cacheLogger: Logger
    private let networkLogger: Logger

    public init(subsystem: String = "com.clockworklabs.SpacetimeDB") {
        self.clientLogger = Logger(subsystem: subsystem, category: "Client")
        self.cacheLogger = Logger(subsystem: subsystem, category: "Cache")
        self.networkLogger = Logger(subsystem: subsystem, category: "Network")
    }

    public func log(level: SpacetimeLogLevel, category: String, message: String) {
        let logger = loggerForCategory(category)
        switch level {
        case .debug:
            logger.debug("\(message, privacy: .public)")
        case .info:
            logger.info("\(message, privacy: .public)")
        case .warning:
            logger.warning("\(message, privacy: .public)")
        case .error:
            logger.error("\(message, privacy: .public)")
        }
    }

    private func loggerForCategory(_ category: String) -> Logger {
        switch category {
        case "Client":
            return clientLogger
        case "Cache":
            return cacheLogger
        case "Network":
            return networkLogger
        default:
            return clientLogger
        }
    }
}

public enum SpacetimeObservability {
    public nonisolated(unsafe) static var logger: any SpacetimeLogger = OSLogSpacetimeLogger()
    public nonisolated(unsafe) static var metrics: any SpacetimeMetrics = NoopSpacetimeMetrics()
}

enum Log {
    static let client = LogCategory("Client")
    static let cache = LogCategory("Cache")
    static let network = LogCategory("Network")
}

struct LogCategory {
    private let category: String

    init(_ category: String) {
        self.category = category
    }

    func debug(_ message: String) {
        SpacetimeObservability.logger.log(level: .debug, category: category, message: message)
    }

    func info(_ message: String) {
        SpacetimeObservability.logger.log(level: .info, category: category, message: message)
    }

    func warning(_ message: String) {
        SpacetimeObservability.logger.log(level: .warning, category: category, message: message)
    }

    func error(_ message: String) {
        SpacetimeObservability.logger.log(level: .error, category: category, message: message)
    }
}
