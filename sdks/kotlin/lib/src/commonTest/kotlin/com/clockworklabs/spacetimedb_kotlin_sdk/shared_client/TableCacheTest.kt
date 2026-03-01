package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class TableCacheTest {

    @Test
    fun insertAddsRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
    }

    @Test
    fun insertMultipleRows() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))
        assertEquals(2, cache.count())
        val all = cache.all().sortedBy { it.id }
        assertEquals(listOf(r1, r2), all)
    }

    @Test
    fun insertDuplicateKeyIncrementsRefCount() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())

        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())
    }

    @Test
    fun deleteRemovesRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(0, cache.count())
    }

    @Test
    fun deleteDecrementsRefCount() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(1, cache.count())

        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(0, cache.count())
    }

    @Test
    fun updateReplacesRow() {
        val cache = createSampleCache()
        val oldRow = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val newRow = SampleRow(1, "alice_updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(newRow, cache.all().single())
    }

    @Test
    fun updateFiresInternalListeners() {
        val cache = createSampleCache()
        val oldRow = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val inserts = mutableListOf<SampleRow>()
        val deletes = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { inserts.add(it) }
        cache.addInternalDeleteListener { deletes.add(it) }

        val newRow = SampleRow(1, "alice_updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(listOf(oldRow), deletes)
        assertEquals(listOf(newRow), inserts)
    }

    @Test
    fun eventTableDoesNotStoreRows() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        val event = TableUpdateRows.EventTable(
            events = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(0, cache.count())
    }

    @Test
    fun clearEmptiesAllRows() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))
        assertEquals(2, cache.count())

        cache.clear()
        assertEquals(0, cache.count())
        assertTrue(cache.all().isEmpty())
    }

    @Test
    fun clearFiresInternalDeleteListeners() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { deleted.add(it) }

        cache.clear()
        assertEquals(2, deleted.size)
        assertTrue(deleted.containsAll(listOf(r1, r2)))
    }

    @Test
    fun iterReturnsAllRows() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))

        val iterated = cache.iter().asSequence().sortedBy { it.id }.toList()
        assertEquals(listOf(r1, r2), iterated)
    }

    @Test
    fun internalInsertListenerFiresOnInsert() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { inserted.add(it) }

        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        assertEquals(listOf(row), inserted)
    }

    @Test
    fun internalDeleteListenerFiresOnDelete() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { deleted.add(it) }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(listOf(row), deleted)
    }

    @Test
    fun pureDeleteViaUpdateRemovesRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(0, cache.count())
    }

    @Test
    fun pureInsertViaUpdateAddsRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")

        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(row.encode()),
            deletes = buildRowList(),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
    }

    @Test
    fun contentKeyTableWorks() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())
    }

    // ---- Public callback tests ----

    @Test
    fun onInsertCallbackFires() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val row = SampleRow(1, "alice")
        val callbacks = cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        callbacks.forEach { it.invoke() }

        assertEquals(listOf(row), inserted)
    }

    @Test
    fun onInsertCallbackDoesNotFireForDuplicate() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, r -> inserted.add(r) }

        // Insert same key again — should NOT fire onInsert (ref count bump only)
        val callbacks = cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        callbacks.forEach { it.invoke() }

        assertTrue(inserted.isEmpty())
    }

    @Test
    fun onDeleteCallbackFires() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.onDelete { _, r -> deleted.add(r) }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(listOf(row), deleted)
    }

    @Test
    fun onUpdateCallbackFires() {
        val cache = createSampleCache()
        val oldRow = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val updates = mutableListOf<Pair<SampleRow, SampleRow>>()
        cache.onUpdate { _, old, new -> updates.add(old to new) }

        val newRow = SampleRow(1, "alice_updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(1, updates.size)
        assertEquals(oldRow, updates[0].first)
        assertEquals(newRow, updates[0].second)
    }

    @Test
    fun onBeforeDeleteFires() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeletes = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeletes.add(r) }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.preApplyDeletes(STUB_CTX, parsed)

        assertEquals(listOf(row), beforeDeletes)
    }

    @Test
    fun preApplyThenApplyDeletesOrderCorrect() {
        val cache = createSampleCache()
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val events = mutableListOf<String>()
        cache.onBeforeDelete { _, _ -> events.add("before") }
        cache.onDelete { _, _ -> events.add("delete") }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.preApplyDeletes(STUB_CTX, parsed) // before fires here
        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() } // delete fires here

        assertEquals(listOf("before", "delete"), events)
    }

    @Test
    fun removeOnInsertStopsCallback() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        val cb: (EventContext, SampleRow) -> Unit = { _, row -> inserted.add(row) }
        cache.onInsert(cb)

        val r1 = SampleRow(1, "alice")
        val callbacks1 = cache.applyInserts(STUB_CTX, buildRowList(r1.encode()))
        callbacks1.forEach { it.invoke() }
        assertEquals(1, inserted.size)

        cache.removeOnInsert(cb)

        val r2 = SampleRow(2, "bob")
        val callbacks2 = cache.applyInserts(STUB_CTX, buildRowList(r2.encode()))
        callbacks2.forEach { it.invoke() }
        assertEquals(1, inserted.size) // no new insert
    }

    @Test
    fun eventTableFiresInsertCallbacks() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val row = SampleRow(1, "event_row")
        val event = TableUpdateRows.EventTable(
            events = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(event)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        // Event table rows fire callbacks but don't persist
        assertEquals(1, inserted.size)
        assertEquals(0, cache.count())
    }
}
