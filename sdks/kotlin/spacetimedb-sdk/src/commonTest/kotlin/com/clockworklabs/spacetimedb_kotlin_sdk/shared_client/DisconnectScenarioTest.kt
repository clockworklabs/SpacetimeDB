package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertIs
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class DisconnectScenarioTest {

    // =========================================================================
    // Disconnect-During-Transaction Scenarios
    // =========================================================================

    @Test
    fun `disconnect during pending one off query fails callback`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callbackResult: OneOffQueryResult? = null
        conn.oneOffQuery("SELECT * FROM sample") { result ->
            callbackResult = result
        }
        advanceUntilIdle()

        // Disconnect before the server responds
        conn.disconnect()
        advanceUntilIdle()

        // Callback should have been invoked with an error
        val result = assertNotNull(callbackResult)
        assertIs<SdkResult.Failure<QueryError>>(result)
    }

    @Test
    fun `disconnect during pending suspend one off query throws`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var queryResult: OneOffQueryResult? = null
        var queryError: Throwable? = null
        launch {
            try {
                queryResult = conn.oneOffQuery("SELECT * FROM sample")
            } catch (e: Throwable) {
                queryError = e
            }
        }
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // The query must not hang silently — it must resolve on disconnect.
        // failPendingOperations delivers an error result via the callback.
        if (queryResult != null) {
            assertIs<SdkResult.Failure<QueryError>>(queryResult, "Disconnect should produce SdkResult.Failure")
        } else {
            assertNotNull(queryError, "Suspended oneOffQuery must resolve on disconnect — got neither result nor error")
        }
        conn.disconnect()
    }

    @Test
    fun `server close during multiple pending operations`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Create multiple pending operations
        val subHandle = conn.subscribe(listOf("SELECT * FROM t"))
        var reducerFired = false
        conn.callReducer("add", byteArrayOf(), "args", callback = { _ -> reducerFired = true })
        var queryResult: OneOffQueryResult? = null
        conn.oneOffQuery("SELECT 1") { queryResult = it }
        advanceUntilIdle()

        // Server closes connection
        transport.closeFromServer()
        advanceUntilIdle()

        // All pending operations should be cleaned up
        assertTrue(subHandle.isEnded)
        assertFalse(reducerFired) // Reducer callback never fires — it was discarded
        val qResult = assertNotNull(queryResult) // One-off query callback fires with error
        assertIs<SdkResult.Failure<QueryError>>(qResult)
    }

    @Test
    fun `transaction update during disconnect does not crash`() = runTest {
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

        // Send a transaction update and immediately close
        transport.sendToClient(
            transactionUpdateMsg(
                handle.querySetId,
                "sample",
                inserts = buildRowList(SampleRow(2, "Bob").encode()),
            )
        )
        transport.closeFromServer()
        advanceUntilIdle()

        // Should not crash — the transaction update may or may not have been processed
        assertFalse(conn.isActive)
    }

    // =========================================================================
    // Concurrent / racing disconnect
    // =========================================================================

    @Test
    fun `disconnect while connecting does not crash`() = runTest {
        // Use a transport that suspends forever in connect()
        val suspendingTransport = object : Transport {
            override suspend fun connect() {
                kotlinx.coroutines.awaitCancellation()
            }
            override suspend fun send(message: ClientMessage) {}
            override fun incoming(): kotlinx.coroutines.flow.Flow<ServerMessage> =
                kotlinx.coroutines.flow.emptyFlow()
            override suspend fun disconnect() {}
        }

        val conn = DbConnection(
            transport = suspendingTransport,
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = emptyList(),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )

        // Start connecting in a background job — it will suspend in transport.connect()
        val connectJob = launch { conn.connect() }
        advanceUntilIdle()

        // Disconnect while connect() is still suspended
        conn.disconnect()
        advanceUntilIdle()

        assertFalse(conn.isActive)
        connectJob.cancel()
    }

    @Test
    fun `multiple sequential disconnects fire callback only once`() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        // Three rapid sequential disconnects
        conn.disconnect()
        conn.disconnect()
        conn.disconnect()
        advanceUntilIdle()

        assertEquals(1, disconnectCount)
    }

    @Test
    fun `disconnect during subscribe applied processing`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        // Queue a SubscribeApplied then immediately disconnect
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "Alice").encode())))
                ),
            )
        )
        conn.disconnect()
        advanceUntilIdle()

        // Connection must be closed; cache state depends on timing but must be consistent
        assertFalse(conn.isActive)
    }

    @Test
    fun `disconnect clears client cache completely`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows(
                            "sample",
                            buildRowList(
                                SampleRow(1, "Alice").encode(),
                                SampleRow(2, "Bob").encode(),
                            )
                        )
                    )
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())

        conn.disconnect()
        advanceUntilIdle()

        // disconnect() must clear the cache
        assertEquals(0, cache.count())
    }

    @Test
    fun `disconnect clears indexes consistently with cache`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)

        val uniqueIndex = UniqueIndex(cache) { it.id }
        val btreeIndex = BTreeIndex(cache) { it.name }

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows(
                            "sample",
                            buildRowList(
                                SampleRow(1, "Alice").encode(),
                                SampleRow(2, "Bob").encode(),
                            )
                        )
                    )
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())
        assertNotNull(uniqueIndex.find(1))
        assertNotNull(uniqueIndex.find(2))
        assertEquals(1, btreeIndex.filter("Alice").size)

        // Send a transaction inserting a new row, then immediately disconnect.
        // Before the fix, the receive loop could complete the CAS (adding the row
        // and firing internal index listeners) but then disconnect() would clear
        // _rows before the indexes were also cleared — leaving stale index entries.
        transport.sendToClient(
            transactionUpdateMsg(
                handle.querySetId,
                "sample",
                inserts = buildRowList(SampleRow(3, "Charlie").encode()),
            )
        )
        conn.disconnect()
        advanceUntilIdle()

        // After disconnect, cache and indexes must be consistent:
        // either both have the row or neither does.
        assertEquals(0, cache.count(), "Cache should be cleared after disconnect")
        assertNull(uniqueIndex.find(1), "UniqueIndex should be cleared after disconnect")
        assertNull(uniqueIndex.find(2), "UniqueIndex should be cleared after disconnect")
        assertNull(uniqueIndex.find(3), "UniqueIndex should not have stale entries after disconnect")
        assertTrue(btreeIndex.filter("Alice").isEmpty(), "BTreeIndex should be cleared after disconnect")
        assertTrue(btreeIndex.filter("Bob").isEmpty(), "BTreeIndex should be cleared after disconnect")
        assertTrue(btreeIndex.filter("Charlie").isEmpty(), "BTreeIndex should not have stale entries after disconnect")
    }

    @Test
    fun `server close followed by client disconnect does not double fail pending`() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Fire a reducer call so there's a pending callback
        conn.callReducer("test", byteArrayOf(1), "args")
        advanceUntilIdle()

        // Server closes, then client also calls disconnect
        transport.closeFromServer()
        conn.disconnect()
        advanceUntilIdle()

        // Callback fires at most once
        assertEquals(1, disconnectCount)
        assertFalse(conn.isActive)
    }

    // =========================================================================
    // Reconnection (new connection after old one is closed)
    // =========================================================================

    @Test
    fun `fresh connection works after previous disconnect`() = runTest {
        val transport1 = FakeTransport()
        val conn1 = buildTestConnection(transport1, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport1.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertTrue(conn1.isActive)
        assertEquals(TEST_IDENTITY, conn1.identity)

        conn1.disconnect()
        advanceUntilIdle()
        assertFalse(conn1.isActive)

        // Build a completely new connection (the "reconnect by rebuilding" pattern)
        val transport2 = FakeTransport()
        val secondIdentity = Identity(BigInteger.TEN)
        val secondConnectionId = ConnectionId(BigInteger(20))
        var conn2ConnectFired = false
        val conn2 = buildTestConnection(transport2, onConnect = { _, _, _ ->
            conn2ConnectFired = true
        }, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport2.sendToClient(
            ServerMessage.InitialConnection(
                identity = secondIdentity,
                connectionId = secondConnectionId,
                token = "new-token",
            )
        )
        advanceUntilIdle()

        assertTrue(conn2.isActive)
        assertTrue(conn2ConnectFired)
        assertEquals(secondIdentity, conn2.identity)

        // Old connection must remain closed
        assertFalse(conn1.isActive)
        conn2.disconnect()
    }

    @Test
    fun `fresh connection cache is independent from old`() = runTest {
        val transport1 = FakeTransport()
        val conn1 = buildTestConnection(transport1, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache1 = createSampleCache()
        conn1.clientCache.register("sample", cache1)
        transport1.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row via first connection
        val handle1 = conn1.subscribe(listOf("SELECT * FROM sample"))
        transport1.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "Alice").encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache1.count())

        conn1.disconnect()
        advanceUntilIdle()

        // Second connection has its own empty cache
        val transport2 = FakeTransport()
        val conn2 = buildTestConnection(transport2, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        val cache2 = createSampleCache()
        conn2.clientCache.register("sample", cache2)
        transport2.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(0, cache2.count())
        conn2.disconnect()
    }

    // =========================================================================
    // sendMessage after disconnect — graceful failure (no crash)
    // =========================================================================

    @Test
    fun `send message after disconnect does not crash`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        conn.disconnect()
        advanceUntilIdle()
        assertFalse(conn.isActive)

        // Attempting to send after disconnect logs a warning and returns — no throw
        conn.callReducer("add", byteArrayOf(), "args")
        // No exception means success
    }

    @Test
    fun `send message on closed channel does not crash`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        // Server closes the connection
        transport.closeFromServer()
        advanceUntilIdle()

        // Any send attempt after server close logs a warning — no throw
        conn.oneOffQuery("SELECT 1") {}
    }

    @Test
    fun `reducer callback does not fire on failed send`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, exceptionHandler = CoroutineExceptionHandler { _, _ -> })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // callReducer returns without throwing — the callback is registered but
        // will never fire since the message was not sent and the connection is closed.
        var callbackFired = false
        conn.callReducer("add", byteArrayOf(), "args", callback = { _ ->
            callbackFired = true
        })
        advanceUntilIdle()

        assertFalse(callbackFired)
    }
}
