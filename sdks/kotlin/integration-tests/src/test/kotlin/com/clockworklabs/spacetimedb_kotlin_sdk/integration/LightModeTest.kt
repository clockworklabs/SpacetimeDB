package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.reducers
import module_bindings.withModuleBindings
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * Verifies that light mode connections work correctly.
 * Light mode skips sending initial subscription rows — the client
 * can still call reducers and receive subsequent table updates.
 */
class LightModeTest {

    private suspend fun connectLightMode(): ConnectedClient {
        val identityDeferred = CompletableDeferred<Pair<Identity, String>>()

        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withLightMode(true)
            .withModuleBindings()
            .onConnect { _, identity, tok ->
                identityDeferred.complete(identity to tok)
            }
            .onConnectError { _, e ->
                identityDeferred.completeExceptionally(e)
            }
            .build()

        val (identity, tok) = withTimeout(DEFAULT_TIMEOUT_MS) { identityDeferred.await() }
        return ConnectedClient(conn = conn, identity = identity, token = tok)
    }

    @Test
    fun `connect in light mode and call reducer`() = runBlocking {
        val client = connectLightMode()

        // Subscribe — in light mode, initial rows are skipped
        val applied = CompletableDeferred<Unit>()
        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .subscribe(listOf("SELECT * FROM message"))
        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        // Send a message and verify we receive the insert callback
        val text = "light-mode-${System.nanoTime()}"
        val received = CompletableDeferred<String>()
        client.conn.db.message.onInsert { _, row ->
            if (row.text == text) received.complete(row.text)
        }
        client.conn.reducers.sendMessage(text)

        assertEquals(text, withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.conn.disconnect()
    }

    @Test
    fun `light mode subscription starts with empty cache`() = runBlocking {
        val client = connectLightMode()

        val applied = CompletableDeferred<Unit>()
        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .subscribe(listOf("SELECT * FROM note"))
        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        // In light mode, the cache should be empty after subscription
        // (no initial rows sent by server)
        assertEquals(client.conn.db.note.count(), 0, "Light mode should not receive initial rows")
        client.conn.disconnect()
    }
}
