package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.BsatnRowList
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.RowSizeHint
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentHashMapOf
import kotlinx.collections.immutable.persistentListOf

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
 * Callback that fires after table operations are applied.
 */
internal fun interface PendingCallback {
    /** Executes this deferred callback. */
    fun invoke()
}

/**
 * A decoded row paired with its raw BSATN bytes (used for content-based keying).
 */
internal data class DecodedRow<Row>(val row: Row, val rawBytes: ByteArray) {
    override fun equals(other: Any?): Boolean =
        other is DecodedRow<*> && row == other.row && rawBytes.contentEquals(other.rawBytes)

    override fun hashCode(): Int = 31 * row.hashCode() + rawBytes.contentHashCode()
}

/**
 * Type-erased marker for pre-decoded row data.
 * Produced by [TableCache.parseUpdate] / [TableCache.parseDeletes],
 * consumed by preApply/apply methods.
 * rows are decoded once and the parsed result is passed to all phases.
 */
internal interface ParsedTableData

internal class ParsedPersistentUpdate<Row>(
    val deletes: List<DecodedRow<Row>>,
    val inserts: List<DecodedRow<Row>>,
) : ParsedTableData

internal class ParsedEventUpdate<Row>(
    val events: List<Row>,
) : ParsedTableData

internal class ParsedDeletesOnly<Row>(
    val rows: List<DecodedRow<Row>>,
) : ParsedTableData

/**
 * Per-table cache entry. Stores rows with reference counting
 * to handle overlapping subscriptions.
 *
 * Rows are keyed by their primary key (or full encoded bytes if no PK).
 *
 * @param Row the row type stored in this cache
 * @param Key the key type used to identify rows (typed PK or BsatnRowKey)
 */
@InternalSpacetimeApi
public class TableCache<Row, Key : Any> private constructor(
    private val decode: (BsatnReader) -> Row,
    private val keyExtractor: (Row, ByteArray) -> Key,
) {
    public companion object {
        /** Creates a table cache that keys rows by an extracted primary key. */
        @InternalSpacetimeApi
        public fun <Row, Key : Any> withPrimaryKey(
            decode: (BsatnReader) -> Row,
            primaryKey: (Row) -> Key,
        ): TableCache<Row, Key> = TableCache(decode) { row, _ -> primaryKey(row) }

        /** Creates a table cache that keys rows by their full BSATN-encoded bytes. */
        @InternalSpacetimeApi
        @Suppress("UNCHECKED_CAST")
        public fun <Row> withContentKey(
            decode: (BsatnReader) -> Row,
        ): TableCache<Row, *> = TableCache(decode) { _, bytes -> BsatnRowKey(bytes) }
    }

    // Map<key, Pair<Row, refCount>> — atomic persistent map for thread-safe reads
    private val _rows = atomic(persistentHashMapOf<Key, Pair<Row, Int>>())

    private val _onInsertCallbacks = atomic(persistentListOf<(EventContext, Row) -> Unit>())
    private val _onDeleteCallbacks = atomic(persistentListOf<(EventContext, Row) -> Unit>())
    private val _onUpdateCallbacks = atomic(persistentListOf<(EventContext, Row, Row) -> Unit>())
    private val _onBeforeDeleteCallbacks = atomic(persistentListOf<(EventContext, Row) -> Unit>())

    private val _internalInsertListeners = atomic(persistentListOf<(Row) -> Unit>())
    private val _internalDeleteListeners = atomic(persistentListOf<(Row) -> Unit>())

    internal fun addInternalInsertListener(cb: (Row) -> Unit) { _internalInsertListeners.update { it.add(cb) } }
    internal fun addInternalDeleteListener(cb: (Row) -> Unit) { _internalDeleteListeners.update { it.add(cb) } }

    /** Registers a callback that fires after a row is inserted. */
    public fun onInsert(cb: (EventContext, Row) -> Unit) { _onInsertCallbacks.update { it.add(cb) } }

    /** Registers a callback that fires after a row is deleted. */
    public fun onDelete(cb: (EventContext, Row) -> Unit) { _onDeleteCallbacks.update { it.add(cb) } }

    /** Registers a callback that fires after a row is updated (old row, new row). */
    public fun onUpdate(cb: (EventContext, Row, Row) -> Unit) { _onUpdateCallbacks.update { it.add(cb) } }

    /** Registers a callback that fires before a row is deleted. */
    public fun onBeforeDelete(cb: (EventContext, Row) -> Unit) { _onBeforeDeleteCallbacks.update { it.add(cb) } }

    /** Removes a previously registered insert callback. */
    public fun removeOnInsert(cb: (EventContext, Row) -> Unit) { _onInsertCallbacks.update { it.remove(cb) } }

    /** Removes a previously registered delete callback. */
    public fun removeOnDelete(cb: (EventContext, Row) -> Unit) { _onDeleteCallbacks.update { it.remove(cb) } }

    /** Removes a previously registered update callback. */
    public fun removeOnUpdate(cb: (EventContext, Row, Row) -> Unit) { _onUpdateCallbacks.update { it.remove(cb) } }

    /** Removes a previously registered before-delete callback. */
    public fun removeOnBeforeDelete(cb: (EventContext, Row) -> Unit) { _onBeforeDeleteCallbacks.update { it.remove(cb) } }

    /** Returns the number of rows currently stored in this table. */
    public fun count(): Int = _rows.value.size

    /** Returns a lazy sequence over all rows in this table. */
    public fun iter(): Sequence<Row> = _rows.value.values.asSequence().map { it.first }

    /** Returns a snapshot list of all rows in this table. */
    public fun all(): List<Row> = _rows.value.values.map { it.first }

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
                require(rowSize > 0) { "Server sent FixedSize(0), which violates the protocol invariant" }
                require(rowList.rowsSize % rowSize == 0) {
                    "FixedSize row data not evenly divisible: ${rowList.rowsSize} bytes / $rowSize row size"
                }
                rowList.rowsSize / rowSize
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

    /** Decodes all rows from a [BsatnRowList], discarding raw bytes. */
    internal fun decodeRowList(rowList: BsatnRowList): List<Row> =
        decodeRowListWithBytes(rowList).map { it.row }

    // --- Parse phase: decode once, reuse across preApply/apply ---

    /**
     * Decode a [TableUpdateRows] into a [ParsedTableData] that can be passed
     * to [preApplyUpdate] and [applyUpdate]. Rows are decoded exactly once.
     */
    internal fun parseUpdate(update: TableUpdateRows): ParsedTableData = when (update) {
        is TableUpdateRows.PersistentTable -> ParsedPersistentUpdate(
            deletes = decodeRowListWithBytes(update.deletes),
            inserts = decodeRowListWithBytes(update.inserts),
        )
        is TableUpdateRows.EventTable -> ParsedEventUpdate(
            events = decodeRowListWithBytes(update.events).map { it.row },
        )
    }

    /**
     * Decode a [BsatnRowList] of deletes into a [ParsedTableData] that can be
     * passed to [preApplyDeletes] and [applyDeletes]. Rows are decoded exactly once.
     */
    internal fun parseDeletes(rowList: BsatnRowList): ParsedTableData =
        ParsedDeletesOnly(rows = decodeRowListWithBytes(rowList))

    // --- Insert (single-phase, no pre-apply needed) ---

    /**
     * Apply insert operations from a BsatnRowList.
     * Returns pending callbacks to execute after all tables are updated.
     */
    internal fun applyInserts(ctx: EventContext, rowList: BsatnRowList): List<PendingCallback> {
        val decoded = decodeRowListWithBytes(rowList)
        val callbacks = mutableListOf<PendingCallback>()
        val newInserts = mutableListOf<Row>()
        _rows.update { current ->
            callbacks.clear()
            newInserts.clear()
            val insertCbs = _onInsertCallbacks.value
            var snapshot = current
            for ((row, rawBytes) in decoded) {
                val id = keyExtractor(row, rawBytes)
                val existing = snapshot[id]
                if (existing != null) {
                    snapshot = snapshot.put(id, Pair(existing.first, existing.second + 1))
                } else {
                    snapshot = snapshot.put(id, Pair(row, 1))
                    newInserts.add(row)
                    if (insertCbs.isNotEmpty()) {
                        callbacks.add(PendingCallback {
                            for (cb in insertCbs) cb(ctx, row)
                        })
                    }
                }
            }
            snapshot
        }
        for (row in newInserts) {
            for (listener in _internalInsertListeners.value) listener(row)
        }
        return callbacks
    }

    // --- Unsubscribe deletes (two-phase) ---

    /**
     * Phase 1 for unsubscribe deletes: fires onBeforeDelete callbacks
     * BEFORE any mutations happen, enabling cross-table consistency.
     * Accepts pre-decoded data from [parseDeletes].
     */
    @Suppress("UNCHECKED_CAST")
    internal fun preApplyDeletes(ctx: EventContext, parsed: ParsedTableData) {
        if (_onBeforeDeleteCallbacks.value.isEmpty()) return
        val data = parsed as ParsedDeletesOnly<Row>
        val snapshot = _rows.value
        for ((row, rawBytes) in data.rows) {
            val id = keyExtractor(row, rawBytes)
            val existing = snapshot[id] ?: continue
            if (existing.second <= 1) {
                for (cb in _onBeforeDeleteCallbacks.value) cb(ctx, existing.first)
            }
        }
    }

    /**
     * Phase 2 for unsubscribe deletes: mutates rows and returns post-mutation callbacks.
     * onBeforeDelete must be called via [preApplyDeletes] before this.
     * Accepts pre-decoded data from [parseDeletes].
     */
    @Suppress("UNCHECKED_CAST")
    internal fun applyDeletes(ctx: EventContext, parsed: ParsedTableData): List<PendingCallback> {
        val data = parsed as ParsedDeletesOnly<Row>
        val callbacks = mutableListOf<PendingCallback>()
        val removedRows = mutableListOf<Row>()
        _rows.update { current ->
            callbacks.clear()
            removedRows.clear()
            val deleteCbs = _onDeleteCallbacks.value
            var snapshot = current
            for ((row, rawBytes) in data.rows) {
                val id = keyExtractor(row, rawBytes)
                val existing = snapshot[id] ?: continue
                if (existing.second <= 1) {
                    val capturedRow = existing.first
                    snapshot = snapshot.remove(id)
                    removedRows.add(capturedRow)
                    if (deleteCbs.isNotEmpty()) {
                        callbacks.add(PendingCallback {
                            for (cb in deleteCbs) cb(ctx, capturedRow)
                        })
                    }
                } else {
                    snapshot = snapshot.put(id, Pair(existing.first, existing.second - 1))
                }
            }
            snapshot
        }
        for (row in removedRows) {
            for (listener in _internalDeleteListeners.value) listener(row)
        }
        return callbacks
    }

    // --- Transaction updates (two-phase) ---

    /**
     * Phase 1 for transaction updates: fires onBeforeDelete callbacks
     * for rows that will be deleted (not updated), BEFORE any mutations happen.
     * Accepts pre-decoded data from [parseUpdate].
     */
    @Suppress("UNCHECKED_CAST")
    internal fun preApplyUpdate(ctx: EventContext, parsed: ParsedTableData) {
        if (_onBeforeDeleteCallbacks.value.isEmpty()) return
        val update = parsed as? ParsedPersistentUpdate<Row> ?: return

        // Build insert key set for update detection
        val insertKeys = mutableSetOf<Key>()
        for ((row, rawBytes) in update.inserts) insertKeys.add(keyExtractor(row, rawBytes))

        // Fire onBeforeDelete for pure deletes only (not updates)
        val snapshot = _rows.value
        for ((row, rawBytes) in update.deletes) {
            val id = keyExtractor(row, rawBytes)
            if (id in insertKeys) continue // This is an update, not a delete
            val existing = snapshot[id] ?: continue
            if (existing.second <= 1) {
                for (cb in _onBeforeDeleteCallbacks.value) cb(ctx, existing.first)
            }
        }
    }

    /**
     * Phase 2 for transaction updates: mutates rows and returns post-mutation callbacks.
     * onBeforeDelete must be called via [preApplyUpdate] before this.
     * Accepts pre-decoded data from [parseUpdate].
     */
    @Suppress("UNCHECKED_CAST")
    internal fun applyUpdate(ctx: EventContext, parsed: ParsedTableData): List<PendingCallback> {
        return when (parsed) {
            is ParsedPersistentUpdate<*> -> {
                val update = parsed as ParsedPersistentUpdate<Row>

                // Build delete map for pairing with inserts
                val deleteMap = mutableMapOf<Key, Row>()
                for ((row, rawBytes) in update.deletes) deleteMap[keyExtractor(row, rawBytes)] = row

                val callbacks = mutableListOf<PendingCallback>()
                val updatedRows = mutableListOf<Pair<Row, Row>>()
                val newInserts = mutableListOf<Row>()
                val removedRows = mutableListOf<Row>()

                _rows.update { current ->
                    callbacks.clear()
                    updatedRows.clear()
                    newInserts.clear()
                    removedRows.clear()
                    val insertCbs = _onInsertCallbacks.value
                    val deleteCbs = _onDeleteCallbacks.value
                    val updateCbs = _onUpdateCallbacks.value
                    val localDeleteMap = deleteMap.toMutableMap()
                    var snapshot = current

                    // Process inserts — check for matching delete (= update)
                    for ((row, rawBytes) in update.inserts) {
                        val id = keyExtractor(row, rawBytes)
                        val deletedRow = localDeleteMap.remove(id)
                        if (deletedRow != null) {
                            // Update: same key in both insert and delete
                            val oldRow = snapshot[id]?.first ?: deletedRow
                            snapshot = snapshot.put(id, Pair(row, snapshot[id]?.second ?: 1))
                            updatedRows.add(oldRow to row)
                            if (updateCbs.isNotEmpty()) {
                                callbacks.add(PendingCallback {
                                    for (cb in updateCbs) cb(ctx, oldRow, row)
                                })
                            }
                        } else {
                            // Pure insert
                            val existing = snapshot[id]
                            if (existing != null) {
                                snapshot = snapshot.put(id, Pair(existing.first, existing.second + 1))
                            } else {
                                snapshot = snapshot.put(id, Pair(row, 1))
                                newInserts.add(row)
                                if (insertCbs.isNotEmpty()) {
                                    callbacks.add(PendingCallback {
                                        for (cb in insertCbs) cb(ctx, row)
                                    })
                                }
                            }
                        }
                    }

                    // Remaining deletes: pure deletes (onBeforeDelete already fired in preApplyUpdate)
                    for ((id, _) in localDeleteMap) {
                        val existing = snapshot[id] ?: continue
                        if (existing.second <= 1) {
                            val capturedRow = existing.first
                            snapshot = snapshot.remove(id)
                            removedRows.add(capturedRow)
                            if (deleteCbs.isNotEmpty()) {
                                callbacks.add(PendingCallback {
                                    for (cb in deleteCbs) cb(ctx, capturedRow)
                                })
                            }
                        } else {
                            snapshot = snapshot.put(id, Pair(existing.first, existing.second - 1))
                        }
                    }

                    snapshot
                }

                // Fire internal listeners after CAS succeeds
                for ((oldRow, newRow) in updatedRows) {
                    for (listener in _internalDeleteListeners.value) listener(oldRow)
                    for (listener in _internalInsertListeners.value) listener(newRow)
                }
                for (row in newInserts) {
                    for (listener in _internalInsertListeners.value) listener(row)
                }
                for (row in removedRows) {
                    for (listener in _internalDeleteListeners.value) listener(row)
                }

                callbacks
            }
            is ParsedEventUpdate<*> -> {
                // Event table: fire insert callbacks, but don't store
                val events = (parsed as ParsedEventUpdate<Row>).events
                val insertCbs = _onInsertCallbacks.value
                val callbacks = mutableListOf<PendingCallback>()
                for (row in events) {
                    if (insertCbs.isNotEmpty()) {
                        callbacks.add(PendingCallback {
                            for (cb in insertCbs) cb(ctx, row)
                        })
                    }
                }
                callbacks
            }
            else -> emptyList()
        }
    }

    /**
     * Clear all rows (used on disconnect).
     */
    internal fun clear() {
        val oldRows = _rows.getAndSet(persistentHashMapOf())
        val listeners = _internalDeleteListeners.value
        if (listeners.isNotEmpty()) {
            for ((_, pair) in oldRows) {
                for (listener in listeners) listener(pair.first)
            }
        }
    }
}

/**
 * Client-side cache holding all table caches.
 * Registry of [TableCache] instances keyed by table name.
 */
@InternalSpacetimeApi
public class ClientCache {
    private val _tables = atomic(persistentHashMapOf<String, TableCache<*, *>>())

    /** Registers a [TableCache] under the given table name. */
    @InternalSpacetimeApi
    public fun <Row, Key : Any> register(tableName: String, cache: TableCache<Row, Key>) {
        _tables.update { it.put(tableName, cache) }
    }

    /** Returns the table cache for [tableName], throwing if not registered. */
    @Suppress("UNCHECKED_CAST")
    internal fun <Row> getTable(tableName: String): TableCache<Row, *> =
        _tables.value[tableName] as? TableCache<Row, *>
            ?: error("Table '$tableName' not found in client cache")

    /** Returns the table cache for [tableName], or `null` if not registered. */
    @Suppress("UNCHECKED_CAST")
    internal fun <Row> getTableOrNull(tableName: String): TableCache<Row, *>? =
        _tables.value[tableName] as? TableCache<Row, *>

    /** Returns the table cache for [tableName], creating it via [factory] if not yet registered. */
    @InternalSpacetimeApi
    @Suppress("UNCHECKED_CAST")
    public fun <Row> getOrCreateTable(tableName: String, factory: () -> TableCache<Row, *>): TableCache<Row, *> {
        // Fast path: already registered
        _tables.value[tableName]?.let { return it as TableCache<Row, *> }

        // Create once outside the CAS loop so factory() is never called on retry
        val created = factory()
        var result: TableCache<Row, *>? = null
        _tables.update { map ->
            val existing = map[tableName]
            if (existing != null) {
                result = existing as TableCache<Row, *>
                map
            } else {
                result = created
                map.put(tableName, created)
            }
        }
        return result!!
    }

    /** Returns the table cache for [tableName] without casting, or `null` if not registered. */
    internal fun getUntypedTable(tableName: String): TableCache<*, *>? =
        _tables.value[tableName]

    /** Returns the set of all registered table names. */
    internal fun tableNames(): Set<String> = _tables.value.keys

    /** Clears all rows from every registered table cache. */
    internal fun clear() {
        for ((_, table) in _tables.value) table.clear()
    }
}
