package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ConnectionStateTransitionTest {

    // =========================================================================
    // Connection State Transitions
    // =========================================================================

    @Test
    fun connectionStateProgression() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        // Initial state — not active
        assertFalse(conn.isActive)

        // After connect() — active
        conn.connect()
        assertTrue(conn.isActive)

        // After disconnect() — not active
        conn.disconnect()
        advanceUntilIdle()
        assertFalse(conn.isActive)
    }

    @Test
    fun connectAfterDisconnectThrows() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        conn.connect()
        conn.disconnect()
        advanceUntilIdle()

        // CLOSED is terminal — cannot reconnect
        assertFailsWith<IllegalStateException> {
            conn.connect()
        }
    }

    @Test
    fun doubleConnectThrows() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        conn.connect()

        // Already CONNECTED — second connect should fail
        assertFailsWith<IllegalStateException> {
            conn.connect()
        }
        conn.disconnect()
    }

    @Test
    fun connectFailureRendersConnectionInactive() = runTest {
        val error = RuntimeException("connection refused")
        val transport = FakeTransport(connectError = error)
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        conn.connect()

        assertFalse(conn.isActive)
        // Cannot reconnect after failure (state is CLOSED)
        assertFailsWith<IllegalStateException> { conn.connect() }
    }

    @Test
    fun serverCloseRendersConnectionInactive() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertTrue(conn.isActive)
        transport.closeFromServer()
        advanceUntilIdle()

        assertFalse(conn.isActive)
    }

    @Test
    fun disconnectFromNeverConnectedIsNoOp() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        // Should not throw
        conn.disconnect()
        assertFalse(conn.isActive)
    }

    @Test
    fun disconnectAfterConnectRendersInactive() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        conn.connect()
        assertTrue(conn.isActive)

        conn.disconnect()
        advanceUntilIdle()

        assertFalse(conn.isActive)
    }

    // =========================================================================
    // Post-Disconnect Operations — sendMessage returns false, caller cleans up
    // =========================================================================

    @Test
    fun callReducerAfterDisconnectCleansUpTracking() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // sendMessage returns false — callback and tracker must be cleaned up
        conn.callReducer("add", byteArrayOf(), "args")
        assertEquals(0, conn.stats.reducerRequestTracker.requestsAwaitingResponse,
            "Reducer tracker must be cleaned up when send fails")
    }

    @Test
    fun callProcedureAfterDisconnectCleansUpTracking() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        conn.callProcedure("proc", byteArrayOf())
        assertEquals(0, conn.stats.procedureRequestTracker.requestsAwaitingResponse,
            "Procedure tracker must be cleaned up when send fails")
    }

    @Test
    fun oneOffQueryAfterDisconnectCleansUpTracking() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        conn.oneOffQuery("SELECT 1") {}
        assertEquals(0, conn.stats.oneOffRequestTracker.requestsAwaitingResponse,
            "OneOffQuery tracker must be cleaned up when send fails")
    }

    @Test
    fun subscribeAfterDisconnectCleansUpTracking() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        assertEquals(0, conn.stats.subscriptionRequestTracker.requestsAwaitingResponse,
            "Subscription tracker must be cleaned up when send fails")
        assertTrue(handle.isEnded, "Handle must be marked ended when send fails")
    }

    // =========================================================================
    // Disconnect reason propagation
    // =========================================================================

    @Test
    fun disconnectWithReasonPassesReasonToCallbacks() = runTest {
        val transport = FakeTransport()
        var receivedReason: Throwable? = null
        val conn = buildTestConnection(transport, onDisconnect = { _, err ->
            receivedReason = err
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val reason = RuntimeException("intentional shutdown")
        conn.disconnect(reason)
        advanceUntilIdle()

        assertEquals(reason, receivedReason)
    }

    @Test
    fun disconnectWithoutReasonPassesNull() = runTest {
        val transport = FakeTransport()
        var receivedReason: Throwable? = Throwable("sentinel")
        val conn = buildTestConnection(transport, onDisconnect = { _, err ->
            receivedReason = err
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertNull(receivedReason)
    }

    // =========================================================================
    // SubscriptionBuilder — subscribe(query) does NOT merge with addQuery()
    // =========================================================================

    @Test
    fun subscribeWithQueryDoesNotMergeAccumulatedAddQueryCalls() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscriptionBuilder()
            .addQuery("SELECT * FROM users")
            .subscribe("SELECT * FROM messages")
        advanceUntilIdle()

        val subMsg = transport.sentMessages.filterIsInstance<ClientMessage.Subscribe>().last()
        assertEquals(
            listOf("SELECT * FROM messages"),
            subMsg.queryStrings,
            "subscribe(query) must use only the passed query, ignoring addQuery() calls"
        )
        conn.disconnect()
    }

    @Test
    fun subscribeWithListDoesNotMergeAccumulatedAddQueryCalls() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscriptionBuilder()
            .addQuery("SELECT * FROM users")
            .subscribe(listOf("SELECT * FROM messages", "SELECT * FROM notes"))
        advanceUntilIdle()

        val subMsg = transport.sentMessages.filterIsInstance<ClientMessage.Subscribe>().last()
        assertEquals(
            listOf("SELECT * FROM messages", "SELECT * FROM notes"),
            subMsg.queryStrings,
            "subscribe(List) must use only the passed queries, ignoring addQuery() calls"
        )
        conn.disconnect()
    }

    // =========================================================================
    // Empty Subscription Queries
    // =========================================================================

    @Test
    fun subscribeWithEmptyQueryListSendsMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(emptyList())
        advanceUntilIdle()

        val subMsg = transport.sentMessages.filterIsInstance<ClientMessage.Subscribe>().lastOrNull()
        assertNotNull(subMsg)
        assertTrue(subMsg.queryStrings.isEmpty())
        assertEquals(emptyList(), handle.queries)
        conn.disconnect()
    }

    // =========================================================================
    // SubscriptionHandle.queries stores original query strings
    // =========================================================================

    @Test
    fun subscriptionHandleStoresOriginalQueries() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val queries = listOf("SELECT * FROM users", "SELECT * FROM messages")
        val handle = conn.subscribe(queries)

        assertEquals(queries, handle.queries)
        conn.disconnect()
    }

    // =========================================================================
    // Connect then immediate disconnect — state must end as Closed
    // =========================================================================

    @Test
    fun connectThenImmediateDisconnectEndsAsClosed() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })

        conn.connect()
        assertTrue(conn.isActive)

        // Disconnect immediately without waiting for server handshake
        conn.disconnect()
        advanceUntilIdle()

        assertFalse(conn.isActive, "State must be Closed after disconnect, not stuck in Connected")

        // Must not be reconnectable — Closed is terminal
        assertFailsWith<IllegalStateException> { conn.connect() }
    }
}
