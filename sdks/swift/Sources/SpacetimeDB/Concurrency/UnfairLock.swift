import Foundation
import os

/// A non-reentrant, low-overhead lock for hot paths.
/// Backed by `os_unfair_lock` pinned in heap memory.
public final class UnfairLock: @unchecked Sendable {
    private let lockPtr: os_unfair_lock_t

    public init() {
        lockPtr = .allocate(capacity: 1)
        lockPtr.initialize(to: os_unfair_lock())
    }

    deinit {
        lockPtr.deinitialize(count: 1)
        lockPtr.deallocate()
    }

    @inline(__always)
    public func lock() {
        os_unfair_lock_lock(lockPtr)
    }

    @inline(__always)
    public func unlock() {
        os_unfair_lock_unlock(lockPtr)
    }

    @inline(__always)
    public func withLock<R>(_ body: () throws -> R) rethrows -> R {
        os_unfair_lock_lock(lockPtr)
        defer { os_unfair_lock_unlock(lockPtr) }
        return try body()
    }
}
