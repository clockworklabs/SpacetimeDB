package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class SubscriptionIntegrationTest {

    // --- Subscriptions ---

    @Test
    fun `subscribe sends client message`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscribe(listOf("SELECT * FROM player"))
        advanceUntilIdle()

        val subMsg = transport.sentMessages.filterIsInstance<ClientMessage.Subscribe>().firstOrNull()
        assertNotNull(subMsg)
        assertEquals(listOf("SELECT * FROM player"), subMsg.queryStrings)
        conn.disconnect()
    }

    @Test
    fun `subscribe applied fires on applied callback`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM player"),
            onApplied = listOf { _ -> applied = true },
        )

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied)
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    @Test
    fun `subscription error fires on error callback`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorMsg: String? = null
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM nonexistent"),
            onError = listOf { _, err -> errorMsg = (err as SubscriptionError.ServerError).message },
        )

        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "table not found",
            )
        )
        advanceUntilIdle()

        assertEquals("table not found", errorMsg)
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    // --- Unsubscribe lifecycle ---

    @Test
    fun `unsubscribe then callback fires on unsubscribe applied`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> applied = true },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(applied)
        assertTrue(handle.isActive)

        var unsubEndFired = false
        handle.unsubscribeThen { _ -> unsubEndFired = true }
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // Verify Unsubscribe message was sent
        val unsubMsg = transport.sentMessages.filterIsInstance<ClientMessage.Unsubscribe>().firstOrNull()
        assertNotNull(unsubMsg)
        assertEquals(handle.querySetId, unsubMsg.querySetId)

        // Server confirms unsubscribe
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(unsubEndFired)
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    @Test
    fun `unsubscribe then callback is set before message sent`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        var callbackFired = false
        handle.unsubscribeThen { _ -> callbackFired = true }
        advanceUntilIdle()

        assertTrue(handle.isUnsubscribing)

        // Simulate immediate server response
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(callbackFired, "Callback should fire even with immediate server response")
        conn.disconnect()
    }

    // --- Unsubscribe from wrong state ---

    @Test
    fun `unsubscribe from pending state throws`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        // Handle is PENDING — no SubscribeApplied received yet
        assertTrue(handle.isPending)

        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    @Test
    fun `unsubscribe from ended state throws`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM player"),
            onError = listOf { _, _ -> },
        )

        // Force ENDED via SubscriptionError
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "error",
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isEnded)

        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    // --- Unsubscribe with custom flags ---

    @Test
    fun `unsubscribe with send dropped rows flag`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        handle.unsubscribe()
        advanceUntilIdle()

        val unsub = transport.sentMessages.filterIsInstance<ClientMessage.Unsubscribe>().last()
        assertEquals(handle.querySetId, unsub.querySetId)
        assertEquals(UnsubscribeFlags.SendDroppedRows, unsub.flags) // hardcoded internally
        conn.disconnect()
    }

    // --- Subscription state machine edge cases ---

    @Test
    fun `subscription error while unsubscribing moves to ended`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorMsg: String? = null
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onError = listOf { _, err -> errorMsg = (err as SubscriptionError.ServerError).message },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        // Start unsubscribing
        handle.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // Server sends error instead of UnsubscribeApplied
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 2u,
                querySetId = handle.querySetId,
                error = "internal error during unsubscribe",
            )
        )
        advanceUntilIdle()

        assertTrue(handle.isEnded)
        assertEquals("internal error during unsubscribe", errorMsg)
        conn.disconnect()
    }

    @Test
    fun `transaction update during unsubscribe still applies`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

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

        // Start unsubscribing
        handle.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // A transaction arrives while unsubscribe is in-flight — row is inserted
        val newRow = SampleRow(2, "Bob")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                update = TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(newRow.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Transaction should still be applied to cache
        assertEquals(2, cache.count())
        conn.disconnect()
    }

    // --- Overlapping subscriptions ---

    @Test
    fun `overlapping subscriptions ref count rows`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        var insertCount = 0
        var deleteCount = 0
        cache.onInsert { _, _ -> insertCount++ }
        cache.onDelete { _, _ -> deleteCount++ }

        // First subscription inserts row
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())
        assertEquals(1, insertCount) // onInsert fires for first occurrence

        // Second subscription also inserts the same row — ref count goes to 2
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Still one row (ref count = 2)
        assertEquals(1, insertCount) // onInsert does NOT fire again

        // First subscription unsubscribes — ref count decrements to 1
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Row still present (ref count = 1)
        assertEquals(0, deleteCount) // onDelete does NOT fire

        // Second subscription unsubscribes — ref count goes to 0
        handle2.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 4u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(0, cache.count()) // Row removed
        assertEquals(1, deleteCount) // onDelete fires now

        conn.disconnect()
    }

    @Test
    fun `overlapping subscription transaction update affects both handles`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        // Two subscriptions on the same table
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // ref count = 2

        // A TransactionUpdate that updates the row (delete old + insert new)
        val updatedRow = SampleRow(1, "Alice Updated")
        var updateOld: SampleRow? = null
        var updateNew: SampleRow? = null
        cache.onUpdate { _, old, new -> updateOld = old; updateNew = new }

        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle1.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(updatedRow.encode()),
                                            deletes = buildRowList(encodedRow),
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

        // The row should be updated in the cache
        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().first().name)
        assertEquals(row, updateOld)
        assertEquals(updatedRow, updateNew)

        // After unsubscribing handle1, the row still has ref count from handle2
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(updatedRow.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Still present via handle2
        assertEquals("Alice Updated", cache.all().first().name)

        conn.disconnect()
    }

    // --- Multi-subscription conflict scenarios ---

    @Test
    fun `multiple subscriptions independent lifecycle`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied1 = false
        var applied2 = false
        val handle1 = conn.subscribe(
            queries = listOf("SELECT * FROM players"),
            onApplied = listOf { _ -> applied1 = true },
        )
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM items"),
            onApplied = listOf { _ -> applied2 = true },
        )
        advanceUntilIdle()

        // Only first subscription is confirmed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied1)
        assertFalse(applied2)
        assertTrue(handle1.isActive)
        assertTrue(handle2.isPending)

        // Unsubscribe first while second is still pending
        handle1.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle1.isUnsubscribing)
        assertTrue(handle2.isPending)

        // Second subscription confirmed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied2)
        assertTrue(handle2.isActive)
        assertTrue(handle1.isUnsubscribing)

        // First unsubscribe confirmed
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isEnded)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    @Test
    fun `subscribe applied during unsubscribe of overlapping subscription`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val sharedRow = SampleRow(1, "Alice")
        val sub1OnlyRow = SampleRow(2, "Bob")

        // Sub1: gets both rows
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1OnlyRow.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())

        // Start unsubscribing sub1
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        assertTrue(handle1.isUnsubscribing)

        // Sub2 arrives while sub1 unsubscribe is in-flight — shares one row
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertTrue(handle2.isActive)
        // sharedRow now has ref count 2, sub1OnlyRow has ref count 1
        assertEquals(2, cache.count())

        // Sub1 unsubscribe completes — drops both rows by ref count
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1OnlyRow.encode())))
                ),
            )
        )
        advanceUntilIdle()

        // sharedRow survives (ref count 2 -> 1), sub1OnlyRow removed (ref count 1 -> 0)
        assertEquals(1, cache.count())
        assertEquals(sharedRow, cache.all().single())
        assertTrue(handle1.isEnded)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    @Test
    fun `subscription error does not affect other subscription cached rows`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")

        // Sub1: active with a row in cache
        val handle1 = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(row.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())
        assertTrue(handle1.isActive)

        // Sub2: errors during subscribe (requestId present = non-fatal)
        var sub2Error: SubscriptionError? = null
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM sample WHERE invalid"),
            onError = listOf { _, err -> sub2Error = err },
        )
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 2u,
                querySetId = handle2.querySetId,
                error = "parse error",
            )
        )
        advanceUntilIdle()

        // Sub2 is ended, but sub1's row must still be in cache
        assertTrue(handle2.isEnded)
        assertNotNull(sub2Error)
        assertTrue(handle1.isActive)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun `transaction update spans multiple query sets`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")

        // Two subscriptions on the same table
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row1.encode())))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList()))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Single TransactionUpdate with updates from BOTH query sets
        var insertCount = 0
        cache.onInsert { _, _ -> insertCount++ }
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle1.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(row2.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        ),
                        QuerySetUpdate(
                            handle2.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(row2.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        ),
                    )
                )
            )
        )
        advanceUntilIdle()

        // row2 inserted via both query sets — ref count = 2, but onInsert fires once
        assertEquals(2, cache.count())
        assertEquals(1, insertCount)
        conn.disconnect()
    }

    @Test
    fun `resubscribe after unsubscribe completes`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")

        // First subscription
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Unsubscribe
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(0, cache.count())
        assertTrue(handle1.isEnded)

        // Re-subscribe with the same query — fresh handle, row re-inserted
        var reApplied = false
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> reApplied = true },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 3u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        assertTrue(reApplied)
        assertTrue(handle2.isActive)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        // Old handle stays ended
        assertTrue(handle1.isEnded)
        conn.disconnect()
    }

    @Test
    fun `three overlapping subscriptions unsubscribe middle`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        var deleteCount = 0
        cache.onDelete { _, _ -> deleteCount++ }

        // Three subscriptions all sharing the same row
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle3 = conn.subscribe(listOf("SELECT * FROM sample WHERE name = 'Alice'"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 3u,
                querySetId = handle3.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        // ref count = 3
        assertEquals(1, cache.count())

        // Unsubscribe middle subscription
        handle2.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 4u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        // ref count 3 -> 2, row still present, no onDelete
        assertEquals(1, cache.count())
        assertEquals(0, deleteCount)
        assertTrue(handle2.isEnded)
        assertTrue(handle1.isActive)
        assertTrue(handle3.isActive)

        // Unsubscribe first
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 5u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        // ref count 2 -> 1, still present
        assertEquals(1, cache.count())
        assertEquals(0, deleteCount)

        // Unsubscribe last — ref count -> 0, row deleted
        handle3.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 6u,
                querySetId = handle3.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        assertEquals(0, cache.count())
        assertEquals(1, deleteCount)
        conn.disconnect()
    }

    @Test
    fun `unsubscribe drops unique rows but keeps shared rows`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val sharedRow = SampleRow(1, "Alice")
        val sub1Only = SampleRow(2, "Bob")
        val sub2Only = SampleRow(3, "Charlie")

        // Sub1: gets sharedRow + sub1Only
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample WHERE id <= 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1Only.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())

        // Sub2: gets sharedRow + sub2Only
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id != 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub2Only.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(3, cache.count())

        val deleted = mutableListOf<Int>()
        cache.onDelete { _, row -> deleted.add(row.id) }

        // Unsubscribe sub1 — drops sharedRow (ref 2->1) and sub1Only (ref 1->0)
        handle1.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1Only.encode())))
                ),
            )
        )
        advanceUntilIdle()

        // sub1Only deleted, sharedRow survives
        assertEquals(2, cache.count())
        assertEquals(listOf(2), deleted) // only sub1Only's id
        val remaining = cache.all().sortedBy { it.id }
        assertEquals(listOf(sharedRow, sub2Only), remaining)
        conn.disconnect()
    }
}
