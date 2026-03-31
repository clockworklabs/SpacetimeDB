package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import java.util.concurrent.CyclicBarrier
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/**
 * Concurrency stress tests for the lock-free data structures in the SDK.
 * These run on JVM with real threads (Dispatchers.Default) to exercise
 * CAS loops and atomic operations under actual contention.
 */
class ConcurrencyStressTest {

    companion object {
        private const val THREAD_COUNT = 16
        private const val OPS_PER_THREAD = 500
    }

    // ---- TableCache: concurrent inserts ----

    @Test
    fun `table cache concurrent inserts are not lost`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val start = threadIdx * OPS_PER_THREAD
                    for (i in start until start + OPS_PER_THREAD) {
                        val row = SampleRow(i, "row-$i")
                        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
                    }
                }
            }
        }

        assertEquals(totalRows, cache.count())
        val allIds = cache.all().map { it.id }.toSet()
        assertEquals(totalRows, allIds.size)
        for (i in 0 until totalRows) {
            assertTrue(i in allIds, "Missing row id=$i")
        }
    }

    // ---- TableCache: concurrent inserts and deletes ----

    @Test
    fun `table cache concurrent insert and delete converges`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val barrier = CyclicBarrier(THREAD_COUNT)

        // Pre-insert rows that will be deleted
        val deleteRange = 0 until (THREAD_COUNT / 2) * OPS_PER_THREAD
        for (i in deleteRange) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "pre-$i").encode()))
        }

        coroutineScope {
            // Half the threads insert new rows
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = deleteRange.last + 1 + threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "new-$i").encode()))
                    }
                }
            }
            // Half the threads delete pre-inserted rows
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val start = threadIdx * OPS_PER_THREAD
                    for (i in start until start + OPS_PER_THREAD) {
                        val row = SampleRow(i, "pre-$i")
                        val parsed = cache.parseDeletes(buildRowList(row.encode()))
                        cache.applyDeletes(STUB_CTX, parsed)
                    }
                }
            }
        }

        // All pre-inserted rows should be gone, all new rows should exist
        val insertedCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        assertEquals(insertedCount, cache.count())
        for (row in cache.all()) {
            assertTrue(row.name.startsWith("new-"), "Unexpected row: $row")
        }
    }

    // ---- TableCache: concurrent reads during writes ----

    @Test
    fun `table cache reads are consistent snapshots during writes`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            // Writers
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
                    }
                }
            }
            // Readers: snapshot must always be self-consistent
            repeat(THREAD_COUNT / 2) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) {
                        val snapshot = cache.all()
                        cache.count()
                        // Snapshot is a point-in-time view — its size should be consistent
                        // (count() may differ since it reads a newer snapshot)
                        val ids = snapshot.map { it.id }.toSet()
                        assertEquals(snapshot.size, ids.size, "Snapshot contains duplicate IDs")
                    }
                }
            }
        }
    }

    // ---- TableCache: concurrent ref count increments and decrements ----

    @Test
    fun `table cache ref count survives concurrent increment decrement`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val sharedRow = SampleRow(42, "shared")
        cache.applyInserts(STUB_CTX, buildRowList(sharedRow.encode()))

        val barrier = CyclicBarrier(THREAD_COUNT)

        // Each thread increments then decrements the refcount
        coroutineScope {
            repeat(THREAD_COUNT) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(sharedRow.encode()))
                        val parsed = cache.parseDeletes(buildRowList(sharedRow.encode()))
                        cache.applyDeletes(STUB_CTX, parsed)
                    }
                }
            }
        }

        // After all increments + decrements, refcount should be back to 1
        assertEquals(1, cache.count())
        assertEquals(sharedRow, cache.all().single())
    }

    // ---- UniqueIndex: consistent with cache under concurrent mutations ----

    @Test
    fun `unique index stays consistent under concurrent inserts`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
                    }
                }
            }
        }

        // Every inserted row must be findable in the index
        for (i in 0 until totalRows) {
            val found = index.find(i)
            assertEquals(SampleRow(i, "row-$i"), found, "Index missing row id=$i")
        }
    }

    @Test
    fun `unique index stays consistent under concurrent inserts and deletes`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }

        // Pre-insert rows to delete
        val deleteCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        for (i in 0 until deleteCount) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "pre-$i").encode()))
        }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            // Inserters
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = deleteCount + threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "new-$i").encode()))
                    }
                }
            }
            // Deleters
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val start = threadIdx * OPS_PER_THREAD
                    for (i in start until start + OPS_PER_THREAD) {
                        val row = SampleRow(i, "pre-$i")
                        val parsed = cache.parseDeletes(buildRowList(row.encode()))
                        cache.applyDeletes(STUB_CTX, parsed)
                    }
                }
            }
        }

        // Deleted rows gone from index
        for (i in 0 until deleteCount) {
            assertEquals(null, index.find(i), "Deleted row id=$i still in index")
        }
        // New rows present in index
        val insertBase = deleteCount
        val insertCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        for (i in insertBase until insertBase + insertCount) {
            assertEquals(SampleRow(i, "new-$i"), index.find(i), "Index missing new row id=$i")
        }
    }

    // ---- BTreeIndex: consistent under concurrent mutations ----

    @Test
    fun `btree index stays consistent under concurrent inserts`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        // Key on name — groups of rows share the same name
        val groupCount = 10
        val index = BTreeIndex(cache) { it.name }
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val groupName = "group-${i % groupCount}"
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, groupName).encode()))
                    }
                }
            }
        }

        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        val expectedPerGroup = totalRows / groupCount
        for (g in 0 until groupCount) {
            val matches = index.filter("group-$g")
            assertEquals(expectedPerGroup, matches.size, "Group group-$g count mismatch")
        }
    }

    // ---- Callback registration: concurrent add/remove during iteration ----

    @Test
    fun `callback registration survives concurrent add remove`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val callCount = AtomicInteger(0)
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            // Half the threads add and remove callbacks
            repeat(THREAD_COUNT / 2) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) {
                        val cb: (EventContext, SampleRow) -> Unit = { _, _ -> callCount.incrementAndGet() }
                        cache.onInsert(cb)
                        cache.removeOnInsert(cb)
                    }
                }
            }
            // Other half trigger inserts (fires callbacks that are registered at snapshot time)
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val callbacks = cache.applyInserts(
                            STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode())
                        )
                        callbacks.forEach { it.invoke() }
                    }
                }
            }
        }

        // The test passes if no ConcurrentModificationException or lost update occurs.
        // callCount can be anything (depends on timing), but count() must be exact.
        val insertedCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        assertEquals(insertedCount, cache.count())
    }

    // ---- ClientCache.getOrCreateTable: concurrent creation of same table ----

    @Test
    fun `client cache get or create table is idempotent under contention`() = runBlocking(Dispatchers.Default) {
        val clientCache = ClientCache()
        val barrier = CyclicBarrier(THREAD_COUNT)
        val creationCount = AtomicInteger(0)

        val results = coroutineScope {
            (0 until THREAD_COUNT).map {
                async {
                    barrier.await()
                    clientCache.getOrCreateTable("players") {
                        creationCount.incrementAndGet()
                        TableCache.withPrimaryKey(::decodeSampleRow) { it.id }
                    }
                }
            }.awaitAll()
        }

        // All threads must get the same instance
        val first = results.first()
        for (table in results) {
            assertSame(first, table, "Different table instance returned by getOrCreateTable")
        }
        // Factory is called by each thread that misses the fast path (line 447).
        // Threads arriving after the table is visible skip factory entirely.
        // CAS retries never re-invoke factory — it's hoisted outside the loop.
        // In practice most threads miss the fast path under contention, but at least 1 must create.
        val count = creationCount.get()
        assertTrue(count >= 1, "Factory must be called at least once, got: $count")
        assertTrue(count <= THREAD_COUNT, "Factory called more than THREAD_COUNT times: $count")
    }

    // ---- NetworkRequestTracker: concurrent start/finish ----

    @Test
    fun `network request tracker concurrent start finish`() = runBlocking(Dispatchers.Default) {
        val tracker = NetworkRequestTracker()
        val barrier = CyclicBarrier(THREAD_COUNT)
        val totalOps = THREAD_COUNT * OPS_PER_THREAD

        coroutineScope {
            repeat(THREAD_COUNT) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) {
                        val id = tracker.startTrackingRequest("test")
                        tracker.finishTrackingRequest(id)
                    }
                }
            }
        }

        assertEquals(totalOps, tracker.sampleCount)
        assertEquals(0, tracker.requestsAwaitingResponse)
    }

    @Test
    fun `network request tracker concurrent insert sample`() = runBlocking(Dispatchers.Default) {
        val tracker = NetworkRequestTracker()
        val barrier = CyclicBarrier(THREAD_COUNT)
        val totalOps = THREAD_COUNT * OPS_PER_THREAD

        coroutineScope {
            repeat(THREAD_COUNT) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) { i ->
                        tracker.insertSample((i + 1).milliseconds, "op-$i")
                    }
                }
            }
        }

        assertEquals(totalOps, tracker.sampleCount)
        // Min must be 1ms (smallest sample), max must be OPS_PER_THREAD ms
        val result = assertNotNull(tracker.allTimeMinMax, "allTimeMinMax should not be null after $totalOps samples")
        assertEquals(1.milliseconds, result.min.duration, "allTimeMin wrong: ${result.min}")
        assertEquals(OPS_PER_THREAD.milliseconds, result.max.duration, "allTimeMax wrong: ${result.max}")
    }

    // ---- Logger: concurrent level/handler read/write ----

    @Test
    fun `logger concurrent level and handler changes`() = runBlocking(Dispatchers.Default) {
        val originalLevel = Logger.level
        val originalHandler = Logger.handler
        val barrier = CyclicBarrier(THREAD_COUNT)
        val logCount = AtomicInteger(0)

        try {
            coroutineScope {
                // Half the threads toggle the log level
                repeat(THREAD_COUNT / 2) { _ ->
                    launch {
                        barrier.await()
                        repeat(OPS_PER_THREAD) { i ->
                            Logger.level = if (i % 2 == 0) LogLevel.DEBUG else LogLevel.ERROR
                        }
                    }
                }
                // Other half swap the handler and log
                repeat(THREAD_COUNT / 2) { _ ->
                    launch {
                        barrier.await()
                        repeat(OPS_PER_THREAD) {
                            Logger.handler = LogHandler { _, _ -> logCount.incrementAndGet() }
                            Logger.info { "stress" }
                        }
                    }
                }
            }
            // No crash or exception = pass. logCount is non-deterministic.
        } finally {
            Logger.level = originalLevel
            Logger.handler = originalHandler
        }
    }

    // ---- Internal listeners: concurrent listener fire during add ----

    @Test
    fun `internal listeners fire safely during concurrent registration`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val listenerCallCount = AtomicInteger(0)
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            // Half add listeners
            repeat(THREAD_COUNT / 2) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD) {
                        cache.addInternalInsertListener { listenerCallCount.incrementAndGet() }
                    }
                }
            }
            // Half do inserts (which fire all currently-registered listeners)
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "r-$i").encode()))
                    }
                }
            }
        }

        val insertedCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        assertEquals(insertedCount, cache.count())
        // Listener calls >= 0, no crash = pass
        assertTrue(listenerCallCount.get() >= 0)
    }

    // ---- TableCache clear() racing with inserts ----

    @Test
    fun `table cache clear racing with inserts`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            // One thread clears repeatedly
            launch {
                barrier.await()
                repeat(OPS_PER_THREAD) {
                    cache.clear()
                }
            }
            // Rest insert
            repeat(THREAD_COUNT - 1) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "r-$i").encode()))
                    }
                }
            }
        }

        // The final state depends on timing, but the cache must be internally consistent:
        // count() == all().size, no duplicates in all()
        val all = cache.all()
        assertEquals(cache.count(), all.size)
        val ids = all.map { it.id }.toSet()
        assertEquals(all.size, ids.size, "Duplicate IDs after clear/insert race")
    }

    // ---- UniqueIndex: reads during concurrent mutations ----

    @Test
    fun `unique index reads return consistent snapshots during mutations`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val index = UniqueIndex(cache) { it.id }
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            // Writers
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "r-$i").encode()))
                    }
                }
            }
            // Readers
            repeat(THREAD_COUNT / 2) { _ ->
                launch {
                    barrier.await()
                    repeat(OPS_PER_THREAD * 2) { i ->
                        val row = index.find(i)
                        // If found, it must be consistent
                        if (row != null) {
                            assertEquals(i, row.id, "Index returned wrong row for key=$i")
                            assertEquals("r-$i", row.name)
                        }
                    }
                }
            }
        }
    }

    // ---- BTreeIndex: concurrent insert/delete with group verification ----

    @Test
    fun `btree index group count converges after concurrent insert delete`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val index = BTreeIndex(cache) { it.name }
        val groupName = "shared-group"

        // Pre-insert rows to delete
        val deleteCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        for (i in 0 until deleteCount) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, groupName).encode()))
        }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            // Insert new rows with same group
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = deleteCount + threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, groupName).encode()))
                    }
                }
            }
            // Delete pre-inserted rows
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val start = threadIdx * OPS_PER_THREAD
                    for (i in start until start + OPS_PER_THREAD) {
                        val row = SampleRow(i, groupName)
                        val parsed = cache.parseDeletes(buildRowList(row.encode()))
                        cache.applyDeletes(STUB_CTX, parsed)
                    }
                }
            }
        }

        val expectedCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        val groupRows = index.filter(groupName)
        assertEquals(expectedCount, groupRows.size, "BTreeIndex group count mismatch")
        // Verify only new rows remain
        for (row in groupRows) {
            assertTrue(row.id >= deleteCount, "Deleted row still in BTreeIndex: $row")
        }
    }

    // ---- DbConnection: concurrent disconnect from multiple threads ----

    @Test
    fun `concurrent disconnect fires callback exactly once`() = runBlocking(Dispatchers.Default) {
        val transport = FakeTransport()
        val disconnectCount = AtomicInteger(0)

        val conn = DbConnection(
            transport = transport,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
            onConnectCallbacks = emptyList(),
            onDisconnectCallbacks = listOf { _, _ -> disconnectCount.incrementAndGet() },
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = Identity(BigInteger.ONE),
                connectionId = ConnectionId(BigInteger.TWO),
                token = "token",
            )
        )
        // Give the receive loop time to process the initial connection
        kotlinx.coroutines.delay(100)
        assertTrue(conn.isActive)

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT) {
                launch {
                    barrier.await()
                    conn.disconnect()
                }
            }
        }

        assertFalse(conn.isActive)
        assertEquals(1, disconnectCount.get(), "onDisconnect must fire exactly once")
    }

    // ---- TableCache: concurrent updates (combined delete+insert) ----

    @Test
    fun `table cache concurrent updates replace correctly`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        // Pre-insert all rows with original names
        for (i in 0 until totalRows) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "original-$i").encode()))
        }
        assertEquals(totalRows, cache.count())

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val oldRow = SampleRow(i, "original-$i")
                        val newRow = SampleRow(i, "updated-$i")
                        val update = TableUpdateRows.PersistentTable(
                            inserts = buildRowList(newRow.encode()),
                            deletes = buildRowList(oldRow.encode()),
                        )
                        val parsed = cache.parseUpdate(update)
                        cache.applyUpdate(STUB_CTX, parsed)
                    }
                }
            }
        }

        // All rows should be updated, count unchanged
        assertEquals(totalRows, cache.count())
        for (row in cache.all()) {
            assertTrue(row.name.startsWith("updated-"), "Row not updated: $row")
        }
    }

    // ---- TableCache: two-phase deletes under contention ----

    @Test
    fun `two phase deletes under contention`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        for (i in 0 until totalRows) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
        }

        val beforeDeleteCount = AtomicInteger(0)
        val deleteCount = AtomicInteger(0)
        cache.onBeforeDelete { _, _ -> beforeDeleteCount.incrementAndGet() }
        cache.onDelete { _, _ -> deleteCount.incrementAndGet() }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val row = SampleRow(i, "row-$i")
                        val parsed = cache.parseDeletes(buildRowList(row.encode()))
                        cache.preApplyDeletes(STUB_CTX, parsed)
                        val callbacks = cache.applyDeletes(STUB_CTX, parsed)
                        callbacks.forEach { it.invoke() }
                    }
                }
            }
        }

        assertEquals(0, cache.count())
        assertEquals(totalRows, beforeDeleteCount.get())
        assertEquals(totalRows, deleteCount.get())
    }

    // ---- TableCache: two-phase updates under contention ----

    @Test
    fun `two phase updates under contention`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        for (i in 0 until totalRows) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "v0-$i").encode()))
        }

        val updateCount = AtomicInteger(0)
        cache.onUpdate { _, _, _ -> updateCount.incrementAndGet() }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val oldRow = SampleRow(i, "v0-$i")
                        val newRow = SampleRow(i, "v1-$i")
                        val update = TableUpdateRows.PersistentTable(
                            inserts = buildRowList(newRow.encode()),
                            deletes = buildRowList(oldRow.encode()),
                        )
                        val parsed = cache.parseUpdate(update)
                        cache.preApplyUpdate(STUB_CTX, parsed)
                        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
                        callbacks.forEach { it.invoke() }
                    }
                }
            }
        }

        assertEquals(totalRows, cache.count())
        assertEquals(totalRows, updateCount.get())
        for (row in cache.all()) {
            assertTrue(row.name.startsWith("v1-"), "Row not updated: $row")
        }
    }

    // ---- Content-key table: concurrent operations without primary key ----

    @Test
    fun `content key table concurrent inserts`() = runBlocking(Dispatchers.Default) {
        val cache = TableCache.withContentKey(::decodeSampleRow)
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        val barrier = CyclicBarrier(THREAD_COUNT)

        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
                    }
                }
            }
        }

        assertEquals(totalRows, cache.count())
        val allIds = cache.all().map { it.id }.toSet()
        assertEquals(totalRows, allIds.size)
    }

    @Test
    fun `content key table concurrent insert and delete`() = runBlocking(Dispatchers.Default) {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        // Pre-insert rows to delete
        val deleteCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        for (i in 0 until deleteCount) {
            cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "pre-$i").encode()))
        }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val base = deleteCount + threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "new-$i").encode()))
                    }
                }
            }
            repeat(THREAD_COUNT / 2) { threadIdx ->
                launch {
                    barrier.await()
                    val start = threadIdx * OPS_PER_THREAD
                    for (i in start until start + OPS_PER_THREAD) {
                        val row = SampleRow(i, "pre-$i")
                        val parsed = cache.parseDeletes(buildRowList(row.encode()))
                        cache.applyDeletes(STUB_CTX, parsed)
                    }
                }
            }
        }

        val expectedCount = (THREAD_COUNT / 2) * OPS_PER_THREAD
        assertEquals(expectedCount, cache.count())
        for (row in cache.all()) {
            assertTrue(row.name.startsWith("new-"), "Unexpected row: $row")
        }
    }

    // ---- Event table: concurrent fire-and-forget ----

    @Test
    fun `event table concurrent updates never store rows`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val insertCallbackCount = AtomicInteger(0)
        cache.onInsert { _, _ -> insertCallbackCount.incrementAndGet() }

        val barrier = CyclicBarrier(THREAD_COUNT)
        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        val event = TableUpdateRows.EventTable(
                            events = buildRowList(SampleRow(i, "evt-$i").encode()),
                        )
                        val parsed = cache.parseUpdate(event)
                        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
                        callbacks.forEach { it.invoke() }
                    }
                }
            }
        }

        // Event rows must never persist
        assertEquals(0, cache.count())
        // Every event should have fired a callback
        assertEquals(THREAD_COUNT * OPS_PER_THREAD, insertCallbackCount.get())
    }

    // ---- Index construction from pre-populated cache under contention ----

    @Test
    fun `index construction during concurrent inserts`() = runBlocking(Dispatchers.Default) {
        val cache = createSampleCache()
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        val barrier = CyclicBarrier(THREAD_COUNT + 1) // +1 for index builder

        val indices = mutableListOf<UniqueIndex<SampleRow, Int>>()

        coroutineScope {
            // Inserters
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
                    }
                }
            }
            // Index builder — constructs index while inserts are in flight
            launch {
                barrier.await()
                // Build index at various points during insertion
                repeat(10) {
                    val index = UniqueIndex(cache) { it.id }
                    synchronized(indices) { indices.add(index) }
                    // Small yield to let inserts progress
                    kotlinx.coroutines.yield()
                }
            }
        }

        // After all inserts complete, every index must be consistent with the final cache
        assertEquals(totalRows, cache.count())
        for (index in indices) {
            // Every row in the cache must be findable in every index
            for (i in 0 until totalRows) {
                val found = index.find(i)
                assertEquals(SampleRow(i, "row-$i"), found, "Index missing row id=$i")
            }
        }
    }

    // ---- ClientCache: concurrent operations across multiple tables ----

    @Test
    fun `client cache concurrent multi table operations`() = runBlocking(Dispatchers.Default) {
        val clientCache = ClientCache()
        val tableCount = 8
        val barrier = CyclicBarrier(THREAD_COUNT)

        // Each thread works on a different table (round-robin)
        coroutineScope {
            repeat(THREAD_COUNT) { threadIdx ->
                launch {
                    barrier.await()
                    val tableName = "table-${threadIdx % tableCount}"
                    val table = clientCache.getOrCreateTable(tableName) {
                        TableCache.withPrimaryKey(::decodeSampleRow) { it.id }
                    }
                    val base = threadIdx * OPS_PER_THREAD
                    for (i in base until base + OPS_PER_THREAD) {
                        table.applyInserts(STUB_CTX, buildRowList(SampleRow(i, "row-$i").encode()))
                    }
                }
            }
        }

        // Verify all tables exist and have correct counts
        val totalRows = THREAD_COUNT * OPS_PER_THREAD
        var totalCount = 0
        val allIds = mutableSetOf<Int>()
        for (t in 0 until tableCount) {
            val table = clientCache.getTable<SampleRow>("table-$t")
            totalCount += table.count()
            for (row in table.all()) {
                assertTrue(allIds.add(row.id), "Duplicate row id=${row.id} across tables")
            }
        }
        assertEquals(totalRows, totalCount)
        assertEquals(totalRows, allIds.size)
    }
}
