package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.use
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.withModuleBindings
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class DbConnectionUseTest {

    private suspend fun buildConnectedDb(): DbConnection {
        val connected = CompletableDeferred<Unit>()
        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .onConnect { _, _, _ -> connected.complete(Unit) }
            .onConnectError { _, e -> connected.completeExceptionally(e) }
            .build()
        withTimeout(DEFAULT_TIMEOUT_MS) { connected.await() }
        return conn
    }

    @Test
    fun `use block auto-disconnects after block completes`() = runBlocking {
        val conn = buildConnectedDb()
        assertTrue(conn.isActive, "Connection should be active before use{}")

        conn.use {
            assertTrue(it.isActive, "Connection should be active inside use{}")
        }

        assertFalse(conn.isActive, "Connection should be inactive after use{}")
    }

    @Test
    fun `use block disconnects even when exception is thrown`() = runBlocking {
        val conn = buildConnectedDb()
        assertTrue(conn.isActive)

        assertFailsWith<IllegalStateException> {
            conn.use {
                throw IllegalStateException("test error inside use{}")
            }
        }

        assertFalse(conn.isActive, "Connection should be inactive after exception in use{}")
    }

    @Test
    fun `use block propagates return value`() = runBlocking {
        val conn = buildConnectedDb()

        val result = conn.use { 42 }

        assertEquals(42, result, "use{} should propagate the return value")
        assertFalse(conn.isActive)
    }

    @Test
    fun `use block disconnects on coroutine cancellation`() = runBlocking {
        val conn = buildConnectedDb()
        assertTrue(conn.isActive)

        try {
            coroutineScope {
                launch {
                    conn.use {
                        // Cancel the outer scope while inside use{}
                        this@coroutineScope.cancel("test cancellation")
                        // Suspend to let cancellation propagate
                        kotlinx.coroutines.delay(Long.MAX_VALUE)
                    }
                }
            }
        } catch (_: CancellationException) {
            // expected
        }

        // Give NonCancellable disconnect a moment to complete
        kotlinx.coroutines.delay(500)
        assertFalse(conn.isActive, "Connection should be inactive after cancellation")
    }
}
