package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ReducerAndQueryEdgeCaseTest {

    // =========================================================================
    // One-Off Query Edge Cases
    // =========================================================================

    @Test
    fun `multiple one off queries concurrently`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var result1: OneOffQueryResult? = null
        var result2: OneOffQueryResult? = null
        var result3: OneOffQueryResult? = null
        val id1 = conn.oneOffQuery("SELECT 1") { result1 = it }
        val id2 = conn.oneOffQuery("SELECT 2") { result2 = it }
        val id3 = conn.oneOffQuery("SELECT 3") { result3 = it }
        advanceUntilIdle()

        // Respond in reverse order
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id3, result = QueryResult.Ok(emptyQueryRows()))
        )
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id1, result = QueryResult.Ok(emptyQueryRows()))
        )
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id2, result = QueryResult.Err("error"))
        )
        advanceUntilIdle()

        assertNotNull(result1)
        assertNotNull(result2)
        assertNotNull(result3)
        assertTrue(result1 is SdkResult.Success)
        assertTrue(result2 is SdkResult.Failure)
        assertTrue(result3 is SdkResult.Success)
        conn.disconnect()
    }

    @Test
    fun `one off query callback is removed after firing`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callCount = 0
        val id = conn.oneOffQuery("SELECT 1") { callCount++ }
        advanceUntilIdle()

        // Send result twice with same requestId
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id, result = QueryResult.Ok(emptyQueryRows()))
        )
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id, result = QueryResult.Ok(emptyQueryRows()))
        )
        advanceUntilIdle()

        assertEquals(1, callCount) // Should only fire once
        conn.disconnect()
    }

    // =========================================================================
    // Reducer Edge Cases
    // =========================================================================

    @Test
    fun `reducer callback is removed after firing`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callCount = 0
        val id = conn.callReducer("add", byteArrayOf(), "args", callback = { callCount++ })
        advanceUntilIdle()

        // Send result twice
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(1, callCount) // Should only fire once
        conn.disconnect()
    }

    @Test
    fun `reducer result ok with table updates mutates cache`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Subscribe first to establish the table
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Call reducer
        var status: Status? = null
        val id = conn.callReducer("add", byteArrayOf(), "args", callback = { ctx -> status = ctx.status })
        advanceUntilIdle()

        // Reducer result with table insert
        val row = SampleRow(1, "FromReducer")
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Ok(
                    retValue = byteArrayOf(),
                    transactionUpdate = TransactionUpdate(
                        listOf(
                            QuerySetUpdate(
                                handle.querySetId,
                                listOf(
                                    TableUpdate(
                                        "sample",
                                        listOf(
                                            TableUpdateRows.PersistentTable(
                                                inserts = buildRowList(row.encode()),
                                                deletes = buildRowList(),
                                            )
                                        )
                                    )
                                )
                            )
                        )
                    ),
                ),
            )
        )
        advanceUntilIdle()

        assertEquals(Status.Committed, status)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        conn.disconnect()
    }

    @Test
    fun `reducer result with empty error bytes`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val id = conn.callReducer("bad", byteArrayOf(), "args", callback = { ctx -> status = ctx.status })
        advanceUntilIdle()

        // Empty error bytes
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(byteArrayOf()),
            )
        )
        advanceUntilIdle()

        assertTrue(status is Status.Failed)
        assertTrue((status as Status.Failed).message.contains("undecodable"))
        conn.disconnect()
    }

    // =========================================================================
    // Multi-Table Transaction Processing
    // =========================================================================

    @Test
    fun `transaction update across multiple tables`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Transaction inserting into both tables
        val rowA = SampleRow(1, "A")
        val rowB = SampleRow(2, "B")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "table_a",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(rowA.encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "table_b",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(rowB.encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(1, cacheA.count())
        assertEquals(1, cacheB.count())
        assertEquals(rowA, cacheA.all().single())
        assertEquals(rowB, cacheB.all().single())
        conn.disconnect()
    }

    @Test
    fun `transaction update with unknown table is skipped`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("known", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM known"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Transaction with both known and unknown tables
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "unknown",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(SampleRow(1, "ghost").encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "known",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(SampleRow(2, "visible").encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Known table gets the insert; unknown table is skipped without error
        assertEquals(1, cache.count())
        assertEquals("visible", cache.all().single().name)
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // =========================================================================
    // Concurrent Reducer Calls
    // =========================================================================

    @Test
    fun `multiple concurrent reducer calls get correct callbacks`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val results = mutableMapOf<String, Status>()
        val id1 = conn.callReducer("add", byteArrayOf(1), "add_args", callback = { ctx ->
            results["add"] = ctx.status
        })
        val id2 = conn.callReducer("remove", byteArrayOf(2), "remove_args", callback = { ctx ->
            results["remove"] = ctx.status
        })
        val id3 = conn.callReducer("update", byteArrayOf(3), "update_args", callback = { ctx ->
            results["update"] = ctx.status
        })
        advanceUntilIdle()

        // Respond in reverse order
        val writer = BsatnWriter()
        writer.writeString("update failed")
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id3,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(writer.toByteArray()),
            )
        )
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id1,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id2,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(3, results.size)
        assertEquals(Status.Committed, results["add"])
        assertEquals(Status.Committed, results["remove"])
        assertTrue(results["update"] is Status.Failed)
        conn.disconnect()
    }

    // =========================================================================
    // Content-Based Keying (Tables Without Primary Keys)
    // =========================================================================

    @Test
    fun `content keyed cache insert and delete`() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        cache.applyInserts(STUB_CTX, buildRowList(row1.encode(), row2.encode()))

        assertEquals(2, cache.count())
        assertTrue(cache.all().containsAll(listOf(row1, row2)))

        // Delete row1 by content
        val parsed = cache.parseDeletes(buildRowList(row1.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(row2, cache.all().single())
    }

    @Test
    fun `content keyed cache duplicate insert increments ref count`() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        assertEquals(1, cache.count()) // One unique row, ref count = 2

        // First delete decrements ref count
        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(1, cache.count()) // Still present

        // Second delete removes it
        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(0, cache.count())
    }

    @Test
    fun `content keyed cache update by content`() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val oldRow = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        // An update with same content in delete + different content in insert
        // For content-keyed tables, the "update" detection is by key,
        // and since keys are content-based, this is a delete+insert, not an update
        val newRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(newRow, cache.all().single())
    }

    // =========================================================================
    // Event Table Behavior
    // =========================================================================

    @Test
    fun `event table does not store rows but fires callbacks`() {
        val cache = createSampleCache()
        val events = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> events.add(row) }

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        val eventUpdate = TableUpdateRows.EventTable(
            events = buildRowList(row1.encode(), row2.encode())
        )
        val parsed = cache.parseUpdate(eventUpdate)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        for (cb in callbacks) cb.invoke()

        assertEquals(0, cache.count()) // Not stored
        assertEquals(listOf(row1, row2), events) // Callbacks fired
    }

    @Test
    fun `event table does not fire on before delete`() {
        val cache = createSampleCache()
        var beforeDeleteFired = false
        cache.onBeforeDelete { _, _ -> beforeDeleteFired = true }

        val eventUpdate = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "Alice").encode())
        )
        val parsed = cache.parseUpdate(eventUpdate)
        cache.preApplyUpdate(STUB_CTX, parsed)
        cache.applyUpdate(STUB_CTX, parsed)

        assertFalse(beforeDeleteFired)
    }
}
