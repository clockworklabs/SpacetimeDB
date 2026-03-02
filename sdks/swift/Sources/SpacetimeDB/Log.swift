import os

enum Log {
    static let client = Logger(subsystem: "com.clockworklabs.SpacetimeDB", category: "Client")
    static let cache = Logger(subsystem: "com.clockworklabs.SpacetimeDB", category: "Cache")
    static let network = Logger(subsystem: "com.clockworklabs.SpacetimeDB", category: "Network")
}
