package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Int128
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Int256
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.UInt128
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.UInt256
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.onFailure
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.onSuccess
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.procedures
import module_bindings.reducers
import module_bindings.withModuleBindings
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Shared compression test logic. Each subclass sets the [mode] and
 * all tests run end-to-end over that compression mode.
 */
abstract class CompressionTestBase(private val mode: CompressionMode) {

    private suspend fun connect(): ConnectedClient {
        val identityDeferred = CompletableDeferred<Pair<Identity, String>>()

        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withCompression(mode)
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
    fun `send message`() = runBlocking {
        val client = connect()
        client.subscribeAll()

        val text = "$mode-msg-${System.nanoTime()}"
        val received = CompletableDeferred<String>()
        client.conn.db.message.onInsert { _, row ->
            if (row.text == text) received.complete(row.text)
        }
        client.conn.reducers.sendMessage(text)

        assertEquals(text, withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.cleanup()
    }

    @Test
    fun `set name`() = runBlocking {
        val client = connect()
        client.subscribeAll()

        val name = "$mode-user-${System.nanoTime()}"
        val received = CompletableDeferred<String>()
        client.conn.db.user.onUpdate { _, _, newRow ->
            if (newRow.identity == client.identity && newRow.name == name) {
                received.complete(newRow.name)
            }
        }
        client.conn.reducers.setName(name)

        assertEquals(name, withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.cleanup()
    }

    @Test
    fun `insert big ints`() = runBlocking {
        val client = connect()
        client.subscribeAll()

        val one = BigInteger.ONE
        val i128 = Int128(one.shl(100))
        val u128 = UInt128(one.shl(120))
        val i256 = Int256(one.shl(200))
        val u256 = UInt256(one.shl(250))

        val received = CompletableDeferred<Boolean>()
        client.conn.db.bigIntRow.onInsert { _, row ->
            if (row.valI128 == i128 && row.valU128 == u128 &&
                row.valI256 == i256 && row.valU256 == u256
            ) {
                received.complete(true)
            }
        }
        client.conn.reducers.insertBigInts(i128, u128, i256, u256)

        assertTrue(withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.cleanup()
    }

    @Test
    fun `add note`() = runBlocking {
        val client = connect()
        client.subscribeAll()

        val content = "$mode-note-${System.nanoTime()}"
        val tag = "test-tag"
        val received = CompletableDeferred<String>()
        client.conn.db.note.onInsert { _, row ->
            if (row.content == content && row.tag == tag) {
                received.complete(row.content)
            }
        }
        client.conn.reducers.addNote(content, tag)

        assertEquals(content, withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.cleanup()
    }

    @Test
    fun `call greet procedure`() = runBlocking {
        val client = connect()

        val received = CompletableDeferred<String>()
        client.conn.procedures.greet("World") { _, result ->
            result
                .onSuccess { received.complete(it) }
                .onFailure { received.completeExceptionally(Exception("$it")) }
        }

        assertEquals("Hello, World!", withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.conn.disconnect()
    }

    @Test
    fun `call server ping procedure`() = runBlocking {
        val client = connect()

        val received = CompletableDeferred<String>()
        client.conn.procedures.serverPing { _, result ->
            result
                .onSuccess { received.complete(it) }
                .onFailure { received.completeExceptionally(Exception("$it")) }
        }

        assertEquals("pong", withTimeout(DEFAULT_TIMEOUT_MS) { received.await() })
        client.conn.disconnect()
    }
}

/** Tests with no compression. */
class NoneCompressionTest : CompressionTestBase(CompressionMode.NONE)

/** Tests with GZIP compression. */
class GzipCompressionTest : CompressionTestBase(CompressionMode.GZIP)

/** Tests with Brotli compression. */
class BrotliCompressionTest : CompressionTestBase(CompressionMode.BROTLI)
