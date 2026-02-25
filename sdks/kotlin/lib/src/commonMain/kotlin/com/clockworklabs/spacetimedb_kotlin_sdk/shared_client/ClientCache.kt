@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.BsatnRowList
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.RowSizeHint
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows

/**
 * Wrapper for ByteArray that provides structural equality/hashCode.
 * Used as a map key for rows without a primary key (content-based keying via BSATN bytes).
 */
internal class BsatnRowKey(val bytes: ByteArray) {
    override fun equals(other: Any?): Boolean =
        other is BsatnRowKey && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = bytes.contentHashCode()
}

/**
 * Operation representing a row change, used in callbacks.
 */
sealed interface Operation<out Row> {
    data class Insert<Row>(val row: Row) : Operation<Row>
    data class Delete<Row>(val row: Row) : Operation<Row>
    data class Update<Row>(val oldRow: Row, val newRow: Row) : Operation<Row>
}

/**
 * Callback that fires after table operations are applied.
 */
fun interface PendingCallback {
    fun invoke()
}

/**
 * Per-table cache entry. Stores rows with reference counting
 * to handle overlapping subscriptions (matching TS SDK's TableCache).
 *
 * Rows are keyed by their primary key (or full encoded bytes if no PK).
 *
 * @param Row the row type stored in this cache
 * @param Key the key type used to identify rows (typed PK or BsatnRowKey)
 */
class TableCache<Row, Key : Any> private constructor(
    private val decode: (BsatnReader) -> Row,
    private val keyExtractor: (Row, ByteArray) -> Key,
) {
    companion object {
        fun <Row, Key : Any> withPrimaryKey(
            decode: (BsatnReader) -> Row,
            primaryKey: (Row) -> Key,
        ): TableCache<Row, Key> = TableCache(decode) { row, _ -> primaryKey(row) }

        @Suppress("UNCHECKED_CAST")
        fun <Row> withContentKey(
            decode: (BsatnReader) -> Row,
        ): TableCache<Row, *> = TableCache<Row, BsatnRowKey>(decode) { _, bytes -> BsatnRowKey(bytes) }
    }

    // Map<key, Pair<Row, refCount>>
    private val rows = mutableMapOf<Key, Pair<Row, Int>>()

    private val onInsertCallbacks = mutableListOf<(EventContext, Row) -> Unit>()
    private val onDeleteCallbacks = mutableListOf<(EventContext, Row) -> Unit>()
    private val onUpdateCallbacks = mutableListOf<(EventContext, Row, Row) -> Unit>()
    private val onBeforeDeleteCallbacks = mutableListOf<(EventContext, Row) -> Unit>()

    internal val internalInsertListeners = mutableListOf<(Row) -> Unit>()
    internal val internalDeleteListeners = mutableListOf<(Row) -> Unit>()

    fun onInsert(cb: (EventContext, Row) -> Unit) { onInsertCallbacks.add(cb) }
    fun onDelete(cb: (EventContext, Row) -> Unit) { onDeleteCallbacks.add(cb) }
    fun onUpdate(cb: (EventContext, Row, Row) -> Unit) { onUpdateCallbacks.add(cb) }
    fun onBeforeDelete(cb: (EventContext, Row) -> Unit) { onBeforeDeleteCallbacks.add(cb) }

    fun removeOnInsert(cb: (EventContext, Row) -> Unit) { onInsertCallbacks.remove(cb) }
    fun removeOnDelete(cb: (EventContext, Row) -> Unit) { onDeleteCallbacks.remove(cb) }
    fun removeOnUpdate(cb: (EventContext, Row, Row) -> Unit) { onUpdateCallbacks.remove(cb) }
    fun removeOnBeforeDelete(cb: (EventContext, Row) -> Unit) { onBeforeDeleteCallbacks.remove(cb) }

    fun count(): Int = rows.size

    fun iter(): Iterator<Row> = rows.values.map { it.first }.iterator()

    fun all(): List<Row> = rows.values.map { it.first }

    /**
     * A decoded row paired with its raw BSATN bytes (used for content-based keying).
     */
    private data class DecodedRow<Row>(val row: Row, val rawBytes: ByteArray)

    /**
     * Decode rows from a BsatnRowList, capturing raw BSATN bytes per row.
     */
    private fun decodeRowListWithBytes(rowList: BsatnRowList): List<DecodedRow<Row>> {
        if (rowList.rowsSize == 0) return emptyList()
        val reader = rowList.rowsReader
        val result = mutableListOf<DecodedRow<Row>>()
        val rowCount = when (val hint = rowList.sizeHint) {
            is RowSizeHint.FixedSize -> {
                val rowSize = hint.size.toInt()
                if (rowSize > 0) rowList.rowsSize / rowSize else 0
            }
            is RowSizeHint.RowOffsets -> hint.offsets.size
        }
        repeat(rowCount) {
            val startOffset = reader.offset
            val row = decode(reader)
            val rawBytes = reader.sliceArray(startOffset, reader.offset)
            result.add(DecodedRow(row, rawBytes))
        }
        return result
    }

    fun decodeRowList(rowList: BsatnRowList): List<Row> =
        decodeRowListWithBytes(rowList).map { it.row }

    /**
     * Apply insert operations from a BsatnRowList.
     * Returns pending callbacks to execute after all tables are updated.
     */
    fun applyInserts(ctx: EventContext, rowList: BsatnRowList): List<PendingCallback> {
        val decoded = decodeRowListWithBytes(rowList)
        val callbacks = mutableListOf<PendingCallback>()
        for ((row, rawBytes) in decoded) {
            val id = keyExtractor(row, rawBytes)
            val existing = rows[id]
            if (existing != null) {
                // Increment ref count
                rows[id] = Pair(existing.first, existing.second + 1)
            } else {
                rows[id] = Pair(row, 1)
                for (listener in internalInsertListeners) listener(row)
                if (onInsertCallbacks.isNotEmpty()) {
                    callbacks.add(PendingCallback {
                        for (cb in onInsertCallbacks) cb(ctx, row)
                    })
                }
            }
        }
        return callbacks
    }

    /**
     * Phase 1 for unsubscribe deletes: fires onBeforeDelete callbacks
     * BEFORE any mutations happen, enabling cross-table consistency.
     */
    fun preApplyDeletes(ctx: EventContext, rowList: BsatnRowList) {
        if (onBeforeDeleteCallbacks.isEmpty()) return
        val decoded = decodeRowListWithBytes(rowList)
        for ((row, rawBytes) in decoded) {
            val id = keyExtractor(row, rawBytes)
            val existing = rows[id] ?: continue
            if (existing.second <= 1) {
                for (cb in onBeforeDeleteCallbacks) cb(ctx, existing.first)
            }
        }
    }

    /**
     * Apply delete operations from a BsatnRowList.
     * Returns pending callbacks to execute after all tables are updated.
     * Note: onBeforeDelete must be called via preApplyDeletes() before this.
     */
    fun applyDeletes(ctx: EventContext, rowList: BsatnRowList): List<PendingCallback> {
        val decoded = decodeRowListWithBytes(rowList)
        val callbacks = mutableListOf<PendingCallback>()
        for ((row, rawBytes) in decoded) {
            val id = keyExtractor(row, rawBytes)
            val existing = rows[id] ?: continue
            if (existing.second <= 1) {
                val capturedRow = existing.first
                rows.remove(id)
                for (listener in internalDeleteListeners) listener(capturedRow)
                if (onDeleteCallbacks.isNotEmpty()) {
                    callbacks.add(PendingCallback {
                        for (cb in onDeleteCallbacks) cb(ctx, capturedRow)
                    })
                }
            } else {
                rows[id] = Pair(existing.first, existing.second - 1)
            }
        }
        return callbacks
    }

    /**
     * Phase 1 for transaction updates: fires onBeforeDelete callbacks
     * for rows that will be deleted (not updated), BEFORE any mutations happen.
     */
    fun preApplyUpdate(ctx: EventContext, update: TableUpdateRows) {
        if (onBeforeDeleteCallbacks.isEmpty()) return
        when (update) {
            is TableUpdateRows.PersistentTable -> {
                val deleteDecoded = decodeRowListWithBytes(update.deletes)
                val insertDecoded = decodeRowListWithBytes(update.inserts)

                // Build insert key set for update detection
                val insertKeys = mutableSetOf<Key>()
                for ((row, rawBytes) in insertDecoded) insertKeys.add(keyExtractor(row, rawBytes))

                // Fire onBeforeDelete for pure deletes only (not updates)
                for ((row, rawBytes) in deleteDecoded) {
                    val id = keyExtractor(row, rawBytes)
                    if (id in insertKeys) continue // This is an update, not a delete
                    val existing = rows[id] ?: continue
                    if (existing.second <= 1) {
                        for (cb in onBeforeDeleteCallbacks) cb(ctx, existing.first)
                    }
                }
            }
            is TableUpdateRows.EventTable -> {
                // Event tables have no deletes
            }
        }
    }

    /**
     * Phase 2 for transaction updates: mutates rows and returns post-mutation callbacks.
     * onBeforeDelete must be called via preApplyUpdate() before this.
     *
     * Matches TS SDK pattern: iterate inserts, consume matching deletes inline,
     * then process remaining deletes.
     */
    fun applyUpdate(ctx: EventContext, update: TableUpdateRows): List<PendingCallback> {
        return when (update) {
            is TableUpdateRows.PersistentTable -> {
                val deleteDecoded = decodeRowListWithBytes(update.deletes)
                val insertDecoded = decodeRowListWithBytes(update.inserts)

                // Build delete map for pairing with inserts
                val deleteMap = mutableMapOf<Key, Row>()
                for ((row, rawBytes) in deleteDecoded) deleteMap[keyExtractor(row, rawBytes)] = row

                val callbacks = mutableListOf<PendingCallback>()

                // Process inserts — check for matching delete (= update)
                for ((row, rawBytes) in insertDecoded) {
                    val id = keyExtractor(row, rawBytes)
                    val deletedRow = deleteMap.remove(id)
                    if (deletedRow != null) {
                        // Update: same key in both insert and delete
                        val oldRow = rows[id]?.first ?: deletedRow
                        rows[id] = Pair(row, rows[id]?.second ?: 1)
                        for (listener in internalDeleteListeners) listener(oldRow)
                        for (listener in internalInsertListeners) listener(row)
                        if (onUpdateCallbacks.isNotEmpty()) {
                            callbacks.add(PendingCallback {
                                for (cb in onUpdateCallbacks) cb(ctx, oldRow, row)
                            })
                        }
                    } else {
                        // Pure insert
                        val existing = rows[id]
                        if (existing != null) {
                            rows[id] = Pair(existing.first, existing.second + 1)
                        } else {
                            rows[id] = Pair(row, 1)
                            for (listener in internalInsertListeners) listener(row)
                            if (onInsertCallbacks.isNotEmpty()) {
                                callbacks.add(PendingCallback {
                                    for (cb in onInsertCallbacks) cb(ctx, row)
                                })
                            }
                        }
                    }
                }

                // Remaining deletes: pure deletes (onBeforeDelete already fired in preApplyUpdate)
                for ((id, _) in deleteMap) {
                    val existing = rows[id] ?: continue
                    if (existing.second <= 1) {
                        val capturedRow = existing.first
                        rows.remove(id)
                        for (listener in internalDeleteListeners) listener(capturedRow)
                        if (onDeleteCallbacks.isNotEmpty()) {
                            callbacks.add(PendingCallback {
                                for (cb in onDeleteCallbacks) cb(ctx, capturedRow)
                            })
                        }
                    } else {
                        rows[id] = Pair(existing.first, existing.second - 1)
                    }
                }

                callbacks
            }
            is TableUpdateRows.EventTable -> {
                // Event table: decode and fire insert callbacks, but don't store
                val decoded = decodeRowListWithBytes(update.events).map { it.row }
                val callbacks = mutableListOf<PendingCallback>()
                for (row in decoded) {
                    if (onInsertCallbacks.isNotEmpty()) {
                        val capturedRow = row
                        callbacks.add(PendingCallback {
                            for (cb in onInsertCallbacks) cb(ctx, capturedRow)
                        })
                    }
                }
                callbacks
            }
        }
    }

    /**
     * Clear all rows (used on disconnect).
     */
    fun clear() {
        if (internalDeleteListeners.isNotEmpty()) {
            for ((_, pair) in rows) {
                for (listener in internalDeleteListeners) listener(pair.first)
            }
        }
        rows.clear()
    }
}

/**
 * Client-side cache holding all table caches.
 * Mirrors TS SDK's ClientCache — registry of TableCache instances by table name.
 */
class ClientCache {
    private val tables = mutableMapOf<String, TableCache<*, *>>()

    fun <Row, Key : Any> register(tableName: String, cache: TableCache<Row, Key>) {
        tables[tableName] = cache
    }

    @Suppress("UNCHECKED_CAST")
    fun <Row> getTable(tableName: String): TableCache<Row, *> =
        tables[tableName] as? TableCache<Row, *>
            ?: error("Table '$tableName' not found in client cache")

    @Suppress("UNCHECKED_CAST")
    fun <Row> getTableOrNull(tableName: String): TableCache<Row, *>? =
        tables[tableName] as? TableCache<Row, *>

    @Suppress("UNCHECKED_CAST")
    fun <Row> getOrCreateTable(tableName: String, factory: () -> TableCache<Row, *>): TableCache<Row, *> {
        return tables.getOrPut(tableName) { factory() } as TableCache<Row, *>
    }

    fun getUntypedTable(tableName: String): TableCache<*, *>? =
        tables[tableName]

    fun tableNames(): Set<String> = tables.keys

    fun clear() {
        for (table in tables.values) table.clear()
    }
}
