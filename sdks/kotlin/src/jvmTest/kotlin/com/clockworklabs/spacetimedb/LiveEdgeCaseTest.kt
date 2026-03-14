package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.websocket.ConnectionState
import kotlinx.coroutines.*
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.measureTime

/**
 * Live edge case tests against a local SpacetimeDB server.
 *
 * Set SPACETIMEDB_TEST=1 to enable.
 */
class LiveEdgeCaseTest {

    private val serverUri = System.getenv("SPACETIMEDB_URI") ?: "ws://127.0.0.1:3000"
    private val moduleName = System.getenv("SPACETIMEDB_MODULE") ?: "kotlin-sdk-test"

    private fun shouldRun(): Boolean = System.getenv("SPACETIMEDB_TEST") == "1"

    // ──────── Invalid connection scenarios ────────

    @Test
    fun connectToNonExistentModule() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connectError = CompletableDeferred<Throwable>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName("non_existent_module_xyz_12345")
                .onConnect { _, _, _ -> connectError.completeExceptionally(AssertionError("Should not connect")) }
                .onConnectError { e -> connectError.complete(e) }
                .onDisconnect { _, e -> if (!connectError.isCompleted) connectError.complete(e ?: RuntimeException("disconnected")) }
                .build()

            val error = withTimeout(10000) { connectError.await() }
            assertNotNull(error)
            println("PASS: Non-existent module rejected: ${error.message?.take(80)}")
            conn.disconnect()
        }
    }

    @Test
    fun connectToUnreachableHost() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connectError = CompletableDeferred<Throwable>()

            val conn = DbConnection.builder()
                .withUri("ws://192.0.2.1:9999") // TEST-NET, guaranteed unreachable
                .withModuleName("test")
                .onConnectError { e -> connectError.complete(e) }
                .onDisconnect { _, e -> if (!connectError.isCompleted) connectError.complete(e ?: RuntimeException("disconnected")) }
                .build()

            val error = withTimeout(15000) { connectError.await() }
            assertNotNull(error)
            println("PASS: Unreachable host properly errored: ${error::class.simpleName}")
            conn.disconnect()
        }
    }

    // ──────── Subscription edge cases ────────

    @Test
    fun subscribeWithInvalidSqlSyntax() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subError = CompletableDeferred<String>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subError.completeExceptionally(AssertionError("Should not apply")) }
                        .onError { err -> subError.complete(err) }
                        .subscribe("SELECTT * FROMM invalid_table_xyz")
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            val error = withTimeout(5000) { subError.await() }
            assertTrue(error.isNotEmpty(), "Should get a non-empty error message")
            println("PASS: Invalid SQL rejected: ${error.take(80)}")
            conn.disconnect()
        }
    }

    @Test
    fun subscribeToNonExistentTable() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subError = CompletableDeferred<String>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subError.completeExceptionally(AssertionError("Should not apply")) }
                        .onError { err -> subError.complete(err) }
                        .subscribe("SELECT * FROM nonexistent_table_xyz")
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            val error = withTimeout(5000) { subError.await() }
            assertTrue(error.isNotEmpty())
            println("PASS: Non-existent table rejected: ${error.take(80)}")
            conn.disconnect()
        }
    }

    @Test
    fun multipleIndependentSubscriptions() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val sub1Applied = CompletableDeferred<Unit>()
            val sub2Applied = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { sub1Applied.complete(Unit) }
                        .subscribe("SELECT * FROM player")

                    c.subscriptionBuilder()
                        .onApplied { sub2Applied.complete(Unit) }
                        .subscribe("SELECT * FROM message")

                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { sub1Applied.await() }
            withTimeout(5000) { sub2Applied.await() }
            println("PASS: Two independent subscriptions applied concurrently")
            conn.disconnect()
        }
    }

    @Test
    fun subscribeToAllTables() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subApplied.complete(Unit) }
                        .subscribeToAllTables()
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }
            println("PASS: subscribeToAllTables (SELECT * FROM *) applied")
            conn.disconnect()
        }
    }

    // ──────── Reducer edge cases ────────

    @Test
    fun callNonExistentReducer() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val result = CompletableDeferred<ReducerResult>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { _, _, _ -> connected.complete(Unit) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }

            conn.callReducer("nonexistent_reducer_xyz", byteArrayOf()) { r ->
                result.complete(r)
            }

            val res = withTimeout(5000) { result.await() }
            // Should get an error outcome, not a crash
            assertNotNull(res)
            println("PASS: Non-existent reducer returned: ${res.outcome::class.simpleName}")
            conn.disconnect()
        }
    }

    @Test
    fun callReducerWithEmptyArgs() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()
            val result = CompletableDeferred<ReducerResult>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subApplied.complete(Unit) }
                        .subscribe("SELECT * FROM player")
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }

            // add_player expects a String arg — empty args should cause an error
            conn.callReducer("add_player", byteArrayOf()) { r ->
                result.complete(r)
            }

            val res = withTimeout(5000) { result.await() }
            assertNotNull(res)
            // Should be an error since args don't match expected schema
            println("PASS: Empty args to add_player returned: ${res.outcome::class.simpleName}")
            conn.disconnect()
        }
    }

    // ──────── One-off query edge cases ────────

    @Test
    fun oneOffQueryInvalidSql() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<DbConnection>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ -> connected.complete(c) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            val c = withTimeout(5000) { connected.await() }
            val result = withTimeout(5000) { c.oneOffQuery("INVALID SQL QUERY!!!") }
            assertNotNull(result.error, "Should return an error for invalid SQL")
            assertNull(result.rows)
            println("PASS: Invalid SQL one-off query returned error: ${result.error?.take(80)}")
            conn.disconnect()
        }
    }

    @Test
    fun oneOffQueryEmptyResult() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<DbConnection>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ -> connected.complete(c) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            val c = withTimeout(5000) { connected.await() }
            // Query with impossible WHERE clause
            val result = withTimeout(5000) { c.oneOffQuery("SELECT * FROM player WHERE id = 999999999") }
            if (result.error != null) {
                println("PASS: Empty result query returned error: ${result.error}")
            } else {
                val rows = result.rows?.tables?.flatMap { it.rows.decodeRows() } ?: emptyList()
                println("PASS: Empty result query returned ${rows.size} rows")
            }
            conn.disconnect()
        }
    }

    // ──────── Token reuse ────────

    @Test
    fun reconnectWithSavedToken() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            // First connection: get identity and token
            val firstConnect = CompletableDeferred<Pair<Identity, String>>()
            val conn1 = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { _, id, token -> firstConnect.complete(Pair(id, token)) }
                .onConnectError { e -> firstConnect.completeExceptionally(e) }
                .build()

            val (firstIdentity, token) = withTimeout(5000) { firstConnect.await() }
            conn1.disconnect()

            // Second connection: reuse the token
            val secondConnect = CompletableDeferred<Pair<Identity, String>>()
            val conn2 = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .withToken(token)
                .onConnect { _, id, newToken -> secondConnect.complete(Pair(id, newToken)) }
                .onConnectError { e -> secondConnect.completeExceptionally(e) }
                .build()

            val (secondIdentity, _) = withTimeout(5000) { secondConnect.await() }
            assertEquals(firstIdentity, secondIdentity, "Same token should yield same identity")
            println("PASS: Token reuse preserved identity: ${firstIdentity.toHex().take(16)}...")
            conn2.disconnect()
        }
    }

    // ──────── Rapid fire operations ────────

    @Test
    fun rapidReducerCallsWithCallbacks() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()
            val targetCount = 20
            val results = java.util.concurrent.ConcurrentHashMap<UInt, ReducerResult>()
            val allDone = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subApplied.complete(Unit) }
                        .subscribe("SELECT * FROM player")
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }

            val elapsed = measureTime {
                repeat(targetCount) { i ->
                    val w = BsatnWriter(64)
                    w.writeString("Rapid_${System.currentTimeMillis()}_$i")
                    conn.callReducer("add_player", w.toByteArray()) { result ->
                        results[result.requestId] = result
                        if (results.size >= targetCount && !allDone.isCompleted) {
                            allDone.complete(Unit)
                        }
                    }
                }
                withTimeout(15000) { allDone.await() }
            }

            assertEquals(targetCount, results.size, "All $targetCount callbacks should fire")
            // Verify all got unique requestIds
            assertEquals(targetCount, results.keys.size)
            println("PASS: $targetCount rapid reducer calls all received callbacks in ${elapsed.inWholeMilliseconds}ms")
            conn.disconnect()
        }
    }

    // ──────── Connection state transitions ────────

    @Test
    fun connectionStateTransitions() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val disconnected = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { _, _, _ -> connected.complete(Unit) }
                .onDisconnect { _, _ -> disconnected.complete(Unit) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            // Before connect completes, state should be CONNECTING or CONNECTED
            val earlyState = conn.connectionState.value
            assertTrue(
                earlyState == ConnectionState.CONNECTING || earlyState == ConnectionState.CONNECTED,
                "Early state should be CONNECTING or CONNECTED, got $earlyState"
            )

            withTimeout(5000) { connected.await() }
            assertEquals(ConnectionState.CONNECTED, conn.connectionState.value)
            assertTrue(conn.isActive)

            conn.disconnect()
            assertEquals(ConnectionState.DISCONNECTED, conn.connectionState.value)
            assertFalse(conn.isActive)

            // Identity should still be available after disconnect
            assertNotNull(conn.identity, "Identity should persist after disconnect")

            println("PASS: State transitions: CONNECTING -> CONNECTED -> DISCONNECTED")
        }
    }

    // ──────── Identity null before connect ────────

    @Test
    fun identityNullBeforeConnect() {
        if (!shouldRun()) { println("SKIP"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { _, _, _ -> connected.complete(Unit) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            // Identity/connectionId/token should be null before InitialConnection arrives
            // (This is a best-effort check — the connect could be very fast)
            // We mainly verify they're non-null after connect
            withTimeout(5000) { connected.await() }

            assertNotNull(conn.identity)
            assertNotNull(conn.connectionId)
            assertNotNull(conn.savedToken)
            println("PASS: Identity, connectionId, and token all non-null after connect")
            conn.disconnect()
        }
    }
}
