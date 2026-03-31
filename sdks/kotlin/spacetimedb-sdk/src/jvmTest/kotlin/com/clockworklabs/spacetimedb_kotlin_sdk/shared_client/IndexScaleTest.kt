package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import java.util.concurrent.CyclicBarrier
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlin.time.measureTime

/**
 * Large-scale performance tests for UniqueIndex and BTreeIndex.
 * These verify correctness and measure performance characteristics
 * at row counts well beyond the functional test suite (which uses 2-8K rows).
 *
 * Run on JVM only — uses real threads for concurrent workloads and
 * timing measurements via kotlin.time.
 */
class IndexScaleTest {

    companion object {
        private const val SMALL = 1_000
        private const val MEDIUM = 10_000
        private const val LARGE = 50_000
    }

    // ---- UniqueIndex: large-scale population via incremental inserts ----

    @Test
    fun `unique index incremental insert10 k`() {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }

        // Every row must be findable
        for (i in 0 until MEDIUM) {
            val found = index.find(i)
            assertNotNull(found, "Missing row id=$i in UniqueIndex after 10K inserts")
            assertEquals(i, found.id)
        }
        assertEquals(MEDIUM, cache.count())
    }

    @Test
    fun `unique index incremental insert50 k`() {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        measureTime {
            for (i in 0 until LARGE) {
                cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
            }
        }

        // Spot-check lookups across the range
        for (i in listOf(0, LARGE / 4, LARGE / 2, LARGE - 1)) {
            val found = index.find(i)
            assertNotNull(found, "Missing row id=$i in UniqueIndex after 50K inserts")
            assertEquals(i, found.id)
        }
        assertEquals(LARGE, cache.count())

        // Measure lookup time over all rows
        val lookupTime = measureTime {
            for (i in 0 until LARGE) {
                index.find(i)
            }
        }

        // Sanity: 50K lookups should complete in well under 5 seconds
        assertTrue(lookupTime.inWholeMilliseconds < 5000,
            "50K UniqueIndex lookups took ${lookupTime.inWholeMilliseconds}ms — too slow")
    }

    // ---- UniqueIndex: construction from pre-populated cache ----

    @Test
    fun `unique index construction from pre populated cache10 k`() {
        val cache = createSampleCache()
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }

        // Time how long index construction takes from a full cache
        val constructionTime = measureTime {
            val index = UniqueIndex(cache) { it.id }
            // Verify all rows indexed
            assertEquals(SampleRow(0, "row-0"), index.find(0))
            assertEquals(SampleRow(MEDIUM - 1, "row-${MEDIUM - 1}"), index.find(MEDIUM - 1))
        }

        assertTrue(constructionTime.inWholeMilliseconds < 5000,
            "UniqueIndex construction from 10K rows took ${constructionTime.inWholeMilliseconds}ms — too slow")
    }

    @Test
    fun `unique index construction from pre populated cache50 k`() {
        val cache = createSampleCache()
        for (i in 0 until LARGE) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }

        val constructionTime = measureTime {
            val index = UniqueIndex(cache) { it.id }
            assertEquals(SampleRow(LARGE - 1, "row-${LARGE - 1}"), index.find(LARGE - 1))
        }

        assertTrue(constructionTime.inWholeMilliseconds < 10000,
            "UniqueIndex construction from 50K rows took ${constructionTime.inWholeMilliseconds}ms — too slow")
    }

    // ---- BTreeIndex: high cardinality (many unique keys) ----

    @Test
    fun `btree index high cardinality10 k`() {
        val cache = createSampleCache()
        // Each row has a unique name — 10K unique keys, 1 row per key
        val index = BTreeIndex(cache) { it.name }

        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "unique-$i").encode()))
        }

        // Every key should return exactly 1 row
        for (i in 0 until MEDIUM) {
            val results = index.filter("unique-$i")
            assertEquals(1, results.size, "Expected 1 row for key unique-$i, got ${results.size}")
        }
    }

    // ---- BTreeIndex: low cardinality (few keys, many rows per key) ----

    @Test
    fun `btree index low cardinality10 k`() {
        val cache = createSampleCache()
        val groupCount = 10
        val index = BTreeIndex(cache) { it.name }

        for (i in 0 until MEDIUM) {
            val group = "group-${i % groupCount}"
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, group).encode()))
        }

        // Each group should have MEDIUM / groupCount rows
        val expectedPerGroup = MEDIUM / groupCount
        for (g in 0 until groupCount) {
            val results = index.filter("group-$g")
            assertEquals(expectedPerGroup, results.size,
                "Group group-$g: expected $expectedPerGroup rows, got ${results.size}")
        }
    }

    @Test
    fun `btree index single key with50 k rows`() {
        val cache = createSampleCache()
        val index = BTreeIndex(cache) { it.name }

        // All 50K rows share the same key
        measureTime {
            for (i in 0 until LARGE) {
                cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "shared").encode()))
            }
        }

        // filter() returns all 50K rows
        val filterTime = measureTime {
            val results = index.filter("shared")
            assertEquals(LARGE, results.size)
        }

        assertTrue(filterTime.inWholeMilliseconds < 2000,
            "BTreeIndex filter returning 50K rows took ${filterTime.inWholeMilliseconds}ms — too slow")

        // Non-existent key returns empty
        assertTrue(index.filter("nonexistent").isEmpty())
    }

    // ---- BTreeIndex: construction from pre-populated cache ----

    @Test
    fun `btree index construction from pre populated cache10 k`() {
        val cache = createSampleCache()
        val groupCount = 100
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "g-${i % groupCount}").encode()))
        }

        val constructionTime = measureTime {
            val index = BTreeIndex(cache) { it.name }
            val results = index.filter("g-0")
            assertEquals(MEDIUM / groupCount, results.size)
        }

        assertTrue(constructionTime.inWholeMilliseconds < 5000,
            "BTreeIndex construction from 10K rows took ${constructionTime.inWholeMilliseconds}ms — too slow")
    }

    // ---- Bulk delete at scale ----

    @Test
    fun `unique index bulk delete50 k`() {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        // Insert 50K rows
        for (i in 0 until LARGE) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }
        assertEquals(LARGE, cache.count())

        // Delete all rows
        val deleteTime = measureTime {
            for (i in 0 until LARGE) {
                val parsed = cache.parseDeletes(buildRowList(SampleRow(i, "row-$i").encode()))
                cache.applyDeletes(STUB_CTX, parsed)
            }
        }

        assertEquals(0, cache.count())
        // All lookups should return null
        for (i in listOf(0, LARGE / 2, LARGE - 1)) {
            assertEquals(null, index.find(i), "Row id=$i still in index after bulk delete")
        }

        assertTrue(deleteTime.inWholeMilliseconds < 10000,
            "50K row bulk delete took ${deleteTime.inWholeMilliseconds}ms — too slow")
    }

    @Test
    fun `btree index bulk delete converges`() {
        val cache = createSampleCache()
        val groupCount = 10
        val index = BTreeIndex(cache) { it.name }
        val rowsPerGroup = MEDIUM / groupCount  // 1000

        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "g-${i % groupCount}").encode()))
        }

        // Delete the first half of each group's rows.
        // Group g has rows: g, g+10, g+20, ... — delete the first rowsPerGroup/2 of them.
        for (g in 0 until groupCount) {
            var deleted = 0
            var id = g
            while (deleted < rowsPerGroup / 2) {
                val parsed = cache.parseDeletes(buildRowList(SampleRow(id, "g-$g").encode()))
                cache.applyDeletes(STUB_CTX, parsed)
                id += groupCount
                deleted++
            }
        }

        assertEquals(MEDIUM / 2, cache.count())
        // Each group should have exactly half its rows remaining
        for (g in 0 until groupCount) {
            val results = index.filter("g-$g")
            assertEquals(rowsPerGroup / 2, results.size,
                "Group g-$g after bulk delete: expected ${rowsPerGroup / 2}, got ${results.size}")
        }
    }

    // ---- Mixed read/write workload at scale ----

    @Test
    fun `unique index read heavy mixed workload`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        // Pre-populate with 10K rows
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }

        val threadCount = 16
        val opsPerThread = 5_000
        val barrier = CyclicBarrier(threadCount)

        coroutineScope {
            // 14 reader threads (87.5% reads)
            repeat(threadCount - 2) { _ ->
                launch {
                    barrier.await()
                    repeat(opsPerThread) { i ->
                        val key = i % MEDIUM
                        val found = index.find(key)
                        if (found != null) {
                            assertEquals(key, found.id, "Read returned wrong row")
                        }
                    }
                }
            }
            // 2 writer threads (12.5% writes — insert new rows beyond MEDIUM)
            repeat(2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = MEDIUM + threadIdx * opsPerThread
                    for (i in base until base + opsPerThread) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "new-$i").encode()))
                    }
                }
            }
        }

        // All original + new rows must be in the index
        val expectedTotal = MEDIUM + 2 * opsPerThread
        assertEquals(expectedTotal, cache.count())
        for (i in listOf(0, MEDIUM - 1, MEDIUM, expectedTotal - 1)) {
            assertNotNull(index.find(i), "Missing row id=$i after mixed workload")
        }
    }

    @Test
    fun `btree index read heavy mixed workload`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val groupCount = 50
        val index = BTreeIndex(cache) { it.name }

        // Pre-populate with 10K rows in 50 groups
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "g-${i % groupCount}").encode()))
        }

        val threadCount = 16
        val opsPerThread = 2_000
        val barrier = CyclicBarrier(threadCount)

        coroutineScope {
            // 14 reader threads
            repeat(threadCount - 2) { _ ->
                launch {
                    barrier.await()
                    repeat(opsPerThread) { i ->
                        val group = "g-${i % groupCount}"
                        val results = index.filter(group)
                        // Group should have at least the pre-populated count
                        assertTrue(results.isNotEmpty(), "Empty filter result for $group")
                    }
                }
            }
            // 2 writer threads add rows to existing groups
            repeat(2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = MEDIUM + threadIdx * opsPerThread
                    for (i in base until base + opsPerThread) {
                        val group = "g-${i % groupCount}"
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, group).encode()))
                    }
                }
            }
        }

        val expectedTotal = MEDIUM + 2 * opsPerThread
        assertEquals(expectedTotal, cache.count())

        // Verify group counts converged
        val expectedPerGroup = expectedTotal / groupCount
        for (g in 0 until groupCount) {
            assertEquals(expectedPerGroup, index.filter("g-$g").size,
                "Group g-$g count mismatch after mixed workload")
        }
    }

    // ---- Insert then delete then re-insert at scale ----

    @Test
    fun `unique index insert delete reinsert cycle`() {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        // Insert 10K
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "v1-$i").encode()))
        }
        assertEquals(MEDIUM, cache.count())

        // Delete all
        for (i in 0 until MEDIUM) {
            val parsed = cache.parseDeletes(buildRowList(SampleRow(i, "v1-$i").encode()))
            cache.applyDeletes(STUB_CTX, parsed)
        }
        assertEquals(0, cache.count())
        assertEquals(null, index.find(0))

        // Re-insert with different names
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "v2-$i").encode()))
        }
        assertEquals(MEDIUM, cache.count())

        // Index should reflect the new version
        for (i in listOf(0, MEDIUM / 2, MEDIUM - 1)) {
            val found = index.find(i)
            assertNotNull(found, "Missing row id=$i after reinsert")
            assertEquals("v2-$i", found.name, "Row id=$i has stale name after reinsert")
        }
    }

    // ---- Multiple indexes on the same cache ----

    @Test
    fun `multiple indexes on same cache at scale`() {
        val cache = createSampleCache()
        val uniqueById = UniqueIndex(cache) { it.id }
        val btreeByName = BTreeIndex(cache) { it.name }

        val groupCount = 20
        for (i in 0 until MEDIUM) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "g-${i % groupCount}").encode()))
        }

        // UniqueIndex: every ID findable
        for (i in 0 until MEDIUM step 100) {
            assertNotNull(uniqueById.find(i), "UniqueIndex missing id=$i")
        }
        // BTreeIndex: correct group sizes
        for (g in 0 until groupCount) {
            assertEquals(MEDIUM / groupCount, btreeByName.filter("g-$g").size)
        }

        // Delete the first half of each group's rows
        val rowsPerGroup = MEDIUM / groupCount
        for (g in 0 until groupCount) {
            var deleted = 0
            var id = g
            while (deleted < rowsPerGroup / 2) {
                val parsed = cache.parseDeletes(buildRowList(SampleRow(id, "g-$g").encode()))
                cache.applyDeletes(STUB_CTX, parsed)
                id += groupCount
                deleted++
            }
        }

        assertEquals(MEDIUM / 2, cache.count())
        // Deleted rows gone from UniqueIndex (first row of g-0 = id 0)
        assertEquals(null, uniqueById.find(0))
        // Second half still present (e.g. id = groupCount * (rowsPerGroup/2) for g-0)
        val firstSurvivor = groupCount * (rowsPerGroup / 2) // first surviving row in g-0
        assertNotNull(uniqueById.find(firstSurvivor))
        // BTreeIndex groups halved
        for (g in 0 until groupCount) {
            assertEquals(rowsPerGroup / 2, btreeByName.filter("g-$g").size)
        }
    }
}
