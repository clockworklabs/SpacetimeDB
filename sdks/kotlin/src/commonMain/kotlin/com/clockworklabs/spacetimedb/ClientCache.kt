package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.protocol.PersistentTableRows
import com.clockworklabs.spacetimedb.protocol.QueryRows
import com.clockworklabs.spacetimedb.protocol.QuerySetUpdate
import com.clockworklabs.spacetimedb.protocol.TableUpdateRows

class ClientCache {
    private val tables = mutableMapOf<String, TableCache>()

    fun getOrCreateTable(name: String): TableCache =
        tables.getOrPut(name) { TableCache(name) }

    fun getTable(name: String): TableCache? = tables[name]

    fun tableNames(): Set<String> = tables.keys.toSet()

    fun applySubscribeRows(rows: QueryRows) {
        for (singleTable in rows.tables) {
            val tableName = singleTable.table.value
            val cache = getOrCreateTable(tableName)
            val decodedRows = singleTable.rows.decodeRows()
            for (row in decodedRows) {
                cache.insertRow(row)
            }
        }
    }

    fun applyUnsubscribeRows(rows: QueryRows) {
        for (singleTable in rows.tables) {
            val tableName = singleTable.table.value
            val cache = getTable(tableName) ?: continue
            val decodedRows = singleTable.rows.decodeRows()
            for (row in decodedRows) {
                cache.deleteRow(row)
            }
        }
    }

    fun applyTransactionUpdate(querySets: List<QuerySetUpdate>): List<TableOperation> {
        val operations = mutableListOf<TableOperation>()
        for (qsUpdate in querySets) {
            for (tableUpdate in qsUpdate.tables) {
                val tableName = tableUpdate.tableName.value
                val cache = getOrCreateTable(tableName)
                for (rowUpdate in tableUpdate.rows) {
                    when (rowUpdate) {
                        is TableUpdateRows.PersistentTable -> {
                            applyPersistentUpdate(cache, tableName, rowUpdate.rows, operations)
                        }
                        is TableUpdateRows.EventTable -> {
                            val decoded = rowUpdate.rows.events.decodeRows()
                            for (row in decoded) {
                                operations.add(TableOperation.EventInsert(tableName, row))
                            }
                        }
                    }
                }
            }
        }
        return operations
    }

    private fun applyPersistentUpdate(
        cache: TableCache,
        tableName: String,
        rows: PersistentTableRows,
        operations: MutableList<TableOperation>,
    ) {
        val deletes = rows.deletes.decodeRows()
        val inserts = rows.inserts.decodeRows()

        val deletedSet = deletes.map { ByteArrayWrapper(it) }.toSet()
        val insertMap = mutableMapOf<ByteArrayWrapper, ByteArray>()
        for (row in inserts) {
            insertMap[ByteArrayWrapper(row)] = row
        }

        for (row in deletes) {
            val wrapper = ByteArrayWrapper(row)
            val newRow = insertMap[wrapper]
            if (newRow != null) {
                cache.deleteRow(row)
                cache.insertRow(newRow)
                operations.add(TableOperation.Update(tableName, row, newRow))
            } else {
                cache.deleteRow(row)
                operations.add(TableOperation.Delete(tableName, row))
            }
        }

        for (row in inserts) {
            val wrapper = ByteArrayWrapper(row)
            if (wrapper !in deletedSet) {
                cache.insertRow(row)
                operations.add(TableOperation.Insert(tableName, row))
            }
        }
    }
}

class TableCache(val name: String) {
    private val rows = mutableMapOf<ByteArrayWrapper, RowEntry>()

    val count: Int get() = rows.size

    fun insertRow(rowBytes: ByteArray) {
        val key = ByteArrayWrapper(rowBytes)
        val existing = rows[key]
        if (existing != null) {
            existing.refCount++
        } else {
            rows[key] = RowEntry(rowBytes, 1)
        }
    }

    fun deleteRow(rowBytes: ByteArray): Boolean {
        val key = ByteArrayWrapper(rowBytes)
        val existing = rows[key] ?: return false
        existing.refCount--
        if (existing.refCount <= 0) {
            rows.remove(key)
        }
        return true
    }

    fun allRows(): List<ByteArray> = rows.values.map { it.data }

    fun containsRow(rowBytes: ByteArray): Boolean =
        rows.containsKey(ByteArrayWrapper(rowBytes))
}

class RowEntry(val data: ByteArray, var refCount: Int)

sealed class TableOperation {
    data class Insert(val tableName: String, val row: ByteArray) : TableOperation()
    data class Delete(val tableName: String, val row: ByteArray) : TableOperation()
    data class Update(val tableName: String, val oldRow: ByteArray, val newRow: ByteArray) : TableOperation()
    data class EventInsert(val tableName: String, val row: ByteArray) : TableOperation()
}

class ByteArrayWrapper(val data: ByteArray) {
    override fun equals(other: Any?): Boolean =
        other is ByteArrayWrapper && data.contentEquals(other.data)

    override fun hashCode(): Int = data.contentHashCode()
}
