package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.protocol.*
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlin.time.measureTime

/**
 * Performance benchmarks for the core SDK machinery.
 *
 * These validate throughput and latency of:
 * - BSATN serialization/deserialization
 * - ClientCache insert/delete/update operations
 * - Full ServerMessage decode pipeline
 * - Gzip decompression throughput
 *
 * All tests run offline — no server required.
 */
class PerformanceBenchmarkTest {

    // ───────────────────────────── BSATN ─────────────────────────────

    @Test
    fun bsatnWriteThroughput() {
        val iterations = 100_000
        // Simulate writing a "player row": u64 id, string name, i32 x, i32 y, f64 health
        val elapsed = measureTime {
            repeat(iterations) {
                val w = BsatnWriter(64)
                w.writeU64(it.toULong())
                w.writeString("Player_$it")
                w.writeI32(it * 10)
                w.writeI32(it * -5)
                w.writeF64(100.0 - (it % 100))
                w.toByteArray()
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("BSATN write: ${iterations} rows in ${elapsed.inWholeMilliseconds}ms ($opsPerSec rows/sec)")
        // Sanity: should do at least 100k rows/sec on any modern machine
        assertTrue(elapsed.inWholeMilliseconds < 5000, "BSATN write too slow: ${elapsed.inWholeMilliseconds}ms")
    }

    @Test
    fun bsatnReadThroughput() {
        val iterations = 100_000
        // Pre-encode rows
        val rows = Array(iterations) { i ->
            val w = BsatnWriter(64)
            w.writeU64(i.toULong())
            w.writeString("Player_$i")
            w.writeI32(i * 10)
            w.writeI32(i * -5)
            w.writeF64(100.0 - (i % 100))
            w.toByteArray()
        }

        val elapsed = measureTime {
            for (data in rows) {
                val r = BsatnReader(data)
                r.readU64()
                r.readString()
                r.readI32()
                r.readI32()
                r.readF64()
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("BSATN read: ${iterations} rows in ${elapsed.inWholeMilliseconds}ms ($opsPerSec rows/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "BSATN read too slow: ${elapsed.inWholeMilliseconds}ms")
    }

    @Test
    fun bsatnRoundTripIntegrity() {
        // Verify data survives write → read for every primitive type
        val w = BsatnWriter(256)
        w.writeBool(true)
        w.writeBool(false)
        w.writeU8(255u)
        w.writeI8(-128)
        w.writeU16(65535u)
        w.writeI16(-32768)
        w.writeU32(UInt.MAX_VALUE)
        w.writeI32(Int.MIN_VALUE)
        w.writeU64(ULong.MAX_VALUE)
        w.writeI64(Long.MIN_VALUE)
        w.writeF32(3.14f)
        w.writeF64(2.718281828459045)
        w.writeString("Hello, SpacetimeDB! 🚀")
        w.writeByteArray(byteArrayOf(0xCA.toByte(), 0xFE.toByte()))

        val r = BsatnReader(w.toByteArray())
        assertEquals(true, r.readBool())
        assertEquals(false, r.readBool())
        assertEquals(255.toUByte(), r.readU8())
        assertEquals((-128).toByte(), r.readI8())
        assertEquals(65535.toUShort(), r.readU16())
        assertEquals((-32768).toShort(), r.readI16())
        assertEquals(UInt.MAX_VALUE, r.readU32())
        assertEquals(Int.MIN_VALUE, r.readI32())
        assertEquals(ULong.MAX_VALUE, r.readU64())
        assertEquals(Long.MIN_VALUE, r.readI64())
        assertEquals(3.14f, r.readF32())
        assertEquals(2.718281828459045, r.readF64())
        assertEquals("Hello, SpacetimeDB! 🚀", r.readString())
        val bytes = r.readByteArray()
        assertEquals(0xCA.toByte(), bytes[0])
        assertEquals(0xFE.toByte(), bytes[1])
        assertTrue(r.isExhausted, "Reader should be fully consumed")
    }

    // ───────────────────────── Client Cache ──────────────────────────

    @Test
    fun cacheInsertThroughput() {
        val cache = ClientCache()
        val table = cache.getOrCreateTable("players")
        val rowCount = 50_000
        // Pre-generate unique rows
        val rows = Array(rowCount) { i ->
            val w = BsatnWriter(32)
            w.writeU64(i.toULong())
            w.writeString("P$i")
            w.toByteArray()
        }

        val elapsed = measureTime {
            for (row in rows) {
                table.insertRow(row)
            }
        }
        assertEquals(rowCount, table.count)
        val opsPerSec = rowCount / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("Cache insert: $rowCount rows in ${elapsed.inWholeMilliseconds}ms ($opsPerSec rows/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "Cache insert too slow")
    }

    @Test
    fun cacheDeleteThroughput() {
        val cache = ClientCache()
        val table = cache.getOrCreateTable("players")
        val rowCount = 50_000
        val rows = Array(rowCount) { i ->
            val w = BsatnWriter(32)
            w.writeU64(i.toULong())
            w.writeString("P$i")
            w.toByteArray()
        }
        for (row in rows) table.insertRow(row)

        val elapsed = measureTime {
            for (row in rows) {
                table.deleteRow(row)
            }
        }
        assertEquals(0, table.count)
        val opsPerSec = rowCount / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("Cache delete: $rowCount rows in ${elapsed.inWholeMilliseconds}ms ($opsPerSec rows/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "Cache delete too slow")
    }

    @Test
    fun cacheRefCountingCorrectness() {
        // Overlapping subscriptions: same row inserted twice, deleted once → still present
        val table = TableCache("test")
        val row = byteArrayOf(1, 2, 3)
        table.insertRow(row)
        table.insertRow(row) // refCount = 2
        assertEquals(1, table.count, "Same row should not duplicate")
        table.deleteRow(row) // refCount = 1
        assertEquals(1, table.count, "Row should remain with refCount > 0")
        assertTrue(table.containsRow(row))
        table.deleteRow(row) // refCount = 0
        assertEquals(0, table.count, "Row should be removed at refCount 0")
    }

    @Test
    fun cacheTransactionUpdatePerformance() {
        val cache = ClientCache()
        // Pre-populate with 10k rows
        val table = cache.getOrCreateTable("entities")
        val existingRows = Array(10_000) { i ->
            val w = BsatnWriter(16)
            w.writeU64(i.toULong())
            w.writeI32(i)
            w.toByteArray()
        }
        for (row in existingRows) table.insertRow(row)

        // Simulate a transaction: delete 1000 rows, insert 1000 new, update 500
        val deleteRows = existingRows.take(1500) // 1000 pure deletes + 500 updates
        val updateNewRows = Array(500) { i ->
            val w = BsatnWriter(16)
            w.writeU64(i.toULong()) // same key as deleted
            w.writeI32(i + 999_999) // different value
            w.toByteArray()
        }
        val insertRows = Array(1000) { i ->
            val w = BsatnWriter(16)
            w.writeU64((20_000 + i).toULong())
            w.writeI32(i)
            w.toByteArray()
        }

        // Build the BsatnRowList payloads
        val deletePayload = buildRowListPayload(deleteRows.toList())
        val insertPayload = buildRowListPayload(updateNewRows.toList() + insertRows.toList())

        val qsUpdate = buildQuerySetUpdate("entities", insertPayload, deletePayload)
        val elapsed = measureTime {
            cache.applyTransactionUpdate(listOf(qsUpdate))
        }

        // Expected: 10000 - 1000 pure deletes + 1000 new inserts = 10000 (500 updates are in-place)
        println("Transaction update: 2500 ops in ${elapsed.inWholeMilliseconds}ms")
        assertTrue(elapsed.inWholeMilliseconds < 2000, "Transaction update too slow")
    }

    // ──────────────────── Protocol Decode Pipeline ───────────────────

    @Test
    fun initialConnectionDecodePerformance() {
        // Build a valid InitialConnection message
        val w = BsatnWriter(256)
        w.writeTag(0u) // InitialConnection tag
        w.writeBytes(ByteArray(32) { it.toByte() }) // identity
        w.writeBytes(ByteArray(16) { it.toByte() }) // connectionId
        w.writeString("test-token-abc123")
        val payload = w.toByteArray()

        val iterations = 50_000
        val elapsed = measureTime {
            repeat(iterations) {
                val msg = ServerMessage.decode(payload)
                assertTrue(msg is ServerMessage.InitialConnection)
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("InitialConnection decode: $iterations msgs in ${elapsed.inWholeMilliseconds}ms ($opsPerSec msg/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "Decode too slow")
    }

    @Test
    fun subscribeAppliedDecodeWithRows() {
        // Build a SubscribeApplied with 100 rows across 2 tables
        val w = BsatnWriter(4096)
        w.writeTag(1u) // SubscribeApplied
        w.writeU32(42u) // requestId
        w.writeU32(7u) // querySetId

        // QueryRows: array of SingleTableRows
        w.writeU32(1u) // 1 table
        w.writeString("players") // table name
        // BsatnRowList: RowSizeHint (tag + data) + length-prefixed row bytes
        val rowSize = 12 // u64 + i32
        val rowCount = 100
        w.writeTag(0u) // RowSizeHint::FixedSize
        w.writeU16(rowSize.toUShort())
        // Row data as a length-prefixed byte array
        w.writeU32((rowSize * rowCount).toUInt())
        repeat(rowCount) { i ->
            // Each row: u64 id, i32 score
            for (b in 0 until 8) w.writeI8(((i shr (b * 8)) and 0xFF).toByte())
            w.writeI32(i * 100)
        }

        val payload = w.toByteArray()

        val iterations = 10_000
        val elapsed = measureTime {
            repeat(iterations) {
                val msg = ServerMessage.decode(payload)
                assertTrue(msg is ServerMessage.SubscribeApplied)
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("SubscribeApplied decode (100 rows): $iterations msgs in ${elapsed.inWholeMilliseconds}ms ($opsPerSec msg/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 10000, "SubscribeApplied decode too slow")
    }

    @Test
    fun clientMessageEncodeThroughput() {
        val iterations = 100_000
        val elapsed = measureTime {
            repeat(iterations) { i ->
                val msg = ClientMessage.CallReducer(
                    requestId = i.toUInt(),
                    reducer = "set_position",
                    args = byteArrayOf(1, 2, 3, 4, 5, 6, 7, 8),
                )
                msg.encode()
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("CallReducer encode: $iterations msgs in ${elapsed.inWholeMilliseconds}ms ($opsPerSec msg/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "Encode too slow")
    }

    // ──────────────────────── Gzip Decompression ─────────────────────

    @Test
    fun gzipDecompressionThroughput() {
        // Compress a realistic payload (1KB of row data) then benchmark decompression
        val payload = ByteArray(1024) { (it % 256).toByte() }
        val compressed = compressGzip(payload)
        println("Gzip: ${payload.size} bytes → ${compressed.size} bytes (${compressed.size * 100 / payload.size}%)")

        val iterations = 50_000
        val elapsed = measureTime {
            repeat(iterations) {
                val decompressed = decompressGzip(compressed)
                assertEquals(payload.size, decompressed.size)
            }
        }
        val opsPerSec = iterations / elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("Gzip decompress: $iterations x ${compressed.size}B in ${elapsed.inWholeMilliseconds}ms ($opsPerSec ops/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 10000, "Gzip decompression too slow")
    }

    @Test
    fun gzipLargePayloadDecompression() {
        // Simulate a large SubscribeApplied (100KB)
        val payload = ByteArray(100_000) { (it % 256).toByte() }
        val compressed = compressGzip(payload)
        println("Gzip large: ${payload.size} bytes → ${compressed.size} bytes")

        val iterations = 1_000
        val elapsed = measureTime {
            repeat(iterations) {
                val result = decompressGzip(compressed)
                assertEquals(payload.size, result.size)
            }
        }
        val mbPerSec = (payload.size.toLong() * iterations / 1024 / 1024) /
            elapsed.inWholeMilliseconds.coerceAtLeast(1) * 1000
        println("Gzip large decompress: $iterations x ${payload.size / 1024}KB in ${elapsed.inWholeMilliseconds}ms ($mbPerSec MB/sec)")
        assertTrue(elapsed.inWholeMilliseconds < 10000, "Large gzip decompression too slow")
    }

    // ──────────────────── Callback System ────────────────────────────

    @Test
    fun tableHandleCallbackPerformance() {
        val handle = TableHandle("test")
        var insertCount = 0
        var deleteCount = 0
        var updateCount = 0

        // Register multiple callbacks
        repeat(10) {
            handle.onInsert { insertCount++ }
            handle.onDelete { deleteCount++ }
            handle.onUpdate { _, _ -> updateCount++ }
        }

        val row = byteArrayOf(1, 2, 3, 4)
        val iterations = 100_000
        val elapsed = measureTime {
            repeat(iterations) {
                handle.fireInsert(row)
                handle.fireDelete(row)
                handle.fireUpdate(row, row)
            }
        }
        assertEquals(iterations * 10, insertCount)
        assertEquals(iterations * 10, deleteCount)
        assertEquals(iterations * 10, updateCount)
        println("Callbacks: ${iterations * 3} fires (10 listeners each) in ${elapsed.inWholeMilliseconds}ms")
        assertTrue(elapsed.inWholeMilliseconds < 5000, "Callbacks too slow")
    }

    @Test
    fun callbackRegistrationAndRemoval() {
        val handle = TableHandle("test")
        var count = 0
        val ids = mutableListOf<CallbackId>()

        // Register 100 callbacks that all increment count
        repeat(100) {
            ids.add(handle.onInsert { count++ })
        }

        // Remove every other one (50 removed, 50 remain)
        for (i in ids.indices step 2) {
            handle.removeOnInsert(ids[i])
        }

        handle.fireInsert(byteArrayOf(1))
        assertEquals(50, count, "Should have 50 callbacks remaining")
    }

    // ──────────────────── End-to-End Message Flow ────────────────────

    @Test
    fun fullMessageRoundTrip() {
        // Encode a Subscribe message, verify it round-trips through binary
        val subscribe = ClientMessage.Subscribe(
            requestId = 1u,
            querySetId = QuerySetId(42u),
            queryStrings = listOf("SELECT * FROM players", "SELECT * FROM items WHERE owner_id = 7"),
        )
        val encoded = subscribe.encode()
        assertTrue(encoded.isNotEmpty())

        // Decode it back manually
        val reader = BsatnReader(encoded)
        assertEquals(0, reader.readTag().toInt()) // Subscribe tag
        assertEquals(1u, reader.readU32()) // requestId
        assertEquals(42u, reader.readU32()) // querySetId
        val queryCount = reader.readU32().toInt()
        assertEquals(2, queryCount)
        assertEquals("SELECT * FROM players", reader.readString())
        assertEquals("SELECT * FROM items WHERE owner_id = 7", reader.readString())
        assertTrue(reader.isExhausted)
    }

    // ──────────────────── Helpers ────────────────────────────────────

    private fun compressGzip(data: ByteArray): ByteArray {
        val bos = java.io.ByteArrayOutputStream()
        java.util.zip.GZIPOutputStream(bos).use { it.write(data) }
        return bos.toByteArray()
    }

    private fun buildRowListPayload(rows: List<ByteArray>): ByteArray {
        val w = BsatnWriter(256)
        w.writeTag(0u) // RowSizeHint::FixedSize
        if (rows.isEmpty()) {
            w.writeU16(0u)
            w.writeU32(0u) // empty data
            return w.toByteArray()
        }
        val rowSize = rows.first().size
        w.writeU16(rowSize.toUShort())
        w.writeU32((rowSize * rows.size).toUInt()) // length-prefixed data
        for (row in rows) w.writeBytes(row)
        return w.toByteArray()
    }

    private fun buildQuerySetUpdate(
        tableName: String,
        insertPayload: ByteArray,
        deletePayload: ByteArray,
    ): QuerySetUpdate {
        // Encode to BSATN and decode — ensures we go through the real codec
        val w = BsatnWriter(insertPayload.size + deletePayload.size + 256)
        w.writeU32(1u) // querySetId
        w.writeU32(1u) // 1 table
        w.writeString(tableName)
        w.writeU32(1u) // 1 row update block
        w.writeTag(0u) // TableUpdateRows::PersistentTable
        // PersistentTableRows: inserts then deletes (each is a full BsatnRowList)
        w.writeBytes(insertPayload)
        w.writeBytes(deletePayload)

        return QuerySetUpdate.read(BsatnReader(w.toByteArray()))
    }
}
