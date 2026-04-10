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
    fun `subscription state transitions pending to active to ended`() = runTest {
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
    fun `unsubscribe from unsubscribing state throws`() = runTest {
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
    fun `subscription error from pending state ends subscription`() = runTest {
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
    fun `multiple subscriptions track independently`() = runTest {
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
    fun `disconnect marks all pending and active subscriptions as ended`() = runTest {
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
    fun `unsubscribe applied with rows removes from cache`() = runTest {
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
        handle.unsubscribeThen {}
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
    fun `unsubscribe applied with null rows does not delete from cache`() = runTest {
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
    fun `multiple on applied callbacks all fire`() = runTest {
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
    fun `multiple on error callbacks all fire`() = runTest {
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
    fun `subscribe applied with many rows`() = runTest {
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
    fun `subscribe applied for unregistered table ignores rows`() = runTest {
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
    // doUnsubscribe callback-vs-CAS race
    // =========================================================================

    @Test
    fun `unsubscribe on ended subscription does not leak callback`() = runTest {
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

    // =========================================================================
    // Concurrent subscribe + unsubscribe
    // =========================================================================

    @Test
    fun `subscribe and immediate unsubscribe transitions correctly`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var appliedFired = false
        var endFired = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM t"),
            onApplied = listOf { _ -> appliedFired = true },
        )
        assertEquals(SubscriptionState.PENDING, handle.state)

        // Server confirms subscription
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(appliedFired)
        assertEquals(SubscriptionState.ACTIVE, handle.state)

        // Immediately unsubscribe
        handle.unsubscribeThen { _ -> endFired = true }
        assertEquals(SubscriptionState.UNSUBSCRIBING, handle.state)

        // Server confirms unsubscribe
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()
        assertTrue(endFired)
        assertEquals(SubscriptionState.ENDED, handle.state)
        conn.disconnect()
    }

    @Test
    fun `unsubscribe before applied throws`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        assertEquals(SubscriptionState.PENDING, handle.state)

        // Unsubscribe while still PENDING — CAS(ACTIVE→UNSUBSCRIBING) must fail
        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        assertEquals(SubscriptionState.PENDING, handle.state)
        conn.disconnect()
    }

    @Test
    fun `double unsubscribe throws`() = runTest {
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

        handle.unsubscribe()
        assertEquals(SubscriptionState.UNSUBSCRIBING, handle.state)

        // Second unsubscribe — state is UNSUBSCRIBING, not ACTIVE
        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }
}
