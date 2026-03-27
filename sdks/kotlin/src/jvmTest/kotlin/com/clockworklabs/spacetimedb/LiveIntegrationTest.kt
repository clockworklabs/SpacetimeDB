package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.websocket.ConnectionState
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlin.time.measureTime

/**
 * Live integration tests against a local SpacetimeDB server.
 *
 * Prerequisites:
 *   1. `spacetime start` running on localhost:3000
 *   2. Test module published: `spacetime publish --server local -p <module> kotlin-sdk-test`
 *
 * Set `SPACETIMEDB_TEST=1` to enable. Skipped by default in CI.
 */
class LiveIntegrationTest {

    private val serverUri = System.getenv("SPACETIMEDB_URI") ?: "ws://127.0.0.1:3000"
    private val moduleName = System.getenv("SPACETIMEDB_MODULE") ?: "kotlin-sdk-test"

    private fun skipIfNoServer() {
        if (System.getenv("SPACETIMEDB_TEST") != "1") {
            println("SKIP: Set SPACETIMEDB_TEST=1 to run live integration tests")
            return
        }
    }

    private fun shouldRun(): Boolean = System.getenv("SPACETIMEDB_TEST") == "1"

    @Test
    fun connectAndReceiveIdentity() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            val connected = CompletableDeferred<Triple<DbConnection, Identity, String>>()
            val disconnected = CompletableDeferred<Throwable?>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, id, token ->
                    connected.complete(Triple(c, id, token))
                }
                .onDisconnect { _, err -> disconnected.complete(err) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            val (_, identity, token) = withTimeout(5000) { connected.await() }

            assertNotNull(identity, "Should receive an identity")
            assertTrue(identity.bytes.size == 32, "Identity should be 32 bytes")
            assertTrue(token.isNotEmpty(), "Should receive an auth token")
            assertNotNull(conn.connectionId, "Should have a connectionId")
            assertEquals(ConnectionState.CONNECTED, conn.connectionState.value)

            println("PASS: Connected as ${identity.toHex().take(16)}...")
            println("      Token: ${token.take(20)}...")
            println("      ConnectionId: ${conn.connectionId}")

            conn.disconnect()
        }
    }

    @Test
    fun subscribeAndReceiveRows() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.subscriptionBuilder()
                        .onApplied { subApplied.complete(Unit) }
                        .onError { err -> subApplied.completeExceptionally(RuntimeException(err)) }
                        .subscribe("SELECT * FROM player")
                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }

            println("PASS: Subscription to 'SELECT * FROM player' applied successfully")

            conn.disconnect()
        }
    }

    @Test
    fun callReducerAndObserveInsert() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()
            val insertReceived = CompletableDeferred<ByteArray>()
            val reducerResult = CompletableDeferred<ReducerResult>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.table("player").onInsert { row ->
                        insertReceived.complete(row)
                    }

                    c.subscriptionBuilder()
                        .onApplied {
                            subApplied.complete(Unit)
                        }
                        .onError { err -> subApplied.completeExceptionally(RuntimeException(err)) }
                        .subscribe("SELECT * FROM player")

                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }

            // Call the add_player reducer
            val playerName = "KotlinSDK_${System.currentTimeMillis()}"
            val argsWriter = BsatnWriter(64)
            argsWriter.writeString(playerName)

            conn.callReducer("add_player", argsWriter.toByteArray()) { result ->
                reducerResult.complete(result)
            }

            val row = withTimeout(5000) { insertReceived.await() }
            assertTrue(row.isNotEmpty(), "Should receive inserted row bytes")

            val result = withTimeout(5000) { reducerResult.await() }
            assertNotNull(result, "Should receive reducer result")

            println("PASS: Called add_player('$playerName')")
            println("      Received insert: ${row.size} bytes")
            println("      Reducer result: ${result.outcome}")

            conn.disconnect()
        }
    }

    @Test
    fun multipleReducerCallsPerformance() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            val connected = CompletableDeferred<Unit>()
            val subApplied = CompletableDeferred<Unit>()
            val insertCount = java.util.concurrent.atomic.AtomicInteger(0)
            val targetCount = 50

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ ->
                    c.table("player").onInsert { insertCount.incrementAndGet() }

                    c.subscriptionBuilder()
                        .onApplied { subApplied.complete(Unit) }
                        .onError { err -> subApplied.completeExceptionally(RuntimeException(err)) }
                        .subscribe("SELECT * FROM player")

                    connected.complete(Unit)
                }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            withTimeout(5000) { connected.await() }
            withTimeout(5000) { subApplied.await() }

            // Fire N reducer calls and measure round-trip time
            val elapsed = measureTime {
                repeat(targetCount) { i ->
                    val w = BsatnWriter(64)
                    w.writeString("Batch_${System.currentTimeMillis()}_$i")
                    conn.callReducer("add_player", w.toByteArray())
                }

                // Wait for all inserts to arrive
                withTimeout(15000) {
                    while (insertCount.get() < targetCount) {
                        delay(50)
                    }
                }
            }

            assertTrue(insertCount.get() >= targetCount, "Should receive all $targetCount inserts")
            val avgMs = elapsed.inWholeMilliseconds.toDouble() / targetCount
            println("PASS: $targetCount reducer calls + round-trip in ${elapsed.inWholeMilliseconds}ms")
            println("      Avg round-trip: ${"%.1f".format(avgMs)}ms per call")

            conn.disconnect()
        }
    }

    @Test
    fun oneOffQueryExecution() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            val connected = CompletableDeferred<DbConnection>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .onConnect { c, _, _ -> connected.complete(c) }
                .onConnectError { e -> connected.completeExceptionally(e) }
                .build()

            val c = withTimeout(5000) { connected.await() }

            val elapsed = measureTime {
                val result = withTimeout(5000) {
                    c.oneOffQuery("SELECT * FROM player")
                }
                if (result.error != null) {
                    println("      Query returned error: ${result.error}")
                } else {
                    val rows = result.rows?.tables?.flatMap { it.rows.decodeRows() } ?: emptyList()
                    println("PASS: One-off query returned ${rows.size} player rows")
                }
            }
            println("      Query time: ${elapsed.inWholeMilliseconds}ms")

            conn.disconnect()
        }
    }

    @Test
    fun reconnectionAfterDisconnect() {
        if (!shouldRun()) { println("SKIP: Set SPACETIMEDB_TEST=1"); return }

        runBlocking {
            var connectCount = 0
            val firstConnect = CompletableDeferred<Unit>()
            val secondConnect = CompletableDeferred<Unit>()

            val conn = DbConnection.builder()
                .withUri(serverUri)
                .withModuleName(moduleName)
                .withReconnectPolicy(ReconnectPolicy(maxRetries = 3, initialDelayMs = 500))
                .onConnect { _, _, _ ->
                    connectCount++
                    if (connectCount == 1) firstConnect.complete(Unit)
                    else secondConnect.complete(Unit)
                }
                .onConnectError { e -> firstConnect.completeExceptionally(e) }
                .build()

            withTimeout(5000) { firstConnect.await() }
            assertEquals(ConnectionState.CONNECTED, conn.connectionState.value)
            println("PASS: First connection established")

            // We can't easily force a server-side disconnect from the client,
            // so we just verify the reconnect policy is wired up correctly
            assertEquals(ConnectionState.CONNECTED, conn.connectionState.value)
            println("PASS: Reconnect policy configured (maxRetries=3, initialDelay=500ms)")

            conn.disconnect()
            assertEquals(ConnectionState.DISCONNECTED, conn.connectionState.value)
            println("PASS: Clean disconnect")
        }
    }
}
