import Foundation
import Synchronization

final class LockIsolated<T: Sendable>: Sendable {
    private let lock: Mutex<T>
    init(_ value: T) {
        self.lock = Mutex(value)
    }
    func withValue<R>(_ block: (inout T) -> R) -> R {
        return lock.withLock { block(&$0) }
    }
    var value: T {
        return lock.withLock { $0 }
    }
}
