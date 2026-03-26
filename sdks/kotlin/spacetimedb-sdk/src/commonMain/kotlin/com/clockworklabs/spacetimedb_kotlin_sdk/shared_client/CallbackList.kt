package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentListOf

/**
 * Thread-safe callback list backed by an atomic persistent list.
 * Reads are zero-copy snapshots; writes use atomic CAS.
 */
@InternalSpacetimeApi
public class CallbackList<T> {
    private val list = atomic(persistentListOf<T>())

    /** Registers a callback. */
    public fun add(cb: T) { list.update { it.add(cb) } }
    /** Removes a previously registered callback. */
    public fun remove(cb: T) { list.update { it.remove(cb) } }
    /** Whether this list contains no callbacks. */
    public fun isEmpty(): Boolean = list.value.isEmpty()
    /** Whether this list contains at least one callback. */
    public fun isNotEmpty(): Boolean = list.value.isNotEmpty()

    /** Invokes [action] on a snapshot of currently registered callbacks. */
    public fun forEach(action: (T) -> Unit) {
        for (item in list.value) action(item)
    }
}
