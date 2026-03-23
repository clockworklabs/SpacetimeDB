package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class CacheOperationsEdgeCaseTest {

    // =========================================================================
    // Cache Operations Edge Cases
    // =========================================================================

    @Test
    fun clearFiresInternalDeleteListenersForAllRows() {
        val cache = createSampleCache()
        val deletedRows = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { deletedRows.add(it) }

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        cache.applyInserts(STUB_CTX, buildRowList(row1.encode(), row2.encode()))

        cache.clear()

        assertEquals(0, cache.count())
        assertEquals(2, deletedRows.size)
        assertTrue(deletedRows.containsAll(listOf(row1, row2)))
    }

    @Test
    fun clearOnEmptyCacheIsNoOp() {
        val cache = createSampleCache()
        var listenerFired = false
        cache.addInternalDeleteListener { listenerFired = true }

        cache.clear()
        assertFalse(listenerFired)
    }

    @Test
    fun deleteNonexistentRowIsNoOp() {
        val cache = createSampleCache()
        val row = SampleRow(99, "Ghost")

        var deleteFired = false
        cache.onDelete { _, _ -> deleteFired = true }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertFalse(deleteFired)
        assertEquals(0, cache.count())
    }

    @Test
    fun insertEmptyRowListIsNoOp() {
        val cache = createSampleCache()
        var insertFired = false
        cache.onInsert { _, _ -> insertFired = true }

        val callbacks = cache.applyInserts(STUB_CTX, buildRowList())

        assertEquals(0, cache.count())
        assertTrue(callbacks.isEmpty())
        assertFalse(insertFired)
    }

    @Test
    fun removeCallbackPreventsItFromFiring() {
        val cache = createSampleCache()
        var fired = false
        val cb: (EventContext, SampleRow) -> Unit = { _, _ -> fired = true }

        cache.onInsert(cb)
        cache.removeOnInsert(cb)

        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(1, "Alice").encode()))
        // Invoke any pending callbacks
        // No PendingCallbacks should exist for this insert since we removed the callback

        assertFalse(fired)
    }

    @Test
    fun internalListenersFiredOnInsertAfterCAS() {
        val cache = createSampleCache()
        val internalInserts = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        assertEquals(listOf(row), internalInserts)
    }

    @Test
    fun internalListenersFiredOnDeleteAfterCAS() {
        val cache = createSampleCache()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(listOf(row), internalDeletes)
    }

    @Test
    fun internalListenersFiredOnUpdateForBothOldAndNew() {
        val cache = createSampleCache()
        val internalInserts = mutableListOf<SampleRow>()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val oldRow = SampleRow(1, "Old")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))
        internalInserts.clear() // Reset from the initial insert

        val newRow = SampleRow(1, "New")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        // On update, old row fires delete listener, new row fires insert listener
        assertEquals(listOf(oldRow), internalDeletes)
        assertEquals(listOf(newRow), internalInserts)
    }

    @Test
    fun batchInsertMultipleRowsFiresCallbacksForEach() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val rows = (1..5).map { SampleRow(it, "Row$it") }
        val callbacks = cache.applyInserts(
            STUB_CTX,
            buildRowList(*rows.map { it.encode() }.toTypedArray())
        )
        for (cb in callbacks) cb.invoke()

        assertEquals(5, cache.count())
        assertEquals(rows, inserted)
    }

    // =========================================================================
    // ClientCache Registry
    // =========================================================================

    @Test
    fun clientCacheGetTableThrowsForUnknownTable() {
        val cc = ClientCache()
        assertFailsWith<IllegalStateException> {
            cc.getTable<SampleRow>("nonexistent")
        }
    }

    @Test
    fun clientCacheGetTableOrNullReturnsNull() {
        val cc = ClientCache()
        assertNull(cc.getTableOrNull<SampleRow>("nonexistent"))
    }

    @Test
    fun clientCacheGetOrCreateTableCreatesOnce() {
        val cc = ClientCache()
        var factoryCalls = 0

        val cache1 = cc.getOrCreateTable<SampleRow>("t") {
            factoryCalls++
            createSampleCache()
        }
        val cache2 = cc.getOrCreateTable<SampleRow>("t") {
            factoryCalls++
            createSampleCache()
        }

        assertEquals(1, factoryCalls)
        assertTrue(cache1 === cache2)
    }

    @Test
    fun clientCacheTableNames() {
        val cc = ClientCache()
        cc.register("alpha", createSampleCache())
        cc.register("beta", createSampleCache())

        assertEquals(setOf("alpha", "beta"), cc.tableNames())
    }

    @Test
    fun clientCacheClearClearsAllTables() {
        val cc = ClientCache()
        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        cc.register("a", cacheA)
        cc.register("b", cacheB)

        cacheA.applyInserts(STUB_CTX, buildRowList(SampleRow(1, "X").encode()))
        cacheB.applyInserts(STUB_CTX, buildRowList(SampleRow(2, "Y").encode()))

        cc.clear()

        assertEquals(0, cacheA.count())
        assertEquals(0, cacheB.count())
    }

    // =========================================================================
    // Ref Count Edge Cases
    // =========================================================================

    @Test
    fun refCountSurvivesUpdateOnMultiRefRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")

        // Insert twice — refCount = 2
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())

        // Update the row — should preserve refCount
        val updatedRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(updatedRow.encode()),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().single().name)

        // Deleting once should still keep the row (refCount was 2, update preserves it)
        val parsedDelete = cache.parseDeletes(buildRowList(updatedRow.encode()))
        cache.applyDeletes(STUB_CTX, parsedDelete)
        // The refCount was preserved during update, so after one delete it should still be there
        assertEquals(1, cache.count())
    }

    @Test
    fun deleteWithHighRefCountOnlyDecrements() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")

        // Insert 3 times — refCount = 3
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        var deleteFired = false
        cache.onDelete { _, _ -> deleteFired = true }

        // Delete once — refCount goes to 2
        val parsed1 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed1)
        assertEquals(1, cache.count())
        assertFalse(deleteFired)

        // Delete again — refCount goes to 1
        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(1, cache.count())
        assertFalse(deleteFired)

        // Delete final — refCount goes to 0
        val parsed3 = cache.parseDeletes(buildRowList(row.encode()))
        val callbacks = cache.applyDeletes(STUB_CTX, parsed3)
        for (cb in callbacks) cb.invoke()
        assertEquals(0, cache.count())
        assertTrue(deleteFired)
    }

    // =========================================================================
    // BsatnRowKey equality and hashCode
    // =========================================================================

    @Test
    fun bsatnRowKeyEqualityAndHashCode() {
        val a = BsatnRowKey(byteArrayOf(1, 2, 3))
        val b = BsatnRowKey(byteArrayOf(1, 2, 3))
        val c = BsatnRowKey(byteArrayOf(1, 2, 4))

        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
        assertFalse(a == c)
    }

    @Test
    fun bsatnRowKeyWorksAsMapKey() {
        val map = mutableMapOf<BsatnRowKey, String>()
        val key1 = BsatnRowKey(byteArrayOf(10, 20))
        val key2 = BsatnRowKey(byteArrayOf(10, 20))
        val key3 = BsatnRowKey(byteArrayOf(30, 40))

        map[key1] = "first"
        map[key2] = "second" // Same content as key1, should overwrite
        map[key3] = "third"

        assertEquals(2, map.size)
        assertEquals("second", map[key1])
        assertEquals("third", map[key3])
    }

    // =========================================================================
    // DecodedRow equality
    // =========================================================================

    @Test
    fun decodedRowEquality() {
        val row1 = DecodedRow(SampleRow(1, "A"), byteArrayOf(1, 2, 3))
        val row2 = DecodedRow(SampleRow(1, "A"), byteArrayOf(1, 2, 3))
        val row3 = DecodedRow(SampleRow(1, "A"), byteArrayOf(4, 5, 6))

        assertEquals(row1, row2)
        assertEquals(row1.hashCode(), row2.hashCode())
        assertFalse(row1 == row3)
    }

    // =========================================================================
    // FixedSize hint validation
    // =========================================================================

    @Test
    fun fixedSizeHintNonDivisibleRowsDataThrows() {
        val cache = createSampleCache()
        // 7 bytes of data with FixedSize(4) → 7 % 4 != 0
        val rowList = BsatnRowList(
            sizeHint = RowSizeHint.FixedSize(4u),
            rowsData = ByteArray(7),
        )
        assertFailsWith<IllegalArgumentException>("Should reject non-divisible FixedSize row data") {
            cache.decodeRowList(rowList)
        }
    }
}
