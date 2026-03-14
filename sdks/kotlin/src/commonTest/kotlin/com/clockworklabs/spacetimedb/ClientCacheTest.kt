package com.clockworklabs.spacetimedb

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ClientCacheTest {

    @Test
    fun insertAndCount() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(1, 2, 3))
        cache.insertRow(byteArrayOf(4, 5, 6))
        assertEquals(2, cache.count)
    }

    @Test
    fun duplicateInsertIncrementsRefCount() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(1, 2, 3))
        cache.insertRow(byteArrayOf(1, 2, 3))
        assertEquals(1, cache.count)
    }

    @Test
    fun deleteRemovesRow() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(1, 2, 3))
        assertTrue(cache.deleteRow(byteArrayOf(1, 2, 3)))
        assertEquals(0, cache.count)
    }

    @Test
    fun deleteNonexistentReturnsFalse() {
        val cache = TableCache("users")
        assertFalse(cache.deleteRow(byteArrayOf(1, 2, 3)))
    }

    @Test
    fun refCountedDelete() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(1, 2, 3))
        cache.insertRow(byteArrayOf(1, 2, 3))
        cache.deleteRow(byteArrayOf(1, 2, 3))
        assertEquals(1, cache.count)
        cache.deleteRow(byteArrayOf(1, 2, 3))
        assertEquals(0, cache.count)
    }

    @Test
    fun containsRow() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(10, 20))
        assertTrue(cache.containsRow(byteArrayOf(10, 20)))
        assertFalse(cache.containsRow(byteArrayOf(30, 40)))
    }

    @Test
    fun allRows() {
        val cache = TableCache("users")
        cache.insertRow(byteArrayOf(1))
        cache.insertRow(byteArrayOf(2))
        cache.insertRow(byteArrayOf(3))
        assertEquals(3, cache.allRows().size)
    }

    @Test
    fun clientCacheGetOrCreate() {
        val cc = ClientCache()
        val t1 = cc.getOrCreateTable("users")
        val t2 = cc.getOrCreateTable("users")
        assertTrue(t1 === t2)
    }

    @Test
    fun clientCacheTableNames() {
        val cc = ClientCache()
        cc.getOrCreateTable("users")
        cc.getOrCreateTable("messages")
        assertEquals(setOf("users", "messages"), cc.tableNames())
    }

    @Test
    fun tableHandleCallbacks() {
        val handle = TableHandle("users")
        var inserted: ByteArray? = null
        var deleted: ByteArray? = null
        var updatedOld: ByteArray? = null
        var updatedNew: ByteArray? = null

        handle.onInsert { row -> inserted = row }
        handle.onDelete { row -> deleted = row }
        handle.onUpdate { old, new -> updatedOld = old; updatedNew = new }

        handle.fireInsert(byteArrayOf(1, 2, 3))
        assertTrue(byteArrayOf(1, 2, 3).contentEquals(inserted!!))

        handle.fireDelete(byteArrayOf(4, 5, 6))
        assertTrue(byteArrayOf(4, 5, 6).contentEquals(deleted!!))

        handle.fireUpdate(byteArrayOf(1), byteArrayOf(2))
        assertTrue(byteArrayOf(1).contentEquals(updatedOld!!))
        assertTrue(byteArrayOf(2).contentEquals(updatedNew!!))
    }
}
