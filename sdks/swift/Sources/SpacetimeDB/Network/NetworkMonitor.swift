import Foundation
import Network

/// Monitors network path changes for connectivity-aware reconnection.
///
/// When the network becomes unavailable, the client defers reconnection attempts
/// (saving battery and avoiding spurious errors). When the network is restored,
/// an immediate reconnection is triggered.
final class NetworkMonitor: @unchecked Sendable {
    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "spacetimedb.network-monitor", qos: .utility)
    private let lock = NSLock()

    private var _isConnected: Bool = true
    var isConnected: Bool {
        lock.lock(); defer { lock.unlock() }; return _isConnected
    }

    var onPathChange: (@Sendable (Bool) -> Void)?

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            let satisfied = path.status == .satisfied
            guard let self else { return }
            self.lock.lock()
            let changed = self._isConnected != satisfied
            if changed {
                self._isConnected = satisfied
            }
            self.lock.unlock()

            if changed {
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
