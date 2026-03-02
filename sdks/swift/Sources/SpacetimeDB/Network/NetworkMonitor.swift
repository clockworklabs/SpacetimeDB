import Network

/// Monitors network path changes for connectivity-aware reconnection.
///
/// When the network becomes unavailable, the client defers reconnection attempts
/// (saving battery and avoiding spurious errors). When the network is restored,
/// an immediate reconnection is triggered.
@MainActor
final class NetworkMonitor {
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "spacetimedb.network-monitor", qos: .utility)

    private(set) var isConnected: Bool = true
    var onPathChange: ((Bool) -> Void)?

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            let satisfied = path.status == .satisfied
            Task { @MainActor [weak self] in
                guard let self, self.isConnected != satisfied else { return }
                self.isConnected = satisfied
                Log.network.info("Network path changed: \(satisfied ? "connected" : "disconnected")")
                self.onPathChange?(satisfied)
            }
        }
        monitor.start(queue: queue)
    }

    func stop() {
        monitor.cancel()
    }
}
