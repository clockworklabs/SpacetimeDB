@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A client-side unique index backed by a HashMap.
 * Provides O(1) lookup by the indexed column value.
 *
 * Subscribes to the TableCache's internal insert/delete hooks
 * to stay synchronized with the cache contents.
 */
class UniqueIndex<Row, Col>(
    tableCache: TableCache<Row, *>,
    private val keyExtractor: (Row) -> Col,
) {
    private val cache = HashMap<Col, Row>()

    init {
        for (row in tableCache.iter()) {
            cache[keyExtractor(row)] = row
        }
        tableCache.internalInsertListeners.add { row ->
            cache[keyExtractor(row)] = row
        }
        tableCache.internalDeleteListeners.add { row ->
            cache.remove(keyExtractor(row))
        }
    }

    fun find(value: Col): Row? = cache[value]
}

/**
 * A client-side non-unique index backed by a HashMap of MutableLists.
 * Provides O(1) lookup for all rows matching a given column value.
 *
 * Subscribes to the TableCache's internal insert/delete hooks
 * to stay synchronized with the cache contents.
 */
class BTreeIndex<Row, Col>(
    tableCache: TableCache<Row, *>,
    private val keyExtractor: (Row) -> Col,
) {
    private val cache = HashMap<Col, MutableList<Row>>()

    init {
        for (row in tableCache.iter()) {
            val key = keyExtractor(row)
            cache.getOrPut(key) { mutableListOf() }.add(row)
        }
        tableCache.internalInsertListeners.add { row ->
            val key = keyExtractor(row)
            cache.getOrPut(key) { mutableListOf() }.add(row)
        }
        tableCache.internalDeleteListeners.add { row ->
            val key = keyExtractor(row)
            cache[key]?.let { list ->
                list.remove(row)
                if (list.isEmpty()) cache.remove(key)
            }
        }
    }

    fun filter(value: Col): List<Row> = cache[value]?.toList() ?: emptyList()
}
