package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class IndexTest {

    // ---- UniqueIndex ----

    @Test
    fun uniqueIndexFindReturnsCorrectRow() {
        val cache = createSampleCache()
        val alice = SampleRow(1, "alice")
        val bob = SampleRow(2, "bob")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode(), bob.encode()))

        val index = UniqueIndex(cache) { it.id }
        assertEquals(alice, index.find(1))
        assertEquals(bob, index.find(2))
        assertNull(index.find(99))
    }

    @Test
    fun uniqueIndexTracksInserts() {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        assertNull(index.find(1))

        val alice = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode()))

        assertEquals(alice, index.find(1))
    }

    @Test
    fun uniqueIndexTracksDeletes() {
        val cache = createSampleCache()
        val alice = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode()))

        val index = UniqueIndex(cache) { it.id }
        assertEquals(alice, index.find(1))

        val parsed = cache.parseDeletes(buildRowList(alice.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertNull(index.find(1))
    }

    // ---- BTreeIndex ----

    @Test
    fun btreeIndexFilterReturnsAllMatching() {
        val cache = createSampleCache()
        val alice = SampleRow(1, "alice")
        val bob = SampleRow(2, "bob")
        val charlie = SampleRow(3, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode(), bob.encode(), charlie.encode()))

        val index = BTreeIndex(cache) { it.name }
        val alices = index.filter("alice").sortedBy { it.id }
        assertEquals(listOf(alice, charlie), alices)
        assertEquals(listOf(bob), index.filter("bob"))
        assertEquals(emptyList(), index.filter("nobody"))
    }

    @Test
    fun btreeIndexHandlesDuplicateKeys() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "same")
        val r2 = SampleRow(2, "same")
        val r3 = SampleRow(3, "same")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode(), r3.encode()))

        val index = BTreeIndex(cache) { it.name }
        assertEquals(3, index.filter("same").size)
    }

    @Test
    fun btreeIndexTracksInserts() {
        val cache = createSampleCache()
        val index = BTreeIndex(cache) { it.name }

        assertEquals(emptyList(), index.filter("alice"))

        val alice = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode()))

        assertEquals(listOf(alice), index.filter("alice"))
    }

    @Test
    fun btreeIndexRemovesEmptyKeyOnDelete() {
        val cache = createSampleCache()
        val alice = SampleRow(1, "alice")
        cache.applyInserts(STUB_CTX, buildRowList(alice.encode()))

        val index = BTreeIndex(cache) { it.name }
        assertEquals(listOf(alice), index.filter("alice"))

        val parsed = cache.parseDeletes(buildRowList(alice.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(emptyList(), index.filter("alice"))
    }

    @Test
    fun btreeIndexPartialDeleteKeepsRemainingRows() {
        val cache = createSampleCache()
        val r1 = SampleRow(1, "group")
        val r2 = SampleRow(2, "group")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode()))

        val index = BTreeIndex(cache) { it.name }
        assertEquals(2, index.filter("group").size)

        val parsed = cache.parseDeletes(buildRowList(r1.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        val remaining = index.filter("group")
        assertEquals(1, remaining.size)
        assertEquals(r2, remaining.single())
    }

    // ---- Null key handling ----

    @Test
    fun uniqueIndexHandlesNullKeys() {
        val cache = createSampleCache()
        val nullKeyRow = SampleRow(0, "null-key")
        val normalRow = SampleRow(1, "normal")
        cache.applyInserts(STUB_CTX, buildRowList(nullKeyRow.encode(), normalRow.encode()))

        // Key extractor returns null for id == 0
        val index = UniqueIndex<SampleRow, Int?>(cache) { if (it.id == 0) null else it.id }
        assertEquals(nullKeyRow, index.find(null))
        assertEquals(normalRow, index.find(1))
        assertNull(index.find(99))
    }

    @Test
    fun btreeIndexHandlesNullKeys() {
        val cache = createSampleCache()
        val r1 = SampleRow(0, "a")
        val r2 = SampleRow(1, "b")
        val r3 = SampleRow(2, "c")
        cache.applyInserts(STUB_CTX, buildRowList(r1.encode(), r2.encode(), r3.encode()))

        // Key extractor returns null for id == 0
        val index = BTreeIndex<SampleRow, Int?>(cache) { if (it.id == 0) null else it.id }
        assertEquals(listOf(r1), index.filter(null))
        assertEquals(listOf(r2), index.filter(1))
        assertEquals(emptyList(), index.filter(99))
    }
}
