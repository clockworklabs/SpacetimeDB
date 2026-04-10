package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class TableCacheIntegrationTest {

    // --- Table cache ---

    @Test
    fun `table cache updates on subscribe applied`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val rowList = buildRowList(row.encode())
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", rowList))),
            )
        )
        advanceUntilIdle()

        assertEquals(1, cache.count())
        assertEquals("Alice", cache.all().first().name)
        conn.disconnect()
    }

    @Test
    fun `table cache inserts and deletes via transaction update`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // First insert a row via SubscribeApplied
        val row1 = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row1.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Now send a TransactionUpdate that inserts row2 and deletes row1
        val row2 = SampleRow(2, "Bob")
        val inserts = buildRowList(row2.encode())
        val deletes = buildRowList(row1.encode())

        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(inserts, deletes))
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(1, cache.count())
        assertEquals("Bob", cache.all().first().name)
        conn.disconnect()
    }

    // --- Table callbacks through integration ---

    @Test
    fun `table on insert fires on subscribe applied`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var insertedRow: SampleRow? = null
        cache.onInsert { _, row -> insertedRow = row }

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

        assertEquals(row, insertedRow)
        conn.disconnect()
    }

    @Test
    fun `table on delete fires on transaction update`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
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
        assertEquals(1, cache.count())

        var deletedRow: SampleRow? = null
        cache.onDelete { _, r -> deletedRow = r }

        // Delete via TransactionUpdate
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(row.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(row, deletedRow)
        assertEquals(0, cache.count())
        conn.disconnect()
    }

    @Test
    fun `table on update fires on transaction update`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
        val oldRow = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(oldRow.encode())))),
            )
        )
        advanceUntilIdle()

        var updatedOld: SampleRow? = null
        var updatedNew: SampleRow? = null
        cache.onUpdate { _, old, new ->
            updatedOld = old
            updatedNew = new
        }

        // Update: delete old row, insert new row with same PK
        val newRow = SampleRow(1, "Alice Updated")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(newRow.encode()),
                                            deletes = buildRowList(oldRow.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(oldRow, updatedOld)
        assertEquals(newRow, updatedNew)
        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().first().name)
        conn.disconnect()
    }

    // --- onBeforeDelete ---

    @Test
    fun `on before delete fires before mutation`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row
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
        assertEquals(1, cache.count())

        // Track onBeforeDelete — at callback time, the row should still be in the cache
        var cacheCountDuringCallback: Int? = null
        var beforeDeleteRow: SampleRow? = null
        cache.onBeforeDelete { _, r ->
            beforeDeleteRow = r
            cacheCountDuringCallback = cache.count()
        }

        // Delete via TransactionUpdate
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(row.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(row, beforeDeleteRow)
        assertEquals(1, cacheCountDuringCallback) // Row still present during onBeforeDelete
        assertEquals(0, cache.count()) // Row removed after
        conn.disconnect()
    }

    // --- Cross-table preApply ordering ---

    @Test
    fun `cross table pre apply runs before any apply`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        // Set up two independent table caches
        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Subscribe and apply initial rows to both tables
        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        val rowA = SampleRow(1, "Alice")
        val rowB = SampleRow(2, "Bob")

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

        // Track event ordering: onBeforeDelete (preApply) vs onDelete (apply)
        val events = mutableListOf<String>()
        cacheA.onBeforeDelete { _, _ -> events.add("preApply_A") }
        cacheA.onDelete { _, _ -> events.add("apply_A") }
        cacheB.onBeforeDelete { _, _ -> events.add("preApply_B") }
        cacheB.onDelete { _, _ -> events.add("apply_B") }

        // Send a TransactionUpdate that deletes from BOTH tables
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate("table_a", listOf(TableUpdateRows.PersistentTable(buildRowList(), buildRowList(rowA.encode())))),
                                TableUpdate("table_b", listOf(TableUpdateRows.PersistentTable(buildRowList(), buildRowList(rowB.encode())))),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // The key invariant: ALL preApply callbacks fire before ANY apply callbacks
        assertEquals(listOf("preApply_A", "preApply_B", "apply_A", "apply_B"), events)
        conn.disconnect()
    }

    // --- Unknown querySetId / requestId (silent early returns) ---

    @Test
    fun `subscribe applied for unknown query set id is ignored`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a callback to verify it does NOT fire
        var insertFired = false
        cache.onInsert { _, _ -> insertFired = true }

        // Send SubscribeApplied for a querySetId that was never subscribed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 99u,
                querySetId = QuerySetId(999u),
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "ghost").encode())))),
            )
        )
        advanceUntilIdle()

        // Should not crash, no rows inserted, no callbacks fired
        assertTrue(conn.isActive)
        assertEquals(0, cache.count())
        assertFalse(insertFired)
        conn.disconnect()
    }

    @Test
    fun `reducer result for unknown request id is ignored`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val cacheCountBefore = cache.count()

        // Send ReducerResultMsg with an Ok that has table updates — should be silently skipped
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = 999u,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        assertEquals(cacheCountBefore, cache.count())
        conn.disconnect()
    }

    @Test
    fun `one off query result for unknown request id is ignored`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a real query so we can verify the unknown one doesn't interfere
        var realCallbackFired = false
        val realRequestId = conn.oneOffQuery("SELECT 1") { _ -> realCallbackFired = true }
        advanceUntilIdle()

        // Send result for unknown requestId
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = 999u,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        // The unknown result should not fire the real callback
        assertTrue(conn.isActive)
        assertFalse(realCallbackFired)

        // Now send the real result — should fire
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = realRequestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()
        assertTrue(realCallbackFired)
        conn.disconnect()
    }
}
