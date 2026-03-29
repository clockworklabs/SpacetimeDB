package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.launch
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
class ConnectionLifecycleTest {

    // --- Connection lifecycle ---

    @Test
    fun onConnectFiresAfterInitialConnection() = runTest {
        val transport = FakeTransport()
        var connectIdentity: Identity? = null
        var connectToken: String? = null

        val conn = buildTestConnection(transport, onConnect = { _, id, tok ->
            connectIdentity = id
            connectToken = tok
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(TEST_IDENTITY, connectIdentity)
        assertEquals(TEST_TOKEN, connectToken)
        conn.disconnect()
    }

    @Test
    fun identityAndTokenSetAfterConnect() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        assertNull(conn.identity)
        assertNull(conn.token)

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(TEST_IDENTITY, conn.identity)
        assertEquals(TEST_TOKEN, conn.token)
        assertEquals(TEST_CONNECTION_ID, conn.connectionId)
        conn.disconnect()
    }

    @Test
    fun onDisconnectFiresOnServerClose() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        var disconnectError: Throwable? = null

        val conn = buildTestConnection(transport, onDisconnect = { _, err ->
            disconnected = true
            disconnectError = err
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        transport.closeFromServer()
        advanceUntilIdle()

        assertTrue(disconnected)
        assertNull(disconnectError)
        conn.disconnect()
    }

    // --- onConnectError ---

    @Test
    fun onConnectErrorFiresWhenTransportFails() = runTest {
        val error = RuntimeException("connection refused")
        val transport = FakeTransport(connectError = error)
        var capturedError: Throwable? = null

        val conn = createTestConnection(transport, onConnectError = { _, err ->
            capturedError = err
        })
        conn.connect()

        assertEquals(error, capturedError)
        assertFalse(conn.isActive)
    }

    // --- Identity mismatch ---

    @Test
    fun identityMismatchFiresOnConnectErrorAndDisconnects() = runTest {
        val transport = FakeTransport()
        var errorMsg: String? = null
        var disconnectReason: Throwable? = null
        var disconnected = false
        val conn = buildTestConnection(
            transport,
            onConnectError = { _, err -> errorMsg = err.message },
            onDisconnect = { _, reason ->
                disconnected = true
                disconnectReason = reason
            },
        )

        // First InitialConnection sets identity
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertEquals(TEST_IDENTITY, conn.identity)

        // Second InitialConnection with different identity triggers error and disconnect
        val differentIdentity = Identity(BigInteger.TEN)
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = differentIdentity,
                connectionId = TEST_CONNECTION_ID,
                token = TEST_TOKEN,
            )
        )
        advanceUntilIdle()

        // onConnectError fired
        assertNotNull(errorMsg)
        assertTrue(errorMsg!!.contains("unexpected identity"))
        // Identity should NOT have changed
        assertEquals(TEST_IDENTITY, conn.identity)
        // Connection should have transitioned to CLOSED (not left in CONNECTED)
        assertTrue(disconnected, "onDisconnect should have fired")
        assertNotNull(disconnectReason, "disconnect reason should be the identity mismatch error")
        assertTrue(disconnectReason!!.message!!.contains("unexpected identity"))
    }

    // --- close() ---

    @Test
    fun closeFiresOnDisconnect() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnected = true
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(disconnected)
    }

    // --- disconnect() states ---

    @Test
    fun disconnectWhenAlreadyDisconnectedIsNoOp() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()
        // Second disconnect should not throw
        conn.disconnect()
    }

    // --- close() from never-connected state ---

    @Test
    fun closeFromNeverConnectedState() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport)
        // close() on a freshly created connection that was never connected should not throw
        conn.disconnect()
    }

    // --- use {} block ---

    @Test
    fun useBlockDisconnectsOnNormalReturn() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, _ -> disconnected = true })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.use { /* no-op */ }
        advanceUntilIdle()

        assertTrue(disconnected)
        assertFalse(conn.isActive)
    }

    @Test
    fun useBlockDisconnectsOnException() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, _ -> disconnected = true })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertFailsWith<IllegalStateException> {
            conn.use { throw IllegalStateException("boom") }
        }
        advanceUntilIdle()

        assertTrue(disconnected)
        assertFalse(conn.isActive)
    }

    @Test
    fun useBlockReturnsValue() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val result = conn.use { 42 }

        assertEquals(42, result)
    }

    @Test
    fun useBlockDisconnectsOnCancellation() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, _ -> disconnected = true })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val job = launch {
            conn.use { kotlinx.coroutines.awaitCancellation() }
        }
        advanceUntilIdle()

        job.cancel()
        advanceUntilIdle()

        assertTrue(disconnected)
    }

    // --- Token not overwritten if already set ---

    @Test
    fun tokenNotOverwrittenOnSecondInitialConnection() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        // First connection sets token
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertEquals(TEST_TOKEN, conn.token)

        // Second InitialConnection with same identity but different token — token stays
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = TEST_IDENTITY,
                connectionId = TEST_CONNECTION_ID,
                token = "new-token",
            )
        )
        advanceUntilIdle()

        assertEquals(TEST_TOKEN, conn.token)
        conn.disconnect()
    }

    // --- sendMessage after close ---

    @Test
    fun subscribeAfterCloseDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // Calling subscribe on a closed connection is a graceful no-op
        // (logs warning, does not throw — matching C# SDK behavior)
        conn.subscribe(listOf("SELECT * FROM player"))
    }

    // --- Disconnect race conditions ---

    @Test
    fun disconnectDuringServerCloseDoesNotDoubleFireCallbacks() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Close from server side and call disconnect concurrently
        transport.closeFromServer()
        conn.disconnect()
        advanceUntilIdle()

        assertEquals(1, disconnectCount, "onDisconnect should fire exactly once")
    }

    @Test
    fun disconnectPassesReasonToCallbacks() = runTest {
        val transport = FakeTransport()
        var receivedError: Throwable? = null
        val conn = buildTestConnection(transport, onDisconnect = { _, err ->
            receivedError = err
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val reason = RuntimeException("forced disconnect")
        conn.disconnect(reason)
        advanceUntilIdle()

        assertEquals(reason, receivedError)
    }

    // --- SubscriptionError with null requestId triggers disconnect ---

    @Test
    fun subscriptionErrorWithNullRequestIdDisconnects() = runTest {
        val transport = FakeTransport()
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnected = true
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorMsg: String? = null
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM player"),
            onError = listOf { _, err -> errorMsg = (err as SubscriptionError.ServerError).message },
        )

        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = null,
                querySetId = handle.querySetId,
                error = "fatal subscription error",
            )
        )
        advanceUntilIdle()

        assertEquals("fatal subscription error", errorMsg)
        assertTrue(handle.isEnded)
        assertTrue(disconnected)
        conn.disconnect()
    }
}
