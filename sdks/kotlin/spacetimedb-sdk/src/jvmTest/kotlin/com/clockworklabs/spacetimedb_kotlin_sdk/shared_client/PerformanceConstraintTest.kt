package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import kotlin.test.Test
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.measureTime

/**
 * Hardware-independent performance regression tests.
 *
 * Instead of absolute time budgets (which are fragile across machines),
 * these tests verify **algorithmic complexity** by measuring the ratio
 * between a small and large workload. If an O(1) operation becomes O(n),
 * or an O(n) operation becomes O(n^2), the ratio will blow past the limit.
 *
 * Pattern: run the same operation at size N and 8N, then assert that
 * the time ratio stays within the expected complexity class:
 *   - O(1):       ratio < 3     (should be ~1, allow jitter)
 *   - O(n):       ratio < 16    (should be ~8, allow 2x jitter)
 *   - O(n log n): ratio < 24    (should be ~11, allow 2x jitter)
 */
class PerformanceConstraintTest {

    companion object {
        private const val SMALL = 100_000
        private const val LARGE = SMALL * 8  // 800_000
        private const val SCALE = 8.0

        // 8 * log2(800_000)/log2(100_000) ≈ 8 * 1.18 ≈ 9.4, so allow up to ~24x
        private const val NLOGN_MAX = 24.0
        private const val LINEAR_MAX = SCALE * 2  // 16x
        // Persistent HAMT has O(log32 n) depth; at 800K entries cache pressure
        // adds ~4x overhead vs 100K. Allow 5x to stay hardware-independent.
        private const val CONSTANT_MAX = 5.0

        /** Warm up the JIT so the first measurement isn't penalized. */
        private inline fun warmup(block: () -> Unit) {
            repeat(3) { block() }
        }

        /** Measure median of 5 runs to reduce noise. */
        private inline fun measure(block: () -> Unit): Duration {
            val times = (1..5).map { measureTime { block() } }.sorted()
            return times[2] // median
        }

        private fun assertRatio(ratio: Double, maxRatio: Double, label: String) {
            println("  $label: ratio=${String.format("%.2f", ratio)}x (limit ${maxRatio}x)")
            assertTrue(ratio < maxRatio,
                "$label ratio was ${String.format("%.2f", ratio)}x — expected <${maxRatio}x")
        }
    }

    // -- BSATN encode: O(n) --------------------------------------------------

    @Test
    fun `bsatn encode scales linearly`() {
        val smallRows = (0 until SMALL).map { SampleRow(it, "name-$it") }
        val largeRows = (0 until LARGE).map { SampleRow(it, "name-$it") }

        warmup { for (row in smallRows) row.encode() }

        val smallTime = measure { for (row in smallRows) row.encode() }
        val largeTime = measure { for (row in largeRows) row.encode() }

        assertRatio(largeTime / smallTime, LINEAR_MAX, "BSATN encode ${SMALL}->${LARGE}")
    }

    // -- BSATN decode: O(n) --------------------------------------------------

    @Test
    fun `bsatn decode scales linearly`() {
        val smallEncoded = (0 until SMALL).map { SampleRow(it, "name-$it").encode() }
        val largeEncoded = (0 until LARGE).map { SampleRow(it, "name-$it").encode() }

        warmup { for (b in smallEncoded) decodeSampleRow(BsatnReader(b)) }

        val smallTime = measure { for (b in smallEncoded) decodeSampleRow(BsatnReader(b)) }
        val largeTime = measure { for (b in largeEncoded) decodeSampleRow(BsatnReader(b)) }

        assertRatio(largeTime / smallTime, LINEAR_MAX, "BSATN decode ${SMALL}->${LARGE}")
    }

    // -- TableCache insert: O(n log n) due to persistent HAMT copies ---------

    @Test
    fun `cache insert scales at most n log n`() {
        val smallRowList = buildRowList(*(0 until SMALL).map { SampleRow(it, "r-$it").encode() }.toTypedArray())
        val largeRowList = buildRowList(*(0 until LARGE).map { SampleRow(it, "r-$it").encode() }.toTypedArray())

        warmup {
            val c = createSampleCache()
            c.applyInserts(STUB_CTX, smallRowList)
        }

        val smallTime = measure {
            val c = createSampleCache()
            c.applyInserts(STUB_CTX, smallRowList)
        }
        val largeTime = measure {
            val c = createSampleCache()
            c.applyInserts(STUB_CTX, largeRowList)
        }

        assertRatio(largeTime / smallTime, NLOGN_MAX, "Cache insert ${SMALL}->${LARGE}")
    }

    // -- TableCache iterate: O(n) --------------------------------------------

    @Test
    fun `cache iterate scales linearly`() {
        val smallCache = createSampleCache()
        smallCache.applyInserts(STUB_CTX, buildRowList(*(0 until SMALL).map { SampleRow(it, "r-$it").encode() }.toTypedArray()))
        val largeCache = createSampleCache()
        largeCache.applyInserts(STUB_CTX, buildRowList(*(0 until LARGE).map { SampleRow(it, "r-$it").encode() }.toTypedArray()))

        warmup { smallCache.iter().forEach { } }

        val smallTime = measure { smallCache.iter().forEach { } }
        val largeTime = measure { largeCache.iter().forEach { } }

        assertRatio(largeTime / smallTime, LINEAR_MAX, "Cache iterate ${SMALL}->${LARGE}")
    }

    // -- UniqueIndex.find: O(1) ----------------------------------------------

    @Test
    fun `unique index find is constant time`() {
        val smallCache = createSampleCache()
        smallCache.applyInserts(STUB_CTX, buildRowList(*(0 until SMALL).map { SampleRow(it, "r-$it").encode() }.toTypedArray()))
        val smallIndex = UniqueIndex(smallCache) { it.id }

        val largeCache = createSampleCache()
        largeCache.applyInserts(STUB_CTX, buildRowList(*(0 until LARGE).map { SampleRow(it, "r-$it").encode() }.toTypedArray()))
        val largeIndex = UniqueIndex(largeCache) { it.id }

        val ops = 50_000
        warmup { repeat(ops) { smallIndex.find(it % SMALL) } }

        val smallTime = measure { repeat(ops) { smallIndex.find(it % SMALL) } }
        val largeTime = measure { repeat(ops) { largeIndex.find(it % LARGE) } }

        assertRatio(largeTime / smallTime, CONSTANT_MAX, "UniqueIndex.find ${SMALL}->${LARGE}")
    }

    // -- BTreeIndex.filter: O(result_size), result scales with table ---------

    @Test
    fun `btree index filter scales linearly in result size`() {
        val buckets = 8

        val smallCache = createSampleCache()
        smallCache.applyInserts(STUB_CTX, buildRowList(*(0 until SMALL).map { SampleRow(it, "g-${it % buckets}").encode() }.toTypedArray()))
        val smallIndex = BTreeIndex(smallCache) { it.name }

        val largeCache = createSampleCache()
        largeCache.applyInserts(STUB_CTX, buildRowList(*(0 until LARGE).map { SampleRow(it, "g-${it % buckets}").encode() }.toTypedArray()))
        val largeIndex = BTreeIndex(largeCache) { it.name }

        // Result set is LARGE/buckets vs SMALL/buckets — scales 8x with table size.
        // Lookup is O(1) but copying the result set is O(result_size).
        val ops = 10_000
        warmup { repeat(ops) { smallIndex.filter("g-${it % buckets}") } }

        val smallTime = measure { repeat(ops) { smallIndex.filter("g-${it % buckets}") } }
        val largeTime = measure { repeat(ops) { largeIndex.filter("g-${it % buckets}") } }

        assertRatio(largeTime / smallTime, LINEAR_MAX, "BTreeIndex.filter ${SMALL}->${LARGE}")
    }
}
