package com.clockworklabs.spacetimedb

import kotlinx.coroutines.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.test.Test

/**
 * Keynote-2 style TPS benchmark — fund transfers with pipelined reducer calls.
 *
 * Mirrors the Rust benchmark client at templates/keynote-2/spacetimedb-rust-client:
 *   - 10 WebSocket connections (no subscriptions)
 *   - Zipf-distributed account selection (alpha=0.5, 100k accounts)
 *   - Batched pipeline: fire 16384 reducer calls, await all responses, repeat
 *   - 5s warmup + 5s measurement
 *
 * Prerequisites:
 *   1. `spacetime start` running on localhost:3000
 *   2. keynote-2 module published: `spacetime publish --server local sim`
 *   3. Database seeded via Rust client: `spacetimedb-rust-transfer-sim seed`
 *
 * Set SPACETIMEDB_TEST=1 to enable.
 */
class Keynote2BenchmarkTest {

    private val serverUri = System.getenv("SPACETIMEDB_URI") ?: "ws://127.0.0.1:3000"
    private val moduleName = System.getenv("SPACETIMEDB_MODULE") ?: "sim"

    private fun shouldRun(): Boolean = System.getenv("SPACETIMEDB_TEST") == "1"

    companion object {
        const val ACCOUNTS = 100_000
        const val ALPHA = 0.5
        const val CONNECTIONS = 10
        const val MAX_INFLIGHT = 16_384
        const val WARMUP_MS = 5_000L
        const val BENCH_MS = 5_000L
        const val AMOUNT = 1
        const val TOTAL_PAIRS = 10_000_000
    }

    /**
     * Zipf distribution sampler via inverse CDF with binary search.
     * Produces integers in [0, n) with P(k) proportional to 1/(k+1)^alpha.
     */
    private class ZipfSampler(n: Int, alpha: Double, seed: Long) {
        private val cdf: DoubleArray
        private val rng = java.util.Random(seed)

        init {
            val weights = DoubleArray(n) { 1.0 / Math.pow((it + 1).toDouble(), alpha) }
            val total = weights.sum()
            cdf = DoubleArray(n)
            var cumulative = 0.0
            for (i in weights.indices) {
                cumulative += weights[i] / total
                cdf[i] = cumulative
            }
        }

        fun sample(): Int {
            val u = rng.nextDouble()
            var lo = 0; var hi = cdf.size - 1
            while (lo < hi) {
                val mid = (lo + hi) ushr 1
                if (cdf[mid] < u) lo = mid + 1 else hi = mid
            }
            return lo
        }
    }

    /** Pre-compute [TOTAL_PAIRS] transfer pairs using Zipf distribution. */
    private fun generateTransferPairs(from: IntArray, to: IntArray) {
        val zipf = ZipfSampler(ACCOUNTS, ALPHA, 0x12345678L)
        var idx = 0
        while (idx < TOTAL_PAIRS) {
            val a = zipf.sample()
            val b = zipf.sample()
            if (a != b && a < ACCOUNTS && b < ACCOUNTS) {
                from[idx] = a
                to[idx] = b
                idx++
            }
        }
    }

    /** BSATN-encode transfer args: (from: u32, to: u32, amount: u32) in little-endian. */
    private fun encodeTransfer(from: Int, to: Int, amount: Int): ByteArray {
        val buf = ByteBuffer.allocate(12).order(ByteOrder.LITTLE_ENDIAN)
        buf.putInt(from).putInt(to).putInt(amount)
        return buf.array()
    }

    @Test
    fun keynote2Benchmark() {
        if (!shouldRun()) { println("SKIP"); return }

        println("=== Kotlin SDK Keynote-2 Transfer Benchmark ===")
        println("alpha=$ALPHA, amount=$AMOUNT, accounts=$ACCOUNTS")
        println("max inflight reducers = $MAX_INFLIGHT")
        println("connections = $CONNECTIONS")
        println()

        // Pre-compute transfer pairs (matches Rust client's make_transfers)
        print("Pre-computing transfer pairs... ")
        val fromArr = IntArray(TOTAL_PAIRS)
        val toArr = IntArray(TOTAL_PAIRS)
        generateTransferPairs(fromArr, toArr)
        println("done")

        val transfersPerWorker = TOTAL_PAIRS / CONNECTIONS

        runBlocking {
            // Open connections (no subscriptions — pure reducer pipelining)
            println("Initializing $CONNECTIONS connections...")
            val connections = (0 until CONNECTIONS).map {
                val ready = CompletableDeferred<DbConnection>()
                val conn = DbConnection.builder()
                    .withUri(serverUri)
                    .withModuleName(moduleName)
                    .withCompression(CompressionMode.NONE)
                    .onConnect { c, _, _ -> ready.complete(c) }
                    .onConnectError { e -> ready.completeExceptionally(e) }
                    .build()
                withTimeout(10_000) { ready.await() }
                conn
            }
            println("All $CONNECTIONS connections established")

            val completed = AtomicLong(0)
            val workersReady = AtomicInteger(0)
            val benchStartNanos = AtomicLong(0)

            println("Warming up for ${WARMUP_MS / 1000}s...")
            val warmupStartNanos = System.nanoTime()

            val jobs = connections.mapIndexed { workerIdx, conn ->
                launch(Dispatchers.Default) {
                    var tIdx = workerIdx * transfersPerWorker

                    // Pipeline batch: fire MAX_INFLIGHT calls, suspend until all respond
                    suspend fun runBatch(): Long {
                        val batchDone = CompletableDeferred<Unit>()
                        val remaining = AtomicInteger(MAX_INFLIGHT)

                        repeat(MAX_INFLIGHT) {
                            val idx = tIdx % TOTAL_PAIRS
                            tIdx++
                            val args = encodeTransfer(fromArr[idx], toArr[idx], AMOUNT)
                            conn.callReducer("transfer", args) {
                                if (remaining.decrementAndGet() == 0) {
                                    batchDone.complete(Unit)
                                }
                            }
                        }

                        batchDone.await()
                        return MAX_INFLIGHT.toLong()
                    }

                    // ── Warmup phase ──
                    while (System.nanoTime() - warmupStartNanos < WARMUP_MS * 1_000_000) {
                        runBatch()
                    }

                    // Sync: wait for all workers to finish warmup
                    workersReady.incrementAndGet()
                    while (workersReady.get() < CONNECTIONS) delay(1)

                    // First worker to pass sets the shared start time
                    benchStartNanos.compareAndSet(0, System.nanoTime())

                    // ── Measurement phase ──
                    val myStart = System.nanoTime()
                    while (System.nanoTime() - myStart < BENCH_MS * 1_000_000) {
                        val count = runBatch()
                        completed.addAndGet(count)
                    }
                }
            }

            println("Finished warmup. Benchmarking for ${BENCH_MS / 1000}s...")
            jobs.forEach { it.join() }

            val benchEndNanos = System.nanoTime()
            val totalCompleted = completed.get()
            val elapsed = (benchEndNanos - benchStartNanos.get()) / 1_000_000_000.0
            val tps = totalCompleted / elapsed

            println()
            println("=== Results ===")
            println("ran for ${"%.3f".format(elapsed)} seconds")
            println("completed $totalCompleted transfers")
            println("throughput was ${"%.1f".format(tps)} TPS")

            connections.forEach { it.disconnect() }
        }
    }
}
