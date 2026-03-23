package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentListOf

/**
 * Thread-safe callback list backed by an atomic persistent list.
 * Reads are zero-copy snapshots; writes use atomic CAS.
 */
public class CallbackList<T> {
    private val list = atomic(persistentListOf<T>())

    public fun add(cb: T) { list.update { it.add(cb) } }
    public fun remove(cb: T) { list.update { it.remove(cb) } }
    public fun isEmpty(): Boolean = list.value.isEmpty()
    public fun isNotEmpty(): Boolean = list.value.isNotEmpty()

    public fun forEach(action: (T) -> Unit) {
        for (item in list.value) action(item)
    }
}
