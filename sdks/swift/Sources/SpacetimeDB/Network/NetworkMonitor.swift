import Foundation
import Network
import Synchronization

/// Monitors network path changes for connectivity-aware reconnection.
///
/// When the network becomes unavailable, the client defers reconnection attempts
/// (saving battery and avoiding spurious errors). When the network is restored,
/// an immediate reconnection is triggered.
final class NetworkMonitor: Sendable {
    private struct State {
        var isConnected: Bool = true
        var onPathChange: (@Sendable (Bool) -> Void)?
    }

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "spacetimedb.network-monitor", qos: .utility)
    private let stateLock: Mutex<State> = Mutex(State())

    var isConnected: Bool {
        stateLock.withLock { state in
            state.isConnected
        }
    }

    var onPathChange: (@Sendable (Bool) -> Void)? {
        get {
            stateLock.withLock { state in
                state.onPathChange
            }
        }
        set {
            stateLock.withLock { state in
                state.onPathChange = newValue
            }
        }
    }

    func start() {
        monitor.pathUpdateHandler = { [weak self] path in
            let satisfied = path.status == .satisfied
            guard let self else { return }
            let callback = self.stateLock.withLock { state -> (@Sendable (Bool) -> Void)? in
                let changed = state.isConnected != satisfied
                guard changed else { return nil }
                state.isConnected = satisfied
                return state.onPathChange
            }

            if let callback {
                Log.network.info("Network path changed: \(satisfied ? "connected" : "disconnected")")
                callback(satisfied)
            }
        }
        monitor.start(queue: queue)
    }

    func stop() {
        monitor.cancel()
    }
}
