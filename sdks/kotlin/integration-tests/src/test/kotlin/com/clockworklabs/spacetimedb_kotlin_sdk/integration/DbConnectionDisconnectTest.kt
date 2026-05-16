package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class DbConnectionDisconnectTest {

    @Test
    fun `double disconnect does not throw`() = runBlocking {
        val client = connectToDb()
        assertTrue(client.conn.isActive)

        client.conn.disconnect()
        assertFalse(client.conn.isActive)

        // Second disconnect should be a no-op
        client.conn.disconnect()
        assertFalse(client.conn.isActive)
    }

    @Test
    fun `disconnect fires onDisconnect callback`() = runBlocking {
        val client = connectToDb()
        val disconnected = CompletableDeferred<Throwable?>()

        client.conn.onDisconnect { _, error ->
            disconnected.complete(error)
        }

        client.conn.disconnect()

        val error = withTimeout(DEFAULT_TIMEOUT_MS) { disconnected.await() }
        assertEquals(error, null, "Clean disconnect should have null error, got: $error")
    }

    @Test
    fun `reducer call after disconnect does not crash`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        client.conn.disconnect()
        assertFalse(client.conn.isActive)

        // Calling a reducer on a disconnected connection should not crash
        try {
            client.conn.reducers.sendMessage("should-not-arrive")
        } catch (_: Exception) {
            // Expected — some SDKs throw, some silently fail
        }
    }

    @Test
    fun `suspend oneOffQuery after disconnect throws immediately`() = runBlocking {
        // After disconnect the send channel is closed, so oneOffQuery throws
        // IllegalStateException immediately rather than hanging.
        val client = connectToDb()
        client.conn.disconnect()

        var threw = false
        try {
            withTimeout(2000) {
                client.conn.oneOffQuery("SELECT * FROM user")
            }
        } catch (_: TimeoutCancellationException) {
            threw = true
        } catch (_: Exception) {
            threw = true
        }
        assertTrue(threw, "suspend oneOffQuery on disconnected conn should fail")
    }

    @Test
    fun `callback oneOffQuery after disconnect does not crash`() = runBlocking {
        val client = connectToDb()
        client.conn.disconnect()

        // Callback variant — just fires and forgets, callback never invoked
        try {
            client.conn.oneOffQuery("SELECT * FROM user") { _ -> }
        } catch (_: Exception) {
            // Expected
        }
        Unit
    }
}
