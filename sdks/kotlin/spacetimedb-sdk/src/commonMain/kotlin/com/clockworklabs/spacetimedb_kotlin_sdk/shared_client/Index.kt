package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.PersistentSet
import kotlinx.collections.immutable.persistentHashMapOf
import kotlinx.collections.immutable.persistentHashSetOf

/**
 * A client-side unique index backed by an atomic persistent map.
 * Provides O(1) lookup by the indexed column value.
 * Thread-safe: reads return a consistent snapshot.
 *
 * Subscribes to the TableCache's internal insert/delete hooks
 * to stay synchronized with the cache contents.
 */
public class UniqueIndex<Row, Col>(
    tableCache: TableCache<Row, *>,
    private val keyExtractor: (Row) -> Col,
) {
    private val _cache = atomic(persistentHashMapOf<Col, Row>())

    init {
        // Register listeners before populating so rows inserted concurrently
        // cause a CAS retry in the population update, picking them up via iter().
        tableCache.addInternalInsertListener { row ->
            _cache.update { it.put(keyExtractor(row), row) }
        }
        tableCache.addInternalDeleteListener { row ->
            _cache.update { it.remove(keyExtractor(row)) }
        }
        _cache.update {
            val builder = it.builder()
            for (row in tableCache.iter()) {
                builder[keyExtractor(row)] = row
            }
            builder.build()
        }
    }

    /** Returns the row matching [value], or `null` if no match. */
    public fun find(value: Col): Row? = _cache.value[value]
}

/**
 * A client-side non-unique index backed by an atomic persistent map of persistent sets.
 * Provides lookup for all rows matching a given column value.
 * Thread-safe: reads return a consistent snapshot.
 *
 * Uses [PersistentSet] (not List) so that add is idempotent — if the listener
 * and the population loop both add the same row during init, no duplicate is produced.
 *
 * Subscribes to the TableCache's internal insert/delete hooks
 * to stay synchronized with the cache contents.
 */
public class BTreeIndex<Row, Col>(
    tableCache: TableCache<Row, *>,
    private val keyExtractor: (Row) -> Col,
) {
    private val _cache = atomic(persistentHashMapOf<Col, PersistentSet<Row>>())

    init {
        tableCache.addInternalInsertListener { row ->
            val key = keyExtractor(row)
            _cache.update { current ->
                current.put(key, (current[key] ?: persistentHashSetOf()).add(row))
            }
        }
        tableCache.addInternalDeleteListener { row ->
            val key = keyExtractor(row)
            _cache.update { current ->
                val set = current[key] ?: return@update current
                val updated = set.remove(row)
                if (updated.isEmpty()) current.remove(key) else current.put(key, updated)
            }
        }
        _cache.update { current ->
            val builder = current.builder()
            for (row in tableCache.iter()) {
                val key = keyExtractor(row)
                builder[key] = (builder[key] ?: persistentHashSetOf()).add(row)
            }
            builder.build()
        }
    }

    /** Returns all rows matching [value], or an empty set if none. */
    public fun filter(value: Col): Set<Row> = _cache.value[value] ?: emptySet()
}
