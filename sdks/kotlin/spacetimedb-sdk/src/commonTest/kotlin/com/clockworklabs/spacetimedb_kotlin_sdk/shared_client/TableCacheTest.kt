package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
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

    // ---- Event table extended coverage ----

    @Test
    fun eventTableBatchMultipleRows() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val rows = (1..10).map { SampleRow(it, "evt-$it") }
        val event = TableUpdateRows.EventTable(
            events = buildRowList(*rows.map { it.encode() }.toTypedArray()),
        )
        val parsed = cache.parseUpdate(event)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(10, inserted.size)
        assertEquals(rows, inserted)
        assertEquals(0, cache.count())
    }

    @Test
    fun eventTableOnDeleteCallbackNeverFires() {
        val cache = createSampleCache()
        var deleteFired = false
        cache.onDelete { _, _ -> deleteFired = true }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "evt").encode()),
        )
        val parsed = cache.parseUpdate(event)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertFalse(deleteFired, "onDelete should never fire for event tables")
    }

    @Test
    fun eventTableOnUpdateCallbackNeverFires() {
        val cache = createSampleCache()
        var updateFired = false
        cache.onUpdate { _, _, _ -> updateFired = true }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "evt").encode()),
        )
        val parsed = cache.parseUpdate(event)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertFalse(updateFired, "onUpdate should never fire for event tables")
    }

    @Test
    fun eventTableOnBeforeDeleteNeverFires() {
        val cache = createSampleCache()
        var beforeDeleteFired = false
        cache.onBeforeDelete { _, _ -> beforeDeleteFired = true }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "evt").encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.preApplyUpdate(STUB_CTX, parsed)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertFalse(beforeDeleteFired, "onBeforeDelete should never fire for event tables")
    }

    @Test
    fun eventTableRemoveOnInsertStopsCallback() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        val cb: (EventContext, SampleRow) -> Unit = { _, row -> inserted.add(row) }
        cache.onInsert(cb)

        // First event fires callback
        val event1 = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "first").encode()),
        )
        val parsed1 = cache.parseUpdate(event1)
        cache.applyUpdate(STUB_CTX, parsed1).forEach { it.invoke() }
        assertEquals(1, inserted.size)

        // Remove callback, second event should NOT fire it
        cache.removeOnInsert(cb)
        val event2 = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(2, "second").encode()),
        )
        val parsed2 = cache.parseUpdate(event2)
        cache.applyUpdate(STUB_CTX, parsed2).forEach { it.invoke() }
        assertEquals(1, inserted.size, "Callback should not fire after removeOnInsert")
    }

    @Test
    fun eventTableSequentialUpdatesNeverAccumulate() {
        val cache = createSampleCache()
        val allInserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> allInserted.add(row) }

        // Send 5 sequential event updates
        for (batch in 0 until 5) {
            val rows = (1..3).map { SampleRow(batch * 3 + it, "b$batch-$it") }
            val event = TableUpdateRows.EventTable(
                events = buildRowList(*rows.map { it.encode() }.toTypedArray()),
            )
            val parsed = cache.parseUpdate(event)
            cache.applyUpdate(STUB_CTX, parsed).forEach { it.invoke() }

            // Cache must remain empty after every batch
            assertEquals(0, cache.count(), "Cache should stay empty after event batch $batch")
        }

        // All 15 callbacks should have fired
        assertEquals(15, allInserted.size)
    }

    @Test
    fun eventTableDoesNotAffectInternalListeners() {
        val cache = createSampleCache()
        val internalInserts = mutableListOf<SampleRow>()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "evt").encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed)

        // Internal listeners should NOT fire for event tables
        assertTrue(internalInserts.isEmpty(), "Internal insert listener should not fire for event tables")
        assertTrue(internalDeletes.isEmpty(), "Internal delete listener should not fire for event tables")
    }

    @Test
    fun eventTableIndexesStayEmpty() {
        val cache = createSampleCache()
        val uniqueIndex = UniqueIndex(cache) { it.id }
        val btreeIndex = BTreeIndex(cache) { it.name }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(
                SampleRow(1, "evt-a").encode(),
                SampleRow(2, "evt-b").encode(),
                SampleRow(3, "evt-a").encode(),
            ),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed)

        // Indexes should remain empty since internal listeners don't fire
        assertEquals(null, uniqueIndex.find(1))
        assertEquals(null, uniqueIndex.find(2))
        assertTrue(btreeIndex.filter("evt-a").isEmpty())
        assertTrue(btreeIndex.filter("evt-b").isEmpty())
    }

    @Test
    fun eventTableDuplicateRowsBothFireCallbacks() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        // Same row data sent twice — both should fire callbacks (no deduplication)
        val row = SampleRow(1, "dup")
        val event = TableUpdateRows.EventTable(
            events = buildRowList(row.encode(), row.encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed).forEach { it.invoke() }

        assertEquals(2, inserted.size, "Both duplicate event rows should fire callbacks")
        assertEquals(row, inserted[0])
        assertEquals(row, inserted[1])
        assertEquals(0, cache.count())
    }

    @Test
    fun eventTableAfterPersistentInsertDoesNotAffectCachedRows() {
        val cache = createSampleCache()

        // Persistent insert
        val persistentRow = SampleRow(1, "persistent")
        cache.applyInserts(STUB_CTX, buildRowList(persistentRow.encode()))
        assertEquals(1, cache.count())

        // Event with same primary key — should NOT affect the cached row
        val event = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "event-version").encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(persistentRow, cache.all().single(), "Persistent row should be untouched by event table update")
    }

    @Test
    fun eventTableEmptyEventsProducesNoCallbacks() {
        val cache = createSampleCache()
        var callbackCount = 0
        cache.onInsert { _, _ -> callbackCount++ }

        val event = TableUpdateRows.EventTable(
            events = buildRowList(), // empty
        )
        val parsed = cache.parseUpdate(event)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(0, callbackCount, "Empty event table should produce no callbacks")
        assertEquals(0, cache.count())
    }

    @Test
    fun eventTableMultipleCallbacksAllFire() {
        val cache = createSampleCache()
        val cb1 = mutableListOf<SampleRow>()
        val cb2 = mutableListOf<SampleRow>()
        val cb3 = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> cb1.add(row) }
        cache.onInsert { _, row -> cb2.add(row) }
        cache.onInsert { _, row -> cb3.add(row) }

        val row = SampleRow(1, "evt")
        val event = TableUpdateRows.EventTable(
            events = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(event)
        cache.applyUpdate(STUB_CTX, parsed).forEach { it.invoke() }

        assertEquals(listOf(row), cb1)
        assertEquals(listOf(row), cb2)
        assertEquals(listOf(row), cb3)
    }
}
