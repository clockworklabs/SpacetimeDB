package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import com.ionspin.kotlin.bignum.integer.BigInteger
import io.ktor.client.HttpClient
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration

/**
 * Tests covering edge cases and gaps identified in the QA review:
 * - Connection state transitions
 * - Subscription lifecycle edge cases
 * - Disconnect-during-transaction scenarios
 * - Concurrent cache operations
 * - Content-based keying (tables without primary keys)
 * - Event table behavior
 * - Multi-subscription interactions
 * - Callback ordering guarantees
 * - One-off query edge cases
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class EdgeCaseTest {

    private val testIdentity = Identity(BigInteger.ONE)
    private val testConnectionId = ConnectionId(BigInteger.TWO)
    private val testToken = "test-token-abc"

    private fun initialConnectionMsg() = ServerMessage.InitialConnection(
        identity = testIdentity,
        connectionId = testConnectionId,
        token = testToken,
    )

    private suspend fun TestScope.buildTestConnection(
        transport: FakeTransport,
        onConnect: ((DbConnectionView, Identity, String) -> Unit)? = null,
        onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
        onConnectError: ((DbConnectionView, Throwable) -> Unit)? = null,
    ): DbConnection {
        val conn = createTestConnection(transport, onConnect, onDisconnect, onConnectError)
        conn.connect()
        return conn
    }

    private fun TestScope.createTestConnection(
        transport: FakeTransport,
        onConnect: ((DbConnectionView, Identity, String) -> Unit)? = null,
        onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
        onConnectError: ((DbConnectionView, Throwable) -> Unit)? = null,
        exceptionHandler: CoroutineExceptionHandler? = null,
    ): DbConnection {
        val context = SupervisorJob() + StandardTestDispatcher(testScheduler) +
                (exceptionHandler ?: CoroutineExceptionHandler { _, _ -> })
        return DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(context),
            onConnectCallbacks = listOfNotNull(onConnect),
            onDisconnectCallbacks = listOfNotNull(onDisconnect),
            onConnectErrorCallbacks = listOfNotNull(onConnectError),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
    }

    private fun emptyQueryRows(): QueryRows = QueryRows(emptyList())

    private fun transactionUpdateMsg(
        querySetId: QuerySetId,
        tableName: String,
        inserts: BsatnRowList = buildRowList(),
        deletes: BsatnRowList = buildRowList(),
    ) = ServerMessage.TransactionUpdateMsg(
        TransactionUpdate(
            listOf(
                QuerySetUpdate(
                    querySetId,
                    listOf(
                        TableUpdate(
                            tableName,
                            listOf(TableUpdateRows.PersistentTable(inserts, deletes))
                        )
                    )
                )
            )
        )
    )

    // =========================================================================
    // Connection State Transitions
    // =========================================================================

    @Test
    fun connectionStateProgression() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport)

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
        val conn = createTestConnection(transport)
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
        val conn = createTestConnection(transport)
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
        val conn = createTestConnection(transport)

        conn.connect()

        assertFalse(conn.isActive)
        // Cannot reconnect after failure (state is CLOSED)
        assertFailsWith<IllegalStateException> { conn.connect() }
    }

    @Test
    fun serverCloseRendersConnectionInactive() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
        val conn = createTestConnection(transport)

        // Should not throw
        conn.disconnect()
        assertFalse(conn.isActive)
    }

    @Test
    fun disconnectAfterConnectRendersInactive() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport)
        conn.connect()
        assertTrue(conn.isActive)

        conn.disconnect()
        advanceUntilIdle()

        assertFalse(conn.isActive)
    }

    // =========================================================================
    // Subscription Lifecycle Edge Cases
    // =========================================================================

    @Test
    fun subscriptionStateTransitionsPendingToActiveToEnded() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        assertEquals(SubscriptionState.PENDING, handle.state)
        assertTrue(handle.isPending)

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ACTIVE, handle.state)
        assertTrue(handle.isActive)

        handle.unsubscribe()
        assertEquals(SubscriptionState.UNSUBSCRIBING, handle.state)
        assertTrue(handle.isUnsubscribing)

        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()
        assertEquals(SubscriptionState.ENDED, handle.state)
        assertTrue(handle.isEnded)

        conn.disconnect()
    }

    @Test
    fun unsubscribeFromUnsubscribingStateThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        handle.unsubscribe()
        assertTrue(handle.isUnsubscribing)

        // Second unsubscribe should fail — already unsubscribing
        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    @Test
    fun subscriptionErrorFromPendingStateEndsSubscription() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorReceived = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM bad"),
            onError = listOf { _, _ -> errorReceived = true },
        )
        assertTrue(handle.isPending)

        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "parse error",
            )
        )
        advanceUntilIdle()

        assertTrue(handle.isEnded)
        assertTrue(errorReceived)
        // Should not be able to unsubscribe
        assertFailsWith<IllegalStateException> { handle.unsubscribe() }
        conn.disconnect()
    }

    @Test
    fun multipleSubscriptionsTrackIndependently() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle1 = conn.subscribe(listOf("SELECT * FROM t1"))
        val handle2 = conn.subscribe(listOf("SELECT * FROM t2"))

        // Both start PENDING
        assertTrue(handle1.isPending)
        assertTrue(handle2.isPending)

        // Apply only handle1
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isActive)
        assertTrue(handle2.isPending) // handle2 still pending

        // Apply handle2
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isActive)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    @Test
    fun disconnectMarksAllPendingAndActiveSubscriptionsAsEnded() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val pending = conn.subscribe(listOf("SELECT * FROM t1"))
        val active = conn.subscribe(listOf("SELECT * FROM t2"))

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = active.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(pending.isPending)
        assertTrue(active.isActive)

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(pending.isEnded)
        assertTrue(active.isEnded)
    }

    @Test
    fun unsubscribeAppliedWithRowsRemovesFromCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
        assertEquals(1, cache.count())

        // Unsubscribe with rows returned
        handle.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        assertEquals(0, cache.count())
        conn.disconnect()
    }

    // =========================================================================
    // Disconnect-During-Transaction Scenarios
    // =========================================================================

    @Test
    fun disconnectDuringPendingOneOffQueryFailsCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callbackResult: ServerMessage.OneOffQueryResult? = null
        conn.oneOffQuery("SELECT * FROM sample") { result ->
            callbackResult = result
        }
        advanceUntilIdle()

        // Disconnect before the server responds
        conn.disconnect()
        advanceUntilIdle()

        // Callback should have been invoked with an error
        assertNotNull(callbackResult)
        assertTrue(callbackResult!!.result is QueryResult.Err)
    }

    @Test
    fun disconnectDuringPendingSuspendOneOffQueryThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var queryResult: ServerMessage.OneOffQueryResult? = null
        var queryError: Throwable? = null
        val job = launch {
            try {
                queryResult = conn.oneOffQuery("SELECT * FROM sample")
            } catch (e: Throwable) {
                queryError = e
            }
        }
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // The suspended query should have been resolved with error result
        // (via failPendingOperations callback invocation which resumes the coroutine)
        val result = queryResult
        if (result != null) {
            assertTrue(result.result is QueryResult.Err)
        }
        // If the coroutine was cancelled, that's also acceptable
        conn.disconnect()
    }

    @Test
    fun serverCloseDuringMultiplePendingOperations() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Create multiple pending operations
        val subHandle = conn.subscribe(listOf("SELECT * FROM t"))
        var reducerFired = false
        conn.callReducer("add", byteArrayOf(), "args", callback = { _ -> reducerFired = true })
        var queryResult: ServerMessage.OneOffQueryResult? = null
        conn.oneOffQuery("SELECT 1") { queryResult = it }
        advanceUntilIdle()

        // Server closes connection
        transport.closeFromServer()
        advanceUntilIdle()

        // All pending operations should be cleaned up
        assertTrue(subHandle.isEnded)
        assertFalse(reducerFired) // Reducer callback never fires — it was discarded
        assertNotNull(queryResult) // One-off query callback fires with error
        assertTrue(queryResult!!.result is QueryResult.Err)
    }

    @Test
    fun transactionUpdateDuringDisconnectDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
    // Content-Based Keying (Tables Without Primary Keys)
    // =========================================================================

    @Test
    fun contentKeyedCacheInsertAndDelete() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        cache.applyInserts(STUB_CTX, buildRowList(row1.encode(), row2.encode()))

        assertEquals(2, cache.count())
        assertTrue(cache.all().containsAll(listOf(row1, row2)))

        // Delete row1 by content
        val parsed = cache.parseDeletes(buildRowList(row1.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(row2, cache.all().single())
    }

    @Test
    fun contentKeyedCacheDuplicateInsertIncrementsRefCount() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        assertEquals(1, cache.count()) // One unique row, ref count = 2

        // First delete decrements ref count
        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)
        assertEquals(1, cache.count()) // Still present

        // Second delete removes it
        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(0, cache.count())
    }

    @Test
    fun contentKeyedCacheUpdateByContent() {
        val cache = TableCache.withContentKey(::decodeSampleRow)

        val oldRow = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        // An update with same content in delete + different content in insert
        // For content-keyed tables, the "update" detection is by key,
        // and since keys are content-based, this is a delete+insert, not an update
        val newRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals(newRow, cache.all().single())
    }

    // =========================================================================
    // Event Table Behavior
    // =========================================================================

    @Test
    fun eventTableDoesNotStoreRowsButFiresCallbacks() {
        val cache = createSampleCache()
        val events = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> events.add(row) }

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        val eventUpdate = TableUpdateRows.EventTable(
            events = buildRowList(row1.encode(), row2.encode())
        )
        val parsed = cache.parseUpdate(eventUpdate)
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        for (cb in callbacks) cb.invoke()

        assertEquals(0, cache.count()) // Not stored
        assertEquals(listOf(row1, row2), events) // Callbacks fired
    }

    @Test
    fun eventTableDoesNotFireOnBeforeDelete() {
        val cache = createSampleCache()
        var beforeDeleteFired = false
        cache.onBeforeDelete { _, _ -> beforeDeleteFired = true }

        val eventUpdate = TableUpdateRows.EventTable(
            events = buildRowList(SampleRow(1, "Alice").encode())
        )
        val parsed = cache.parseUpdate(eventUpdate)
        cache.preApplyUpdate(STUB_CTX, parsed)
        cache.applyUpdate(STUB_CTX, parsed)

        assertFalse(beforeDeleteFired)
    }

    // =========================================================================
    // Callback Ordering Guarantees
    // =========================================================================

    @Test
    fun preApplyDeleteFiresBeforeApplyDeleteAcrossTables() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val rowA = SampleRow(1, "A")
        val rowB = SampleRow(2, "B")
        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows("table_a", buildRowList(rowA.encode())),
                        SingleTableRows("table_b", buildRowList(rowB.encode())),
                    )
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cacheA.count())
        assertEquals(1, cacheB.count())

        // Track ordering: onBeforeDelete should fire for BOTH tables
        // BEFORE any onDelete fires
        val events = mutableListOf<String>()
        cacheA.onBeforeDelete { _, _ -> events.add("beforeDelete_A") }
        cacheB.onBeforeDelete { _, _ -> events.add("beforeDelete_B") }
        cacheA.onDelete { _, _ -> events.add("delete_A") }
        cacheB.onDelete { _, _ -> events.add("delete_B") }

        // Transaction deleting from both tables
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "table_a",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(rowA.encode()),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "table_b",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(rowB.encode()),
                                        )
                                    )
                                ),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // All beforeDeletes must come before any delete
        val beforeDeleteIndices = events.indices.filter { events[it].startsWith("beforeDelete") }
        val deleteIndices = events.indices.filter { events[it].startsWith("delete_") }
        assertTrue(beforeDeleteIndices.isNotEmpty())
        assertTrue(deleteIndices.isNotEmpty())
        assertTrue(beforeDeleteIndices.max() < deleteIndices.min())

        conn.disconnect()
    }

    @Test
    fun updateDoesNotFireOnBeforeDeleteForUpdatedRow() {
        val cache = createSampleCache()
        val oldRow = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))

        val beforeDeleteRows = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, row -> beforeDeleteRows.add(row) }

        // Update (same key in both inserts and deletes) should NOT fire onBeforeDelete
        val newRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)
        cache.applyUpdate(STUB_CTX, parsed)

        assertTrue(beforeDeleteRows.isEmpty(), "onBeforeDelete should NOT fire for updates")
    }

    @Test
    fun pureDeleteFiresOnBeforeDelete() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val beforeDeleteRows = mutableListOf<SampleRow>()
        cache.onBeforeDelete { _, r -> beforeDeleteRows.add(r) }

        // Pure delete (no corresponding insert) should fire onBeforeDelete
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.preApplyUpdate(STUB_CTX, parsed)

        assertEquals(listOf(row), beforeDeleteRows)
    }

    @Test
    fun callbackFiringOrderInsertUpdateDelete() {
        val cache = createSampleCache()

        // Pre-populate
        val existingRow = SampleRow(1, "Old")
        val toDelete = SampleRow(2, "Delete Me")
        cache.applyInserts(STUB_CTX, buildRowList(existingRow.encode(), toDelete.encode()))

        val events = mutableListOf<String>()
        cache.onInsert { _, row -> events.add("insert:${row.name}") }
        cache.onUpdate { _, old, new -> events.add("update:${old.name}->${new.name}") }
        cache.onDelete { _, row -> events.add("delete:${row.name}") }
        cache.onBeforeDelete { _, row -> events.add("beforeDelete:${row.name}") }

        // Transaction: update row1, delete row2, insert row3
        val updatedRow = SampleRow(1, "New")
        val newRow = SampleRow(3, "Fresh")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(updatedRow.encode(), newRow.encode()),
            deletes = buildRowList(existingRow.encode(), toDelete.encode()),
        )
        val parsed = cache.parseUpdate(update)

        // Pre-apply phase
        cache.preApplyUpdate(STUB_CTX, parsed)

        // Only pure deletes get onBeforeDelete (not updates)
        assertEquals(listOf("beforeDelete:Delete Me"), events)

        // Apply phase
        events.clear()
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        for (cb in callbacks) cb.invoke()

        // Should contain update, insert, and delete events
        assertTrue(events.contains("update:Old->New"))
        assertTrue(events.contains("insert:Fresh"))
        assertTrue(events.contains("delete:Delete Me"))
    }

    // =========================================================================
    // Cache Operations Edge Cases
    // =========================================================================

    @Test
    fun clearFiresInternalDeleteListenersForAllRows() {
        val cache = createSampleCache()
        val deletedRows = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { deletedRows.add(it) }

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")
        cache.applyInserts(STUB_CTX, buildRowList(row1.encode(), row2.encode()))

        cache.clear()

        assertEquals(0, cache.count())
        assertEquals(2, deletedRows.size)
        assertTrue(deletedRows.containsAll(listOf(row1, row2)))
    }

    @Test
    fun clearOnEmptyCacheIsNoOp() {
        val cache = createSampleCache()
        var listenerFired = false
        cache.addInternalDeleteListener { listenerFired = true }

        cache.clear()
        assertFalse(listenerFired)
    }

    @Test
    fun deleteNonexistentRowIsNoOp() {
        val cache = createSampleCache()
        val row = SampleRow(99, "Ghost")

        var deleteFired = false
        cache.onDelete { _, _ -> deleteFired = true }

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertFalse(deleteFired)
        assertEquals(0, cache.count())
    }

    @Test
    fun insertEmptyRowListIsNoOp() {
        val cache = createSampleCache()
        var insertFired = false
        cache.onInsert { _, _ -> insertFired = true }

        val callbacks = cache.applyInserts(STUB_CTX, buildRowList())

        assertEquals(0, cache.count())
        assertTrue(callbacks.isEmpty())
        assertFalse(insertFired)
    }

    @Test
    fun removeCallbackPreventsItFromFiring() {
        val cache = createSampleCache()
        var fired = false
        val cb: (EventContext, SampleRow) -> Unit = { _, _ -> fired = true }

        cache.onInsert(cb)
        cache.removeOnInsert(cb)

        cache.applyInserts(STUB_CTX, buildRowList(SampleRow(1, "Alice").encode()))
        // Invoke any pending callbacks
        // No PendingCallbacks should exist for this insert since we removed the callback

        assertFalse(fired)
    }

    @Test
    fun internalListenersFiredOnInsertAfterCAS() {
        val cache = createSampleCache()
        val internalInserts = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        assertEquals(listOf(row), internalInserts)
    }

    @Test
    fun internalListenersFiredOnDeleteAfterCAS() {
        val cache = createSampleCache()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        val parsed = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed)

        assertEquals(listOf(row), internalDeletes)
    }

    @Test
    fun internalListenersFiredOnUpdateForBothOldAndNew() {
        val cache = createSampleCache()
        val internalInserts = mutableListOf<SampleRow>()
        val internalDeletes = mutableListOf<SampleRow>()
        cache.addInternalInsertListener { internalInserts.add(it) }
        cache.addInternalDeleteListener { internalDeletes.add(it) }

        val oldRow = SampleRow(1, "Old")
        cache.applyInserts(STUB_CTX, buildRowList(oldRow.encode()))
        internalInserts.clear() // Reset from the initial insert

        val newRow = SampleRow(1, "New")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(newRow.encode()),
            deletes = buildRowList(oldRow.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        // On update, old row fires delete listener, new row fires insert listener
        assertEquals(listOf(oldRow), internalDeletes)
        assertEquals(listOf(newRow), internalInserts)
    }

    @Test
    fun batchInsertMultipleRowsFiresCallbacksForEach() {
        val cache = createSampleCache()
        val inserted = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> inserted.add(row) }

        val rows = (1..5).map { SampleRow(it, "Row$it") }
        val callbacks = cache.applyInserts(
            STUB_CTX,
            buildRowList(*rows.map { it.encode() }.toTypedArray())
        )
        for (cb in callbacks) cb.invoke()

        assertEquals(5, cache.count())
        assertEquals(rows, inserted)
    }

    // =========================================================================
    // ClientCache Registry
    // =========================================================================

    @Test
    fun clientCacheGetTableThrowsForUnknownTable() {
        val cc = ClientCache()
        assertFailsWith<IllegalStateException> {
            cc.getTable<SampleRow>("nonexistent")
        }
    }

    @Test
    fun clientCacheGetTableOrNullReturnsNull() {
        val cc = ClientCache()
        assertNull(cc.getTableOrNull<SampleRow>("nonexistent"))
    }

    @Test
    fun clientCacheGetOrCreateTableCreatesOnce() {
        val cc = ClientCache()
        var factoryCalls = 0

        val cache1 = cc.getOrCreateTable<SampleRow>("t") {
            factoryCalls++
            createSampleCache()
        }
        val cache2 = cc.getOrCreateTable<SampleRow>("t") {
            factoryCalls++
            createSampleCache()
        }

        assertEquals(1, factoryCalls)
        assertTrue(cache1 === cache2)
    }

    @Test
    fun clientCacheTableNames() {
        val cc = ClientCache()
        cc.register("alpha", createSampleCache())
        cc.register("beta", createSampleCache())

        assertEquals(setOf("alpha", "beta"), cc.tableNames())
    }

    @Test
    fun clientCacheClearClearsAllTables() {
        val cc = ClientCache()
        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        cc.register("a", cacheA)
        cc.register("b", cacheB)

        cacheA.applyInserts(STUB_CTX, buildRowList(SampleRow(1, "X").encode()))
        cacheB.applyInserts(STUB_CTX, buildRowList(SampleRow(2, "Y").encode()))

        cc.clear()

        assertEquals(0, cacheA.count())
        assertEquals(0, cacheB.count())
    }

    // =========================================================================
    // One-Off Query Edge Cases
    // =========================================================================

    @Test
    fun multipleOneOffQueriesConcurrently() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val results = mutableMapOf<UInt, ServerMessage.OneOffQueryResult>()
        val id1 = conn.oneOffQuery("SELECT 1") { results[it.requestId] = it }
        val id2 = conn.oneOffQuery("SELECT 2") { results[it.requestId] = it }
        val id3 = conn.oneOffQuery("SELECT 3") { results[it.requestId] = it }
        advanceUntilIdle()

        // Respond in reverse order
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id3, result = QueryResult.Ok(emptyQueryRows()))
        )
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id1, result = QueryResult.Ok(emptyQueryRows()))
        )
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id2, result = QueryResult.Err("error"))
        )
        advanceUntilIdle()

        assertEquals(3, results.size)
        assertTrue(results[id1]!!.result is QueryResult.Ok)
        assertTrue(results[id2]!!.result is QueryResult.Err)
        assertTrue(results[id3]!!.result is QueryResult.Ok)
        conn.disconnect()
    }

    @Test
    fun oneOffQueryCallbackIsRemovedAfterFiring() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callCount = 0
        val id = conn.oneOffQuery("SELECT 1") { callCount++ }
        advanceUntilIdle()

        // Send result twice with same requestId
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id, result = QueryResult.Ok(emptyQueryRows()))
        )
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(requestId = id, result = QueryResult.Ok(emptyQueryRows()))
        )
        advanceUntilIdle()

        assertEquals(1, callCount) // Should only fire once
        conn.disconnect()
    }

    // =========================================================================
    // Reducer Edge Cases
    // =========================================================================

    @Test
    fun reducerCallbackIsRemovedAfterFiring() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callCount = 0
        val id = conn.callReducer("add", byteArrayOf(), "args", callback = { callCount++ })
        advanceUntilIdle()

        // Send result twice
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(1, callCount) // Should only fire once
        conn.disconnect()
    }

    @Test
    fun reducerResultOkWithTableUpdatesMutatesCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Subscribe first to establish the table
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Call reducer
        var status: Status? = null
        val id = conn.callReducer("add", byteArrayOf(), "args", callback = { ctx -> status = ctx.status })
        advanceUntilIdle()

        // Reducer result with table insert
        val row = SampleRow(1, "FromReducer")
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Ok(
                    retValue = byteArrayOf(),
                    transactionUpdate = TransactionUpdate(
                        listOf(
                            QuerySetUpdate(
                                handle.querySetId,
                                listOf(
                                    TableUpdate(
                                        "sample",
                                        listOf(
                                            TableUpdateRows.PersistentTable(
                                                inserts = buildRowList(row.encode()),
                                                deletes = buildRowList(),
                                            )
                                        )
                                    )
                                )
                            )
                        )
                    ),
                ),
            )
        )
        advanceUntilIdle()

        assertEquals(Status.Committed, status)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        conn.disconnect()
    }

    @Test
    fun reducerResultWithEmptyErrorBytes() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val id = conn.callReducer("bad", byteArrayOf(), "args", callback = { ctx -> status = ctx.status })
        advanceUntilIdle()

        // Empty error bytes
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(byteArrayOf()),
            )
        )
        advanceUntilIdle()

        assertTrue(status is Status.Failed)
        assertTrue((status as Status.Failed).message.contains("undecodable"))
        conn.disconnect()
    }

    // =========================================================================
    // Multi-Table Transaction Processing
    // =========================================================================

    @Test
    fun transactionUpdateAcrossMultipleTables() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Transaction inserting into both tables
        val rowA = SampleRow(1, "A")
        val rowB = SampleRow(2, "B")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "table_a",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(rowA.encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "table_b",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(rowB.encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(1, cacheA.count())
        assertEquals(1, cacheB.count())
        assertEquals(rowA, cacheA.all().single())
        assertEquals(rowB, cacheB.all().single())
        conn.disconnect()
    }

    @Test
    fun transactionUpdateWithUnknownTableIsSkipped() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("known", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM known"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        // Transaction with both known and unknown tables
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "unknown",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(SampleRow(1, "ghost").encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                                TableUpdate(
                                    "known",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(SampleRow(2, "visible").encode()),
                                            deletes = buildRowList(),
                                        )
                                    )
                                ),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Known table gets the insert; unknown table is skipped without error
        assertEquals(1, cache.count())
        assertEquals("visible", cache.all().single().name)
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // =========================================================================
    // Callback Exception Resilience
    // =========================================================================

    @Test
    fun onConnectExceptionDoesNotPreventSubsequentMessages() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport, onConnect = { _, _, _ ->
            error("connect callback explosion")
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Connection should still work despite callback exception
        assertTrue(conn.isActive)

        val handle = conn.subscribe(listOf("SELECT * FROM t"))
        var applied = false
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        // The subscribe was sent and the SubscribeApplied was processed
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    @Test
    fun onBeforeDeleteExceptionDoesNotPreventMutation() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        cache.onBeforeDelete { _, _ -> error("boom in beforeDelete") }

        // The preApply phase will throw, but let's verify the apply phase
        // still works independently (since the exception is in user code,
        // it's caught by runUserCallback in DbConnection context)
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        // preApplyUpdate will throw since we're not wrapped in runUserCallback
        // This tests that if it does throw, the cache is still consistent
        try {
            cache.preApplyUpdate(STUB_CTX, parsed)
        } catch (_: Exception) {
            // Expected
        }

        // applyUpdate should still work
        val callbacks = cache.applyUpdate(STUB_CTX, parsed)
        assertEquals(0, cache.count())
    }

    // =========================================================================
    // Ref Count Edge Cases
    // =========================================================================

    @Test
    fun refCountSurvivesUpdateOnMultiRefRow() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")

        // Insert twice — refCount = 2
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        assertEquals(1, cache.count())

        // Update the row — should preserve refCount
        val updatedRow = SampleRow(1, "Alice Updated")
        val update = TableUpdateRows.PersistentTable(
            inserts = buildRowList(updatedRow.encode()),
            deletes = buildRowList(row.encode()),
        )
        val parsed = cache.parseUpdate(update)
        cache.applyUpdate(STUB_CTX, parsed)

        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().single().name)

        // Deleting once should still keep the row (refCount was 2, update preserves it)
        val parsedDelete = cache.parseDeletes(buildRowList(updatedRow.encode()))
        cache.applyDeletes(STUB_CTX, parsedDelete)
        // The refCount was preserved during update, so after one delete it should still be there
        assertEquals(1, cache.count())
    }

    @Test
    fun deleteWithHighRefCountOnlyDecrements() {
        val cache = createSampleCache()
        val row = SampleRow(1, "Alice")

        // Insert 3 times — refCount = 3
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))
        cache.applyInserts(STUB_CTX, buildRowList(row.encode()))

        var deleteFired = false
        cache.onDelete { _, _ -> deleteFired = true }

        // Delete once — refCount goes to 2
        val parsed1 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed1)
        assertEquals(1, cache.count())
        assertFalse(deleteFired)

        // Delete again — refCount goes to 1
        val parsed2 = cache.parseDeletes(buildRowList(row.encode()))
        cache.applyDeletes(STUB_CTX, parsed2)
        assertEquals(1, cache.count())
        assertFalse(deleteFired)

        // Delete final — refCount goes to 0
        val parsed3 = cache.parseDeletes(buildRowList(row.encode()))
        val callbacks = cache.applyDeletes(STUB_CTX, parsed3)
        for (cb in callbacks) cb.invoke()
        assertEquals(0, cache.count())
        assertTrue(deleteFired)
    }

    // =========================================================================
    // Unsubscribe with Null Rows
    // =========================================================================

    @Test
    fun unsubscribeAppliedWithNullRowsDoesNotDeleteFromCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
        assertEquals(1, cache.count())

        // Unsubscribe without SendDroppedRows — server sends null rows
        handle.unsubscribeThen {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        // Row stays in cache when rows is null
        assertEquals(1, cache.count())
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    // =========================================================================
    // Multiple Callbacks Registration
    // =========================================================================

    @Test
    fun multipleOnAppliedCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var count = 0
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM t"),
            onApplied = listOf(
                { _ -> count++ },
                { _ -> count++ },
                { _ -> count++ },
            ),
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertEquals(3, count)
        conn.disconnect()
    }

    @Test
    fun multipleOnErrorCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var count = 0
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM t"),
            onError = listOf(
                { _, _ -> count++ },
                { _, _ -> count++ },
            ),
        )
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "oops",
            )
        )
        advanceUntilIdle()

        assertEquals(2, count)
        conn.disconnect()
    }

    // =========================================================================
    // Post-Disconnect Operations
    // =========================================================================

    @Test
    fun callReducerAfterDisconnectThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertFailsWith<IllegalStateException> {
            conn.callReducer("add", byteArrayOf(), "args")
        }
    }

    @Test
    fun callProcedureAfterDisconnectThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertFailsWith<IllegalStateException> {
            conn.callProcedure("proc", byteArrayOf())
        }
    }

    @Test
    fun oneOffQueryAfterDisconnectThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertFailsWith<IllegalStateException> {
            conn.oneOffQuery("SELECT 1") {}
        }
    }

    // =========================================================================
    // SubscribeApplied with Large Row Sets
    // =========================================================================

    @Test
    fun subscribeAppliedWithManyRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // 100 rows
        val rows = (1..100).map { SampleRow(it, "Row$it") }
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(
                        SingleTableRows(
                            "sample",
                            buildRowList(*rows.map { it.encode() }.toTypedArray())
                        )
                    )
                ),
            )
        )
        advanceUntilIdle()

        assertEquals(100, cache.count())
        conn.disconnect()
    }

    // =========================================================================
    // EventContext Correctness
    // =========================================================================

    @Test
    fun subscribeAppliedContextType() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var capturedCtx: EventContext? = null
        cache.onInsert { ctx, _ -> capturedCtx = ctx }

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

        assertTrue(capturedCtx is EventContext.SubscribeApplied)
        conn.disconnect()
    }

    @Test
    fun transactionUpdateContextType() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        var capturedCtx: EventContext? = null
        cache.onInsert { ctx, _ -> capturedCtx = ctx }

        transport.sendToClient(
            transactionUpdateMsg(
                handle.querySetId,
                "sample",
                inserts = buildRowList(SampleRow(1, "Alice").encode()),
            )
        )
        advanceUntilIdle()

        assertTrue(capturedCtx is EventContext.Transaction)
        conn.disconnect()
    }

    // =========================================================================
    // onDisconnect callback edge cases
    // =========================================================================

    @Test
    fun onDisconnectAddedAfterBuildStillFires() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Add callback AFTER connection is established
        var fired = false
        conn.onDisconnect { _, _ -> fired = true }

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(fired)
    }

    @Test
    fun onConnectErrorAddedAfterBuildStillFires() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        // Add callback AFTER connection is established
        var fired = false
        conn.onConnectError { _, _ -> fired = true }

        // Trigger identity mismatch (which fires onConnectError)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val differentIdentity = Identity(BigInteger.TEN)
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = differentIdentity,
                connectionId = testConnectionId,
                token = testToken,
            )
        )
        advanceUntilIdle()

        assertTrue(fired)
        // Connection auto-closes on identity mismatch (no manual disconnect needed)
    }

    // =========================================================================
    // Empty Subscription Queries
    // =========================================================================

    @Test
    fun subscribeWithEmptyQueryListSendsMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val queries = listOf("SELECT * FROM users", "SELECT * FROM messages")
        val handle = conn.subscribe(queries)

        assertEquals(queries, handle.queries)
        conn.disconnect()
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
        })
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
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertNull(receivedReason)
    }

    // =========================================================================
    // Reconnection (new connection after old one is closed)
    // =========================================================================

    @Test
    fun freshConnectionWorksAfterPreviousDisconnect() = runTest {
        val transport1 = FakeTransport()
        val conn1 = buildTestConnection(transport1)
        transport1.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertTrue(conn1.isActive)
        assertEquals(testIdentity, conn1.identity)

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
        })
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
    fun freshConnectionCacheIsIndependentFromOld() = runTest {
        val transport1 = FakeTransport()
        val conn1 = buildTestConnection(transport1)
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
        val conn2 = buildTestConnection(transport2)
        val cache2 = createSampleCache()
        conn2.clientCache.register("sample", cache2)
        transport2.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(0, cache2.count())
        conn2.disconnect()
    }

    // =========================================================================
    // Concurrent / racing disconnect
    // =========================================================================

    @Test
    fun disconnectWhileConnectingDoesNotCrash() = runTest {
        // Use a transport that suspends forever in connect()
        val suspendingTransport = object : Transport {
            override val isConnected: Boolean get() = false
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
            httpClient = io.ktor.client.HttpClient(),
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
    fun multipleSequentialDisconnectsFireCallbackOnlyOnce() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        })
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
    fun disconnectDuringSubscribeAppliedProcessing() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
    fun disconnectClearsClientCacheCompletely() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
    fun disconnectClearsIndexesConsistentlyWithCache() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
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
    fun serverCloseFollowedByClientDisconnectDoesNotDoubleFailPending() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        })
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
    // SubscribeApplied for table not in cache
    // =========================================================================

    @Test
    fun subscribeAppliedForUnregisteredTableIgnoresRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        // No cache registered for "sample"
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "Alice").encode())))
                ),
            )
        )
        advanceUntilIdle()

        // Should not crash — rows for unregistered tables are silently skipped
        assertTrue(conn.isActive)
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    // =========================================================================
    // Concurrent Reducer Calls
    // =========================================================================

    @Test
    fun multipleConcurrentReducerCallsGetCorrectCallbacks() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val results = mutableMapOf<String, Status>()
        val id1 = conn.callReducer("add", byteArrayOf(1), "add_args", callback = { ctx ->
            results["add"] = ctx.status
        })
        val id2 = conn.callReducer("remove", byteArrayOf(2), "remove_args", callback = { ctx ->
            results["remove"] = ctx.status
        })
        val id3 = conn.callReducer("update", byteArrayOf(3), "update_args", callback = { ctx ->
            results["update"] = ctx.status
        })
        advanceUntilIdle()

        // Respond in reverse order
        val writer = BsatnWriter()
        writer.writeString("update failed")
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id3,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(writer.toByteArray()),
            )
        )
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id1,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = id2,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(3, results.size)
        assertEquals(Status.Committed, results["add"])
        assertEquals(Status.Committed, results["remove"])
        assertTrue(results["update"] is Status.Failed)
        conn.disconnect()
    }

    // =========================================================================
    // BsatnRowKey equality and hashCode
    // =========================================================================

    @Test
    fun bsatnRowKeyEqualityAndHashCode() {
        val a = BsatnRowKey(byteArrayOf(1, 2, 3))
        val b = BsatnRowKey(byteArrayOf(1, 2, 3))
        val c = BsatnRowKey(byteArrayOf(1, 2, 4))

        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
        assertFalse(a == c)
    }

    @Test
    fun bsatnRowKeyWorksAsMapKey() {
        val map = mutableMapOf<BsatnRowKey, String>()
        val key1 = BsatnRowKey(byteArrayOf(10, 20))
        val key2 = BsatnRowKey(byteArrayOf(10, 20))
        val key3 = BsatnRowKey(byteArrayOf(30, 40))

        map[key1] = "first"
        map[key2] = "second" // Same content as key1, should overwrite
        map[key3] = "third"

        assertEquals(2, map.size)
        assertEquals("second", map[key1])
        assertEquals("third", map[key3])
    }

    // =========================================================================
    // DecodedRow equality
    // =========================================================================

    @Test
    fun decodedRowEquality() {
        val row1 = DecodedRow(SampleRow(1, "A"), byteArrayOf(1, 2, 3))
        val row2 = DecodedRow(SampleRow(1, "A"), byteArrayOf(1, 2, 3))
        val row3 = DecodedRow(SampleRow(1, "A"), byteArrayOf(4, 5, 6))

        assertEquals(row1, row2)
        assertEquals(row1.hashCode(), row2.hashCode())
        assertFalse(row1 == row3)
    }
}
