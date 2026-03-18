package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class SubscriptionEdgeCaseTest {

    // =========================================================================
    // Subscription Lifecycle Edge Cases
    // =========================================================================

    @Test
    fun subscriptionStateTransitionsPendingToActiveToEnded() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        assertEquals(SubscriptionState.PENDING, handle.state)
        assertTrue(handle.isPending)

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ACTIVE, handle.state)
        assertTrue(handle.isActive)

        handle.unsubscribe()
        assertEquals(SubscriptionState.UNSUBSCRIBING, handle.state)
        assertTrue(handle.isUnsubscribing)

        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ENDED, handle.state)
        assertTrue(handle.isEnded)

        conn.disconnect()
    }

    @Test
    fun unsubscribeFromUnsubscribingStateThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        handle.unsubscribe()
        assertTrue(handle.isUnsubscribing)

        // Second unsubscribe should fail — already unsubscribing
        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    @Test
    fun subscriptionErrorFromPendingStateEndsSubscription() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorReceived = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM bad"),
            onError = listOf { _, _ -> errorReceived = true },
        )
        assertTrue(handle.isPending)

        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "parse error",
            )
        )
        advanceUntilIdle()

        assertTrue(handle.isEnded)
        assertTrue(errorReceived)
        // Should not be able to unsubscribe
        assertFailsWith<IllegalStateException> { handle.unsubscribe() }
        conn.disconnect()
    }

    @Test
    fun multipleSubscriptionsTrackIndependently() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle1 = conn.subscribe(listOf("SELECT * FROM t1"))
        val handle2 = conn.subscribe(listOf("SELECT * FROM t2"))

        // Both start PENDING
        assertTrue(handle1.isPending)
        assertTrue(handle2.isPending)

        // Apply only handle1
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isActive)
        assertTrue(handle2.isPending) // handle2 still pending

        // Apply handle2
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isActive)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    @Test
    fun disconnectMarksAllPendingAndActiveSubscriptionsAsEnded() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val pending = conn.subscribe(listOf("SELECT * FROM t1"))
        val active = conn.subscribe(listOf("SELECT * FROM t2"))

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = active.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(pending.isPending)
        assertTrue(active.isActive)

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(pending.isEnded)
        assertTrue(active.isEnded)
    }

    @Test
    fun unsubscribeAppliedWithRowsRemovesFromCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
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

        // Unsubscribe with rows returned
        handle.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        assertEquals(0, cache.count())
        conn.disconnect()
    }

    // =========================================================================
    // Unsubscribe with Null Rows
    // =========================================================================

    @Test
    fun unsubscribeAppliedWithNullRowsDoesNotDeleteFromCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
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

        // Unsubscribe without SendDroppedRows — server sends null rows
        handle.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        // Row stays in cache when rows is null
        assertEquals(1, cache.count())
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    // =========================================================================
    // Multiple Callbacks Registration
    // =========================================================================

    @Test
    fun multipleOnAppliedCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var count = 0
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM t"),
            onApplied = listOf(
                { _ -> count++ },
                { _ -> count++ },
                { _ -> count++ },
            ),
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertEquals(3, count)
        conn.disconnect()
    }

    @Test
    fun multipleOnErrorCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var count = 0
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM t"),
            onError = listOf(
                { _, _ -> count++ },
                { _, _ -> count++ },
            ),
        )
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "oops",
            )
        )
        advanceUntilIdle()

        assertEquals(2, count)
        conn.disconnect()
    }

    // =========================================================================
    // SubscribeApplied with Large Row Sets
    // =========================================================================

    @Test
    fun subscribeAppliedWithManyRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // 100 rows
        val rows = (1..100).map { SampleRow(it, "Row$it") }
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows(
                            "sample",
                            buildRowList(*rows.map { it.encode() }.toTypedArray())
                        )
                    )
                ),
            )
        )
        advanceUntilIdle()

        assertEquals(100, cache.count())
        conn.disconnect()
    }

    // =========================================================================
    // SubscribeApplied for table not in cache
    // =========================================================================

    @Test
    fun subscribeAppliedForUnregisteredTableIgnoresRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        // No cache registered for "sample"
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "Alice").encode())))
                ),
            )
        )
        advanceUntilIdle()

        // Should not crash — rows for unregistered tables are silently skipped
        assertTrue(conn.isActive)
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    // =========================================================================
    // subscribeToAllTables excludes event tables
    // =========================================================================

    @Test
    fun subscribeToAllTablesUsesModuleDescriptorSubscribableNames() = runTest {
        val transport = FakeTransport()
        val descriptor = object : ModuleDescriptor {
            override val subscribableTableNames = listOf("player", "inventory")
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        val conn = buildTestConnection(transport, moduleDescriptor = descriptor, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscribeToAllTables()
        advanceUntilIdle()

        // The subscribe message should contain only the persistent table names
        val subscribeMsg = transport.sentMessages.filterIsInstance<com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage.Subscribe>().single()
        assertEquals(2, subscribeMsg.queryStrings.size)
        assertTrue(subscribeMsg.queryStrings.any { it.contains("player") })
        assertTrue(subscribeMsg.queryStrings.any { it.contains("inventory") })

        conn.disconnect()
    }

    @Test
    fun subscribeToAllTablesFallsBackToCacheWhenNoDescriptor() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscribeToAllTables()
        advanceUntilIdle()

        val subscribeMsg = transport.sentMessages.filterIsInstance<com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage.Subscribe>().single()
        assertEquals(1, subscribeMsg.queryStrings.size)
        assertTrue(subscribeMsg.queryStrings.single().contains("sample"))

        conn.disconnect()
    }

    // =========================================================================
    // doUnsubscribe callback-vs-CAS race
    // =========================================================================

    @Test
    fun unsubscribeOnEndedSubscriptionDoesNotLeakCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ACTIVE, handle.state)

        // Server ends the subscription (e.g. SubscriptionError with null requestId triggers disconnect)
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ENDED, handle.state)

        // User tries to unsubscribe with a callback on the already-ended subscription.
        // The callback must NOT fire — the CAS should fail and throw.
        var callbackFired = false
        assertFailsWith<IllegalStateException> {
            handle.unsubscribeThen {
                callbackFired = true
            }
        }
        advanceUntilIdle()
        kotlin.test.assertFalse(callbackFired, "onEnd callback must not fire when CAS fails")
        conn.disconnect()
    }
}
