package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
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

    // ---- Content-based keying extended coverage ----

    @Test
    fun contentKeyInsertMultipleDistinctRows() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        val r3 = SampleRow(1, "charlie") // same id, different name = different content key
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode(), r3.encode()))
        assertEquals(3, cache.count())
        val all = cache.all().sortedBy { it.name }
        assertEquals(listOf(r1, r2, r3), all)
    }

    @Test
    fun contentKeyDuplicateInsertIncrementsRefCount() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        // Same content = same key, refcount bumped but only 1 logical row
        assertEquals(1, cache.count())

        // First delete decrements refcount but row survives
        val parsed1 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed1)
        assertEquals(1, cache.count())

        // Second delete removes the row
        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(0, cache.count())
    }

    @Test
    fun contentKeyDeleteMatchesByBytesNotFieldValues() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        // Different content (same id but different name) should NOT delete the original
        val differentContent = SampleRow(1, "bob")
        val parsed = cache.parseDeletes(buildRowList(differentContent.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(1, cache.count(), "Delete with different content should not affect existing row")

        // Delete with exact same content works
        val exactMatch = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, exactMatch)
        assertEquals(0, cache.count())
    }

    @Test
    fun contentKeyOnInsertCallbackFires() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val row = SampleRow(1, "alice")
        val callbacks = cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        callbacks.forEach { it.invoke() }

        assertEquals(listOf(row), inserted)
    }

    @Test
    fun contentKeyOnInsertDoesNotFireForDuplicateContent() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, r -> inserted.add(r) }

        // Same content again — refcount bump only, no callback
        val callbacks = cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        callbacks.forEach { it.invoke() }
        assertTrue(inserted.isEmpty())
    }

    @Test
    fun contentKeyOnDeleteCallbackFires() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
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
    fun contentKeyOnDeleteDoesNotFireWhenRefCountStillPositive() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        // Insert twice — refcount = 2
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.onDelete { _, r -> deleted.add(r) }

        // First delete decrements refcount but doesn't remove
        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }
        assertTrue(deleted.isEmpty(), "onDelete should not fire when refcount > 0")
        assertEquals(1, cache.count())
    }

    @Test
    fun contentKeyOnBeforeDeleteFires() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeletes = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeletes.add(r) }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.preApplyDeletes(STUB_CTX, parsed)

        assertEquals(listOf(row), beforeDeletes)
    }

    @Test
    fun contentKeyOnBeforeDeleteSkipsWhenRefCountHigh() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeletes = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeletes.add(r) }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.preApplyDeletes(STUB_CTX, parsed)

        assertTrue(beforeDeletes.isEmpty(), "onBeforeDelete should not fire when refcount > 1")
    }

    @Test
    fun contentKeyTwoPhaseDeleteOrder() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val events = mutableListOf<String>()
        cache.onBeforeDelete { _, _ -> events.add("before") }
        cache.onDelete { _, _ -> events.add("delete") }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.preApplyDeletes(STUB_CTX, parsed)
        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(listOf("before", "delete"), events)
    }

    @Test
    fun contentKeyUpdateAlwaysDecomposesIntoDeleteAndInsert() {
        // For content-key tables, old and new content have different bytes = different keys.
        // So a PersistentTable update with delete(old) + insert(new) is never merged into onUpdate.
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val oldRow = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val updates = mutableListOf<Pair<SampleRow, SampleRow>>()
        val inserts = mutableListOf<SampleRow>()
        val deletes = mutableListOf<SampleRow>()
        cache.onUpdate { _, old, new -> updates.add(old to new) }
        cache.onInsert { _, row -> inserts.add(row) }
        cache.onDelete { _, row -> deletes.add(row) }

        val newRow = SampleRow(1, "alice_updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        // onUpdate never fires — different content = different keys
        assertTrue(updates.isEmpty(), "onUpdate should never fire for content-key tables with different content")
        assertEquals(listOf(newRow), inserts)
        assertEquals(listOf(oldRow), deletes)
        assertEquals(1, cache.count())
    }

    @Test
    fun contentKeySameContentDeleteAndInsertMergesIntoUpdate() {
        // Edge case: if delete and insert have IDENTICAL content (same bytes),
        // they share the same content key and ARE merged into an onUpdate.
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val updates = mutableListOf<Pair<SampleRow, SampleRow>>()
        val inserts = mutableListOf<SampleRow>()
        val deletes = mutableListOf<SampleRow>()
        cache.onUpdate { _, old, new -> updates.add(old to new) }
        cache.onInsert { _, r -> inserts.add(r) }
        cache.onDelete { _, r -> deletes.add(r) }

        // Delete and insert exact same content
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(row.encode()),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        // Same content key in both sides → treated as update
        assertEquals(1, updates.size)
        assertEquals(row, updates[0].first)
        assertEquals(row, updates[0].second)
        assertTrue(inserts.isEmpty())
        assertTrue(deletes.isEmpty())
        assertEquals(1, cache.count())
    }

    @Test
    fun contentKeyPreApplyUpdateFiresBeforeDeleteForPureDeletes() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row1 = SampleRow(1, "alice")
        val row2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(row1.encode(), row2.encode()))

        val beforeDeletes = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeletes.add(r) }

        // Pure delete of row1 (no matching insert)
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row1.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)

        assertEquals(listOf(row1), beforeDeletes)
    }

    @Test
    fun contentKeyPreApplyUpdateSkipsDeletesThatAreUpdates() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeletes = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeletes.add(r) }

        // Same content in both delete and insert = update, not pure delete
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(row.encode()),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)

        assertTrue(beforeDeletes.isEmpty(), "onBeforeDelete should not fire for updates")
    }

    @Test
    fun contentKeyInternalListenersFireCorrectly() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val internalInserts = mutableListOf<SampleRow>()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(listOf(row), internalInserts)
        assertTrue(internalDeletes.isEmpty())

        internalInserts.clear()
        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(listOf(row), internalDeletes)
        assertTrue(internalInserts.isEmpty())
    }

    @Test
    fun contentKeyInternalListenersDoNotFireForRefCountBump() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val internalInserts = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }

        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, internalInserts.size)

        // Same content again — refcount bump, no internal listener
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, internalInserts.size, "Internal listener should not fire for refcount bump")
    }

    @Test
    fun contentKeyIterAndAll() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        val r3 = SampleRow(1, "charlie") // same id as r1 but different content key
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode(), r3.encode()))

        val allRows = cache.all().sortedBy { it.name }
        assertEquals(listOf(r1, r2, r3), allRows)

        val iterRows = cache.iter().sortedBy { it.name }.toList()
        assertEquals(listOf(r1, r2, r3), iterRows)
    }

    @Test
    fun contentKeyClearRemovesAllAndFiresInternalListeners() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { deleted.add(it) }

        cache.clear()
        assertEquals(0, cache.count())
        assertTrue(cache.all().isEmpty())
        assertEquals(2, deleted.size)
        assertTrue(deleted.containsAll(listOf(r1, r2)))
    }

    @Test
    fun contentKeyIndexesWorkWithContentKeyCache() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val uniqueById = UniqueIndex(cache) { it.id }
        val btreeByName = BTreeIndex(cache) { it.name }

        val r1 = SampleRow(1, "alice")
        val r2 = SampleRow(2, "bob")
        val r3 = SampleRow(3, "alice") // same name, different id
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode(), r3.encode()))

        assertEquals(r1, uniqueById.find(1))
        assertEquals(r2, uniqueById.find(2))
        assertEquals(r3, uniqueById.find(3))
        assertEquals(2, btreeByName.filter("alice").size)
        assertEquals(1, btreeByName.filter("bob").size)

        // Delete r1 — index updates
        val parsed = cache.parseDeletes(buildRowList(r1.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertNull(uniqueById.find(1))
        assertEquals(1, btreeByName.filter("alice").size)
    }

    @Test
    fun contentKeyMixedUpdateWithPureDeleteAndPureInsert() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val existing1 = SampleRow(1, "alice")
        val existing2 = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(existing1.encode(), existing2.encode()))

        val inserts = mutableListOf<SampleRow>()
        val deletes = mutableListOf<SampleRow>()
        val updates = mutableListOf<Pair<SampleRow, SampleRow>>()
        cache.onInsert { _, r -> inserts.add(r) }
        cache.onDelete { _, r -> deletes.add(r) }
        cache.onUpdate { _, old, new -> updates.add(old to new) }

        // Delete existing1, insert new row — these have different content keys
        val newRow = SampleRow(3, "charlie")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(existing1.encode()),
        )
        val parsed = cache.parseUpdate(update)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertEquals(listOf(newRow), inserts)
        assertEquals(listOf(existing1), deletes)
        assertTrue(updates.isEmpty())
        assertEquals(2, cache.count()) // existing2 + newRow
    }

    @Test
    fun contentKeyDeleteOfNonExistentContentIsNoOp() {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val deleted = mutableListOf<SampleRow>()
        cache.onDelete { _, r -> deleted.add(r) }

        // Try to delete content that doesn't exist
        val nonExistent = SampleRow(99, "nobody")
        val parsed = cache.parseDeletes(buildRowList(nonExistent.encode()))
        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
        callbacks.forEach { it.invoke() }

        assertTrue(deleted.isEmpty())
        assertEquals(1, cache.count())
    }

    @Test
    fun contentKeyRefCountWithCallbackLifecycle() {
        // Full lifecycle: insert x3 (same content), delete x3, verify callback timing
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val row = SampleRow(1, "alice")

        val inserts = mutableListOf<Int>()
        val deletes = mutableListOf<Int>()
        cache.onInsert { _, _ -> inserts.add(cache.count()) }
        cache.onDelete { _, _ -> deletes.add(cache.count()) }

        // First insert → callback fires (count=1 after insert)
        cache.applyInserts(STUB_CTX, buildRowList(row.encode())).forEach { it.invoke() }
        assertEquals(listOf(1), inserts)

        // Second insert → no callback (refcount bump)
        cache.applyInserts(STUB_CTX, buildRowList(row.encode())).forEach { it.invoke() }
        assertEquals(listOf(1), inserts, "No callback on second insert")

        // Third insert → no callback
        cache.applyInserts(STUB_CTX, buildRowList(row.encode())).forEach { it.invoke() }
        assertEquals(listOf(1), inserts, "No callback on third insert")

        // First delete → no callback (refcount 3→2)
        val p1 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, p1).forEach { it.invoke() }
        assertTrue(deletes.isEmpty(), "No delete callback while refcount > 0")

        // Second delete → no callback (refcount 2→1)
        val p2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, p2).forEach { it.invoke() }
        assertTrue(deletes.isEmpty(), "No delete callback while refcount > 0")

        // Third delete → callback fires (refcount 1→0, removed)
        val p3 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, p3).forEach { it.invoke() }
        assertEquals(1, deletes.size, "Delete callback fires when row removed")
        assertEquals(0, cache.count())
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
