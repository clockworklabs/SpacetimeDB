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
    // Post-Disconnect Operations
    // =========================================================================

    @Test
    fun callReducerAfterDisconnectDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // Graceful no-op — logs warning, does not throw
        conn.callReducer("add", byteArrayOf(), "args")
    }

    @Test
    fun callProcedureAfterDisconnectDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // Graceful no-op — logs warning, does not throw
        conn.callProcedure("proc", byteArrayOf())
    }

    @Test
    fun oneOffQueryAfterDisconnectDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // Graceful no-op — logs warning, does not throw
        conn.oneOffQuery("SELECT 1") {}
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
}
