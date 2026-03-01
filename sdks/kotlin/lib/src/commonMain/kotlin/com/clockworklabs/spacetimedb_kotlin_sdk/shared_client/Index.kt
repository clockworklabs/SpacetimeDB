package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.PersistentList
import kotlinx.collections.immutable.PersistentMap
import kotlinx.collections.immutable.persistentHashMapOf
import kotlinx.collections.immutable.persistentListOf

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
            var snapshot = persistentHashMapOf<Col, Row>()
            for (row in tableCache.iter()) {
                snapshot = snapshot.put(keyExtractor(row), row)
            }
            snapshot
        }
    }

    public fun find(value: Col): Row? = _cache.value[value]
}

/**
 * A client-side non-unique index backed by an atomic persistent map of persistent lists.
 * Provides lookup for all rows matching a given column value.
 * Thread-safe: reads return a consistent snapshot.
 *
 * Subscribes to the TableCache's internal insert/delete hooks
 * to stay synchronized with the cache contents.
 */
public class BTreeIndex<Row, Col>(
    tableCache: TableCache<Row, *>,
    private val keyExtractor: (Row) -> Col,
) {
    private val _cache = atomic(persistentHashMapOf<Col, PersistentList<Row>>())

    init {
        tableCache.addInternalInsertListener { row ->
            val key = keyExtractor(row)
            _cache.update { current ->
                current.put(key, (current[key] ?: persistentListOf()).add(row))
            }
        }
        tableCache.addInternalDeleteListener { row ->
            val key = keyExtractor(row)
            _cache.update { current ->
                val list = current[key] ?: return@update current
                val updated = list.remove(row)
                if (updated.isEmpty()) current.remove(key) else current.put(key, updated)
            }
        }
        _cache.update {
            var snapshot = persistentHashMapOf<Col, PersistentList<Row>>()
            for (row in tableCache.iter()) {
                val key = keyExtractor(row)
                snapshot = snapshot.put(key, (snapshot[key] ?: persistentListOf()).add(row))
            }
            snapshot
        }
    }

    public fun filter(value: Col): List<Row> = _cache.value[value] ?: emptyList()
}
