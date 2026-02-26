package com.clockworklabs.spacetimedb

typealias InsertCallback = (ByteArray) -> Unit
typealias DeleteCallback = (ByteArray) -> Unit
typealias UpdateCallback = (oldRow: ByteArray, newRow: ByteArray) -> Unit

/**
 * Handle for observing row changes on a single table.
 *
 * Obtain via [DbConnection.table]. Register callbacks with [onInsert], [onDelete],
 * and [onUpdate]; remove them later with the returned [CallbackId].
 */
class TableHandle(val tableName: String) {
    private var nextId = 0
    private val insertCallbacks = mutableMapOf<Int, InsertCallback>()
    private val deleteCallbacks = mutableMapOf<Int, DeleteCallback>()
    private val updateCallbacks = mutableMapOf<Int, UpdateCallback>()

    fun onInsert(callback: InsertCallback): CallbackId {
        val id = nextId++
        insertCallbacks[id] = callback
        return CallbackId(id)
    }

    fun onDelete(callback: DeleteCallback): CallbackId {
        val id = nextId++
        deleteCallbacks[id] = callback
        return CallbackId(id)
    }

    fun onUpdate(callback: UpdateCallback): CallbackId {
        val id = nextId++
        updateCallbacks[id] = callback
        return CallbackId(id)
    }

    fun removeOnInsert(id: CallbackId) {
        insertCallbacks.remove(id.value)
    }

    fun removeOnDelete(id: CallbackId) {
        deleteCallbacks.remove(id.value)
    }

    fun removeOnUpdate(id: CallbackId) {
        updateCallbacks.remove(id.value)
    }

    internal fun fireInsert(row: ByteArray) {
        insertCallbacks.values.forEach { it(row) }
    }

    internal fun fireDelete(row: ByteArray) {
        deleteCallbacks.values.forEach { it(row) }
    }

    internal fun fireUpdate(oldRow: ByteArray, newRow: ByteArray) {
        updateCallbacks.values.forEach { it(oldRow, newRow) }
    }
}

/** Opaque identifier returned by callback registration methods. Used to remove the callback later. */
@kotlin.jvm.JvmInline
value class CallbackId(val value: Int)
