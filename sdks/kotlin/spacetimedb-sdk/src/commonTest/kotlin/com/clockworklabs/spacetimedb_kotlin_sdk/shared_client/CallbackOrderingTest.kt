package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QueryRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.SingleTableRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TransactionUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class CallbackOrderingTest {

    // =========================================================================
    // Callback Ordering Guarantees
    // =========================================================================

    @Test
    fun `pre apply delete fires before apply delete across tables`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val rowA = SampleRow(1, "A")
        val rowB = SampleRow(2, "B")
        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows("table_a", buildRowList(rowA.encode())),
                        SingleTableRows("table_b", buildRowList(rowB.encode())),
                    )
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cacheA.count())
        assertEquals(1, cacheB.count())

        // Track ordering: onBeforeDelete should fire for BOTH tables
        // BEFORE any onDelete fires
        val events = mutableListOf<String>()
        cacheA.onBeforeDelete { _, _ -> events.add("beforeDelete_A") }
        cacheB.onBeforeDelete { _, _ -> events.add("beforeDelete_B") }
        cacheA.onDelete { _, _ -> events.add("delete_A") }
        cacheB.onDelete { _, _ -> events.add("delete_B") }

        // Transaction deleting from both tables
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
                                            inserts = buildRowList(),
                                            deletes = buildRowList(rowA.encode()),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "table_b",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(rowB.encode()),
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

        // All beforeDeletes must come before any delete
        val beforeDeleteIndices = events.indices.filter { events[it].startsWith("beforeDelete") }
        val deleteIndices = events.indices.filter { events[it].startsWith("delete_") }
        assertTrue(beforeDeleteIndices.isNotEmpty())
        assertTrue(deleteIndices.isNotEmpty())
        assertTrue(beforeDeleteIndices.max() < deleteIndices.min())

        conn.disconnect()
    }

    @Test
    fun `update does not fire on before delete for updated row`() {
        val cache = createSampleCache()
        val oldRow = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val beforeDeleteRows = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, row -> beforeDeleteRows.add(row) }

        // Update (same key in both inserts and deletes) should NOT fire onBeforeDelete
        val newRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)
        cache.applyUpdate(STUB_CTX, parsed)

        assertTrue(beforeDeleteRows.isEmpty(), "onBeforeDelete should NOT fire for updates")
    }

    @Test
    fun `pure delete fires on before delete`() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeleteRows = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeleteRows.add(r) }

        // Pure delete (no corresponding insert) should fire onBeforeDelete
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)

        assertEquals(listOf(row), beforeDeleteRows)
    }

    @Test
    fun `callback firing order insert update delete`() {
        val cache = createSampleCache()

        // Pre-populate
        val existingRow = SampleRow(1, "Old")
        val toDelete = SampleRow(2, "Delete Me")
        cache.applyInserts(STUB_CTX, buildRowList(existingRow.encode(), toDelete.encode()))

        val events = mutableListOf<String>()
        cache.onInsert { _, row -> events.add("insert:${row.name}") }
        cache.onUpdate { _, old, new -> events.add("update:${old.name}->${new.name}") }
        cache.onDelete { _, row -> events.add("delete:${row.name}") }
        cache.onBeforeDelete { _, row -> events.add("beforeDelete:${row.name}") }

        // Transaction: update row1, delete row2, insert row3
        val updatedRow = SampleRow(1, "New")
        val newRow = SampleRow(3, "Fresh")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(updatedRow.encode(), newRow.encode()),
            deletes = buildRowList(existingRow.encode(), toDelete.encode()),
        )
        val parsed = cache.parseUpdate(update)

        // Pre-apply phase
        cache.preApplyUpdate(STUB_CTX, parsed)

        // Only pure deletes get onBeforeDelete (not updates)
        assertEquals(listOf("beforeDelete:Delete Me"), events)

        // Apply phase
        events.clear()
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        for (cb in callbacks) cb.invoke()

        // Must contain all events in the correct order:
        // updates and inserts fire first (from the insert processing loop),
        // then pure deletes (from the remaining-deletes loop).
        assertEquals(
            listOf("update:Old->New", "insert:Fresh", "delete:Delete Me"),
            events,
        )
    }

    // =========================================================================
    // Callback Exception Resilience
    // =========================================================================

    @Test
    fun `on connect exception does not prevent subsequent messages`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, onConnect = { _, _, _ ->
            error("connect callback explosion")
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Connection should still work despite callback exception
        assertTrue(conn.isActive)

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        // The subscribe was sent and the SubscribeApplied was processed
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    @Test
    fun `on before delete exception does not prevent mutation`() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        cache.onBeforeDelete { _, _ -> error("boom in beforeDelete") }

        // The preApply phase will throw, but let's verify the apply phase
        // still works independently (since the exception is in user code,
        // it's caught by runUserCallback in DbConnection context)
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        // preApplyUpdate will throw since we're not wrapped in runUserCallback
        // This tests that if it does throw, the cache is still consistent
        try {
            cache.preApplyUpdate(STUB_CTX, parsed)
        } catch (_: Exception) {
            // Expected
        }

        // applyUpdate should still work
        cache.applyUpdate(STUB_CTX, parsed)
        assertEquals(0, cache.count())
    }

    // =========================================================================
    // EventContext Correctness
    // =========================================================================

    @Test
    fun `subscribe applied context type`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var capturedCtx: EventContext? = null
        cache.onInsert { ctx, _ -> capturedCtx = ctx }

        val row = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        assertTrue(capturedCtx is EventContext.SubscribeApplied)
        conn.disconnect()
    }

    @Test
    fun `transaction update context type`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        var capturedCtx: EventContext? = null
        cache.onInsert { ctx, _ -> capturedCtx = ctx }

        transport.sendToClient(
            transactionUpdateMsg(
                handle.querySetId,
                "sample",
                inserts = buildRowList(SampleRow(1, "Alice").encode()),
            )
        )
        advanceUntilIdle()

        assertTrue(capturedCtx is EventContext.Transaction)
        conn.disconnect()
    }

    // =========================================================================
    // onDisconnect callback edge cases
    // =========================================================================

    @Test
    fun `on disconnect added after build still fires`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Add callback AFTER connection is established
        var fired = false
        conn.onDisconnect { _, _ -> fired = true }

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(fired)
    }

    @Test
    fun `on connect error added after build still fires`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        // Add callback AFTER connection is established
        var fired = false
        conn.onConnectError { _, _ -> fired = true }

        // Trigger identity mismatch (which fires onConnectError)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val differentIdentity = Identity(BigInteger.TEN)
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = differentIdentity,
                connectionId = TEST_CONNECTION_ID,
                token = TEST_TOKEN,
            )
        )
        advanceUntilIdle()

        assertTrue(fired)
        // Connection auto-closes on identity mismatch (no manual disconnect needed)
    }
}
