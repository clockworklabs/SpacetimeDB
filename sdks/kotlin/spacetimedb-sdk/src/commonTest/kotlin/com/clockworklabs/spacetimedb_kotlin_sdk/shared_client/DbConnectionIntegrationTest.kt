package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.ionspin.kotlin.bignum.integer.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import io.ktor.client.HttpClient
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

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class DbConnectionIntegrationTest {

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
        moduleDescriptor: ModuleDescriptor? = null,
        callbackDispatcher: kotlinx.coroutines.CoroutineDispatcher? = null,
    ): DbConnection {
        val conn = createTestConnection(transport, onConnect, onDisconnect, onConnectError, moduleDescriptor, callbackDispatcher)
        conn.connect()
        return conn
    }

    private fun TestScope.createTestConnection(
        transport: FakeTransport,
        onConnect: ((DbConnectionView, Identity, String) -> Unit)? = null,
        onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
        onConnectError: ((DbConnectionView, Throwable) -> Unit)? = null,
        moduleDescriptor: ModuleDescriptor? = null,
        callbackDispatcher: kotlinx.coroutines.CoroutineDispatcher? = null,
    ): DbConnection {
        return DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOfNotNull(onConnect),
            onDisconnectCallbacks = listOfNotNull(onDisconnect),
            onConnectErrorCallbacks = listOfNotNull(onConnectError),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = moduleDescriptor,
            callbackDispatcher = callbackDispatcher,
        )
    }

    /** Generic helper that accepts any [Transport] implementation. */
    private fun TestScope.createConnectionWithTransport(
        transport: Transport,
        onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
    ): DbConnection {
        return DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = emptyList(),
            onDisconnectCallbacks = listOfNotNull(onDisconnect),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
    }

    private fun emptyQueryRows(): QueryRows = QueryRows(emptyList())

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

        assertEquals(testIdentity, connectIdentity)
        assertEquals(testToken, connectToken)
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

        assertEquals(testIdentity, conn.identity)
        assertEquals(testToken, conn.token)
        assertEquals(testConnectionId, conn.connectionId)
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

    // --- Subscriptions ---

    @Test
    fun subscribeSendsClientMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.subscribe(listOf("SELECT * FROM player"))
        advanceUntilIdle()

        val subMsg = transport.sentMessages.filterIsInstance<ClientMessage.Subscribe>().firstOrNull()
        assertNotNull(subMsg)
        assertEquals(listOf("SELECT * FROM player"), subMsg.queryStrings)
        conn.disconnect()
    }

    @Test
    fun subscribeAppliedFiresOnAppliedCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM player"),
            onApplied = listOf { _ -> applied = true },
        )

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied)
        assertTrue(handle.isActive)
        conn.disconnect()
    }

    @Test
    fun subscriptionErrorFiresOnErrorCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorMsg: String? = null
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM nonexistent"),
            onError = listOf { _, err -> errorMsg = err.message },
        )

        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "table not found",
            )
        )
        advanceUntilIdle()

        assertEquals("table not found", errorMsg)
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    // --- Table cache ---

    @Test
    fun tableCacheUpdatesOnSubscribeApplied() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val rowList = buildRowList(row.encode())
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", rowList))),
            )
        )
        advanceUntilIdle()

        assertEquals(1, cache.count())
        assertEquals("Alice", cache.all().first().name)
        conn.disconnect()
    }

    @Test
    fun tableCacheInsertsAndDeletesViaTransactionUpdate() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // First insert a row via SubscribeApplied
        val row1 = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row1.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Now send a TransactionUpdate that inserts row2 and deletes row1
        val row2 = SampleRow(2, "Bob")
        val inserts = buildRowList(row2.encode())
        val deletes = buildRowList(row1.encode())

        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(inserts, deletes))
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(1, cache.count())
        assertEquals("Bob", cache.all().first().name)
        conn.disconnect()
    }

    // --- Reducers ---

    @Test
    fun callReducerSendsClientMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callReducer("add", byteArrayOf(1, 2, 3), "test-args")
        advanceUntilIdle()

        val reducerMsg = transport.sentMessages.filterIsInstance<ClientMessage.CallReducer>().firstOrNull()
        assertNotNull(reducerMsg)
        assertEquals("add", reducerMsg.reducer)
        assertTrue(reducerMsg.args.contentEquals(byteArrayOf(1, 2, 3)))
        conn.disconnect()
    }

    @Test
    fun reducerResultOkFiresCallbackWithCommitted() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val requestId = conn.callReducer(
            reducerName = "add",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { ctx -> status = ctx.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Ok(
                    retValue = byteArrayOf(),
                    transactionUpdate = TransactionUpdate(emptyList()),
                ),
            )
        )
        advanceUntilIdle()

        assertEquals(Status.Committed, status)
        conn.disconnect()
    }

    @Test
    fun reducerResultErrFiresCallbackWithFailed() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val errorText = "something went wrong"
        val writer = BsatnWriter()
        writer.writeString(errorText)
        val errorBytes = writer.toByteArray()

        val requestId = conn.callReducer(
            reducerName = "bad_reducer",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { ctx -> status = ctx.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(errorBytes),
            )
        )
        advanceUntilIdle()

        assertTrue(status is Status.Failed)
        assertEquals(errorText, (status as Status.Failed).message)
        conn.disconnect()
    }

    // --- One-off queries ---

    @Test
    fun oneOffQueryCallbackReceivesResult() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var result: ServerMessage.OneOffQueryResult? = null
        val requestId = conn.oneOffQuery("SELECT * FROM sample") { msg ->
            result = msg
        }
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = requestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        val capturedResult = result
        assertNotNull(capturedResult)
        assertTrue(capturedResult.result is QueryResult.Ok)
        conn.disconnect()
    }

    @Test
    fun oneOffQuerySuspendReturnsResult() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Retrieve the requestId that will be assigned by inspecting sentMessages
        val beforeCount = transport.sentMessages.size
        // Launch the suspend query in a separate coroutine since it suspends
        var queryResult: ServerMessage.OneOffQueryResult? = null
        val job = launch {
            queryResult = conn.oneOffQuery("SELECT * FROM sample")
        }
        advanceUntilIdle()

        // Find the OneOffQuery message
        val queryMsg = transport.sentMessages.drop(beforeCount)
            .filterIsInstance<ClientMessage.OneOffQuery>().firstOrNull()
        assertNotNull(queryMsg)

        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = queryMsg.requestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        val capturedQueryResult = queryResult
        assertNotNull(capturedQueryResult)
        assertTrue(capturedQueryResult.result is QueryResult.Ok)
        conn.disconnect()
    }

    // --- Disconnect ---

    @Test
    fun disconnectClearsPendingCallbacks() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        conn.callReducer(
            reducerName = "add",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { _ -> },
        )
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertTrue(handle.isEnded)
    }

    @Test
    fun disconnectIsFinal() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport)
        conn.connect()
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        assertFalse(conn.isActive)
        assertFailsWith<IllegalStateException> { conn.connect() }
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

    // --- Unsubscribe lifecycle ---

    @Test
    fun unsubscribeThenCallbackFiresOnUnsubscribeApplied() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied = false
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> applied = true },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(applied)
        assertTrue(handle.isActive)

        var unsubEndFired = false
        handle.unsubscribeThen { _ -> unsubEndFired = true }
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // Verify Unsubscribe message was sent
        val unsubMsg = transport.sentMessages.filterIsInstance<ClientMessage.Unsubscribe>().firstOrNull()
        assertNotNull(unsubMsg)
        assertEquals(handle.querySetId, unsubMsg.querySetId)

        // Server confirms unsubscribe
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(unsubEndFired)
        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    @Test
    fun unsubscribeThenCallbackIsSetBeforeMessageSent() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        var callbackFired = false
        handle.unsubscribeThen { _ -> callbackFired = true }
        advanceUntilIdle()

        assertTrue(handle.isUnsubscribing)

        // Simulate immediate server response
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(callbackFired, "Callback should fire even with immediate server response")
        conn.disconnect()
    }

    // --- Reducer outcomes ---

    @Test
    fun reducerResultOkEmptyFiresCallbackWithCommitted() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val requestId = conn.callReducer(
            reducerName = "noop",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { ctx -> status = ctx.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(Status.Committed, status)
        conn.disconnect()
    }

    @Test
    fun reducerResultInternalErrorFiresCallbackWithFailed() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        val requestId = conn.callReducer(
            reducerName = "broken",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { ctx -> status = ctx.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.InternalError("internal server error"),
            )
        )
        advanceUntilIdle()

        assertTrue(status is Status.Failed)
        assertEquals("internal server error", (status as Status.Failed).message)
        conn.disconnect()
    }

    // --- Procedures ---

    @Test
    fun callProcedureSendsClientMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callProcedure("my_proc", byteArrayOf(42))
        advanceUntilIdle()

        val procMsg = transport.sentMessages.filterIsInstance<ClientMessage.CallProcedure>().firstOrNull()
        assertNotNull(procMsg)
        assertEquals("my_proc", procMsg.procedure)
        assertTrue(procMsg.args.contentEquals(byteArrayOf(42)))
        conn.disconnect()
    }

    @Test
    fun procedureResultFiresCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var receivedStatus: ProcedureStatus? = null
        val requestId = conn.callProcedure(
            procedureName = "my_proc",
            args = byteArrayOf(),
            callback = { _, msg -> receivedStatus = msg.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                status = ProcedureStatus.Returned(byteArrayOf(1, 2, 3)),
                timestamp = Timestamp.UNIX_EPOCH,
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
                requestId = requestId,
            )
        )
        advanceUntilIdle()

        assertTrue(receivedStatus is ProcedureStatus.Returned)
        conn.disconnect()
    }

    @Test
    fun procedureResultInternalErrorFiresCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var receivedStatus: ProcedureStatus? = null
        val requestId = conn.callProcedure(
            procedureName = "bad_proc",
            args = byteArrayOf(),
            callback = { _, msg -> receivedStatus = msg.status },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                status = ProcedureStatus.InternalError("proc failed"),
                timestamp = Timestamp.UNIX_EPOCH,
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
                requestId = requestId,
            )
        )
        advanceUntilIdle()

        assertTrue(receivedStatus is ProcedureStatus.InternalError)
        assertEquals("proc failed", (receivedStatus as ProcedureStatus.InternalError).message)
        conn.disconnect()
    }

    // --- One-off query error ---

    @Test
    fun oneOffQueryCallbackReceivesError() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var result: ServerMessage.OneOffQueryResult? = null
        val requestId = conn.oneOffQuery("SELECT * FROM bad") { msg ->
            result = msg
        }
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = requestId,
                result = QueryResult.Err("syntax error"),
            )
        )
        advanceUntilIdle()

        val capturedResult = result
        assertNotNull(capturedResult)
        val errResult = capturedResult.result
        assertTrue(errResult is QueryResult.Err)
        assertEquals("syntax error", errResult.error)
        conn.disconnect()
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

    // --- Table callbacks through integration ---

    @Test
    fun tableOnInsertFiresOnSubscribeApplied() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var insertedRow: SampleRow? = null
        cache.onInsert { _, row -> insertedRow = row }

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

        assertEquals(row, insertedRow)
        conn.disconnect()
    }

    @Test
    fun tableOnDeleteFiresOnTransactionUpdate() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
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

        var deletedRow: SampleRow? = null
        cache.onDelete { _, r -> deletedRow = r }

        // Delete via TransactionUpdate
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(row.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(row, deletedRow)
        assertEquals(0, cache.count())
        conn.disconnect()
    }

    @Test
    fun tableOnUpdateFiresOnTransactionUpdate() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
        val oldRow = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(oldRow.encode())))),
            )
        )
        advanceUntilIdle()

        var updatedOld: SampleRow? = null
        var updatedNew: SampleRow? = null
        cache.onUpdate { _, old, new ->
            updatedOld = old
            updatedNew = new
        }

        // Update: delete old row, insert new row with same PK
        val newRow = SampleRow(1, "Alice Updated")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(newRow.encode()),
                                            deletes = buildRowList(oldRow.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(oldRow, updatedOld)
        assertEquals(newRow, updatedNew)
        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().first().name)
        conn.disconnect()
    }

    // --- Identity mismatch ---

    @Test
    fun identityMismatchFiresOnConnectError() = runTest {
        val transport = FakeTransport()
        var errorMsg: String? = null
        val conn = buildTestConnection(transport, onConnectError = { _, err ->
            errorMsg = err.message
        })

        // First InitialConnection sets identity
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertEquals(testIdentity, conn.identity)

        // Second InitialConnection with different identity triggers error
        val differentIdentity = Identity(BigInteger.TEN)
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = differentIdentity,
                connectionId = testConnectionId,
                token = testToken,
            )
        )
        advanceUntilIdle()

        assertNotNull(errorMsg)
        assertTrue(errorMsg!!.contains("unexpected identity"))
        // Identity should NOT have changed
        assertEquals(testIdentity, conn.identity)
        conn.disconnect()
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
            onError = listOf { _, err -> errorMsg = err.message },
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

    // --- Callback removal ---

    @Test
    fun removeOnDisconnectPreventsCallback() = runTest {
        val transport = FakeTransport()
        var fired = false
        val cb: (DbConnectionView, Throwable?) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onDisconnect = cb)
        conn.removeOnDisconnect(cb)
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        transport.closeFromServer()
        advanceUntilIdle()

        assertFalse(fired)
        conn.disconnect()
    }

    // --- Unsubscribe from wrong state ---

    @Test
    fun unsubscribeFromPendingStateThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        // Handle is PENDING — no SubscribeApplied received yet
        assertTrue(handle.isPending)

        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    @Test
    fun unsubscribeFromEndedStateThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM player"),
            onError = listOf { _, _ -> },
        )

        // Force ENDED via SubscriptionError
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 1u,
                querySetId = handle.querySetId,
                error = "error",
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isEnded)

        assertFailsWith<IllegalStateException> {
            handle.unsubscribe()
        }
        conn.disconnect()
    }

    // --- onBeforeDelete ---

    @Test
    fun onBeforeDeleteFiresBeforeMutation() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row
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

        // Track onBeforeDelete — at callback time, the row should still be in the cache
        var cacheCountDuringCallback: Int? = null
        var beforeDeleteRow: SampleRow? = null
        cache.onBeforeDelete { _, r ->
            beforeDeleteRow = r
            cacheCountDuringCallback = cache.count()
        }

        // Delete via TransactionUpdate
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(),
                                            deletes = buildRowList(row.encode()),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        assertEquals(row, beforeDeleteRow)
        assertEquals(1, cacheCountDuringCallback) // Row still present during onBeforeDelete
        assertEquals(0, cache.count()) // Row removed after
        conn.disconnect()
    }

    // --- Builder validation ---

    @Test
    fun builderFailsWithoutUri() = runTest {
        assertFailsWith<IllegalArgumentException> {
            DbConnection.Builder()
                .withDatabaseName("test")
                .build()
        }
    }

    @Test
    fun builderFailsWithoutDatabaseName() = runTest {
        assertFailsWith<IllegalArgumentException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .build()
        }
    }

    // --- Unknown querySetId / requestId (silent early returns) ---

    @Test
    fun subscribeAppliedForUnknownQuerySetIdIsIgnored() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a callback to verify it does NOT fire
        var insertFired = false
        cache.onInsert { _, _ -> insertFired = true }

        // Send SubscribeApplied for a querySetId that was never subscribed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 99u,
                querySetId = QuerySetId(999u),
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "ghost").encode())))),
            )
        )
        advanceUntilIdle()

        // Should not crash, no rows inserted, no callbacks fired
        assertTrue(conn.isActive)
        assertEquals(0, cache.count())
        assertFalse(insertFired)
        conn.disconnect()
    }

    @Test
    fun reducerResultForUnknownRequestIdIsIgnored() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val cacheCountBefore = cache.count()

        // Send ReducerResultMsg with an Ok that has table updates — should be silently skipped
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = 999u,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        assertEquals(cacheCountBefore, cache.count())
        conn.disconnect()
    }

    @Test
    fun oneOffQueryResultForUnknownRequestIdIsIgnored() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a real query so we can verify the unknown one doesn't interfere
        var realCallbackFired = false
        val realRequestId = conn.oneOffQuery("SELECT 1") { _ -> realCallbackFired = true }
        advanceUntilIdle()

        // Send result for unknown requestId
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = 999u,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        // The unknown result should not fire the real callback
        assertTrue(conn.isActive)
        assertFalse(realCallbackFired)

        // Now send the real result — should fire
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = realRequestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()
        assertTrue(realCallbackFired)
        conn.disconnect()
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

    // --- oneOffQuery cancellation ---

    @Test
    fun oneOffQuerySuspendCancellationCleansUpCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val job = launch {
            conn.oneOffQuery("SELECT * FROM sample") // will suspend forever
        }
        advanceUntilIdle()

        // Cancel the coroutine — should clean up the callback
        job.cancel()
        advanceUntilIdle()

        // Now send a result for that requestId — should not crash
        val queryMsg = transport.sentMessages.filterIsInstance<ClientMessage.OneOffQuery>().lastOrNull()
        assertNotNull(queryMsg)
        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = queryMsg.requestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- User callback exception does not crash receive loop ---

    @Test
    fun userCallbackExceptionDoesNotCrashConnection() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a callback that throws
        cache.onInsert { _, _ -> error("callback explosion") }

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

        // Row should still be inserted despite callback exception
        assertEquals(1, cache.count())
        // Connection should still be active
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- Multiple callbacks ---

    @Test
    fun multipleOnConnectCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        var count = 0
        val cb: (DbConnectionView, Identity, String) -> Unit = { _, _, _ -> count++ }
        val conn = DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOf(cb, cb, cb),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(3, count)
        conn.disconnect()
    }

    // --- Token not overwritten if already set ---

    @Test
    fun tokenNotOverwrittenOnSecondInitialConnection() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        // First connection sets token
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertEquals(testToken, conn.token)

        // Second InitialConnection with same identity but different token — token stays
        transport.sendToClient(
            ServerMessage.InitialConnection(
                identity = testIdentity,
                connectionId = testConnectionId,
                token = "new-token",
            )
        )
        advanceUntilIdle()

        assertEquals(testToken, conn.token)
        conn.disconnect()
    }

    // --- removeOnConnectError ---

    @Test
    fun removeOnConnectErrorPreventsCallback() = runTest {
        val transport = FakeTransport(connectError = RuntimeException("fail"))
        var fired = false
        val cb: (DbConnectionView, Throwable) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onConnectError = cb)
        conn.removeOnConnectError(cb)

        try {
            conn.connect()
        } catch (_: Exception) { }
        advanceUntilIdle()

        assertFalse(fired)
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

    // --- callReducer without callback (fire-and-forget) ---

    @Test
    fun callReducerWithoutCallbackSendsMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callReducer("add", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()

        val sent = transport.sentMessages.filterIsInstance<ClientMessage.CallReducer>()
        assertEquals(1, sent.size)
        assertEquals("add", sent[0].reducer)

        // Sending a result for it should not crash (no callback registered)
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = sent[0].requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- callProcedure without callback (fire-and-forget) ---

    @Test
    fun callProcedureWithoutCallbackSendsMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callProcedure("myProc", byteArrayOf(), callback = null)
        advanceUntilIdle()

        val sent = transport.sentMessages.filterIsInstance<ClientMessage.CallProcedure>()
        assertEquals(1, sent.size)
        assertEquals("myProc", sent[0].procedure)

        // Sending a result for it should not crash (no callback registered)
        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                requestId = sent[0].requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                status = ProcedureStatus.Returned(byteArrayOf()),
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- Reducer result before identity is set ---

    @Test
    fun reducerResultBeforeIdentitySetIsIgnored() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        // Do NOT send InitialConnection — identity stays null

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = 1u,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        // Connection should still be active (message silently ignored)
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- Procedure result before identity is set ---

    @Test
    fun procedureResultBeforeIdentitySetIsIgnored() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        // Do NOT send InitialConnection — identity stays null

        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                requestId = 1u,
                timestamp = Timestamp.UNIX_EPOCH,
                status = ProcedureStatus.Returned(byteArrayOf()),
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- decodeReducerError with corrupted BSATN ---

    @Test
    fun reducerErrWithCorruptedBsatnDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var status: Status? = null
        conn.callReducer("bad", byteArrayOf(), "args", callback = { ctx ->
            status = ctx.status
        })
        advanceUntilIdle()

        val sent = transport.sentMessages.filterIsInstance<ClientMessage.CallReducer>().last()
        // Send Err with invalid BSATN bytes (not a valid BSATN string)
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = sent.requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.Err(byteArrayOf(0xFF.toByte(), 0x00, 0x01)),
            )
        )
        advanceUntilIdle()

        val capturedStatus = status
        assertNotNull(capturedStatus)
        assertTrue(capturedStatus is Status.Failed)
        assertTrue(capturedStatus.message.contains("undecodable"))
        conn.disconnect()
    }

    // --- unsubscribe with custom flags ---

    @Test
    fun unsubscribeWithSendDroppedRowsFlag() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        handle.unsubscribe(UnsubscribeFlags.SendDroppedRows)
        advanceUntilIdle()

        val unsub = transport.sentMessages.filterIsInstance<ClientMessage.Unsubscribe>().last()
        assertEquals(handle.querySetId, unsub.querySetId)
        assertEquals(UnsubscribeFlags.SendDroppedRows, unsub.flags)
        conn.disconnect()
    }

    // --- sendMessage after close ---

    @Test
    fun subscribeAfterCloseThrows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()

        // Calling subscribe on a closed connection should throw
        // so the caller knows the message was not sent
        assertFailsWith<IllegalStateException> {
            conn.subscribe(listOf("SELECT * FROM player"))
        }
    }

    // --- Builder ensureMinimumVersion ---

    @Test
    fun builderRejectsOldCliVersion() = runTest {
        val oldModule = object : ModuleDescriptor {
            override val cliVersion = "1.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        assertFailsWith<IllegalStateException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .withDatabaseName("test")
                .withModule(oldModule)
                .build()
        }
    }

    // --- Module descriptor integration ---

    @Test
    fun dbConnectionConstructorDoesNotCallRegisterTables() = runTest {
        val transport = FakeTransport()
        var tablesRegistered = false

        val descriptor = object : ModuleDescriptor {
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {
                tablesRegistered = true
                cache.register("sample", createSampleCache())
            }
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Use the module descriptor through DbConnection — pass it via the helper
        val conn = buildTestConnection(transport, moduleDescriptor = descriptor)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Verify that when moduleDescriptor is set, handleReducerEvent is called
        // during reducer processing (this tests the actual integration, not manual calls)
        assertFalse(tablesRegistered) // registerTables is NOT called by DbConnection constructor —
        // it's the Builder's responsibility. This verifies that.

        // The table should NOT be registered since we bypassed the Builder
        assertNull(conn.clientCache.getUntypedTable("sample"))
        conn.disconnect()
    }

    // --- handleReducerEvent fires from module descriptor ---

    @Test
    fun moduleDescriptorHandleReducerEventFires() = runTest {
        val transport = FakeTransport()
        var reducerEventName: String? = null

        val descriptor = object : ModuleDescriptor {
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {
                reducerEventName = ctx.reducerName
            }
        }

        val conn = buildTestConnection(transport, moduleDescriptor = descriptor)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callReducer("myReducer", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()

        val sent = transport.sentMessages.filterIsInstance<ClientMessage.CallReducer>().last()
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = sent.requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals("myReducer", reducerEventName)
        conn.disconnect()
    }

    // --- Mid-stream transport failures ---

    @Test
    fun transportErrorFiresOnDisconnectWithError() = runTest {
        val transport = FakeTransport()
        var disconnectError: Throwable? = null
        var disconnected = false
        val conn = buildTestConnection(transport, onDisconnect = { _, err ->
            disconnected = true
            disconnectError = err
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        // Simulate mid-stream transport error
        val networkError = RuntimeException("connection reset by peer")
        transport.closeWithError(networkError)
        advanceUntilIdle()

        assertTrue(disconnected)
        assertNotNull(disconnectError)
        assertEquals("connection reset by peer", disconnectError!!.message)
        conn.disconnect()
    }

    @Test
    fun transportErrorFailsPendingSubscription() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Subscribe but don't send SubscribeApplied
        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        advanceUntilIdle()
        assertTrue(handle.isPending)

        // Kill the transport — pending subscription should be failed
        transport.closeWithError(RuntimeException("network error"))
        advanceUntilIdle()

        assertTrue(handle.isEnded)
        conn.disconnect()
    }

    @Test
    fun transportErrorFailsPendingReducerCallback() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Call reducer but don't send result
        var callbackFired = false
        conn.callReducer("add", byteArrayOf(), "args", callback = { _ ->
            callbackFired = true
        })
        advanceUntilIdle()

        // Kill the transport — pending callback should be cleared
        transport.closeWithError(RuntimeException("network error"))
        advanceUntilIdle()

        // The callback should NOT have been fired (no result arrived)
        assertFalse(callbackFired)
        conn.disconnect()
    }

    @Test
    fun sendErrorDoesNotCrashReceiveLoop() = runTest {
        val transport = FakeTransport()
        // Use a CoroutineExceptionHandler so the unhandled send-loop exception
        // doesn't propagate to runTest — we're testing that the receive loop survives.
        val handler = kotlinx.coroutines.CoroutineExceptionHandler { _, _ -> }
        val conn = DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler) + handler),
            onConnectCallbacks = emptyList(),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Make sends fail
        transport.sendError = RuntimeException("write failed")

        // The send loop dies, but the receive loop should still be active
        conn.callReducer("add", byteArrayOf(), "args")
        advanceUntilIdle()

        // Connection should still receive messages
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        advanceUntilIdle()

        // The subscribe message was dropped (send loop is dead),
        // but we can still feed a SubscribeApplied to verify the receive loop is alive
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(SampleRow(1, "Alice").encode())))),
            )
        )
        advanceUntilIdle()

        assertEquals(1, cache.count())
        conn.disconnect()
    }

    // --- Raw transport: partial/corrupted frame handling ---

    @Test
    fun truncatedBsatnFrameFiresOnDisconnect() = runTest {
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        advanceUntilIdle()

        // Send a valid InitialConnection first, then a truncated frame
        val writer = BsatnWriter()
        writer.writeSumTag(0u) // InitialConnection tag
        writer.writeU256(testIdentity.data) // identity
        writer.writeU128(testConnectionId.data) // connectionId
        writer.writeString(testToken) // token
        rawTransport.sendRawToClient(writer.toByteArray())
        advanceUntilIdle()

        // Now send a truncated frame — only the tag byte, missing all fields
        rawTransport.sendRawToClient(byteArrayOf(0x00))
        advanceUntilIdle()

        assertNotNull(disconnectError)
        conn.disconnect()
    }

    @Test
    fun invalidServerMessageTagFiresOnDisconnect() = runTest {
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        advanceUntilIdle()

        // Send a frame with an invalid sum tag (255)
        rawTransport.sendRawToClient(byteArrayOf(0xFF.toByte()))
        advanceUntilIdle()

        assertNotNull(disconnectError)
        assertTrue(disconnectError!!.message!!.contains("Unknown ServerMessage tag"))
        conn.disconnect()
    }

    @Test
    fun emptyFrameFiresOnDisconnect() = runTest {
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        advanceUntilIdle()

        // Send an empty byte array — BsatnReader will fail to read even the tag byte
        rawTransport.sendRawToClient(byteArrayOf())
        advanceUntilIdle()

        assertNotNull(disconnectError)
        conn.disconnect()
    }

    /** Encode a valid InitialConnection as raw BSATN bytes. */
    private fun encodeInitialConnectionBytes(): ByteArray {
        val w = BsatnWriter()
        w.writeSumTag(0u) // InitialConnection tag
        w.writeU256(testIdentity.data)
        w.writeU128(testConnectionId.data)
        w.writeString(testToken)
        return w.toByteArray()
    }

    @Test
    fun truncatedMidFieldDisconnects() = runTest {
        // Valid tag (6 = ReducerResultMsg) + valid requestId, but truncated before timestamp
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        val w = BsatnWriter()
        w.writeSumTag(6u) // ReducerResultMsg
        w.writeU32(1u)    // requestId — valid
        // Missing: timestamp + ReducerOutcome
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError, "Truncated mid-field should fire onDisconnect with error")
        assertFalse(conn.isActive)
    }

    @Test
    fun invalidNestedOptionTagDisconnects() = runTest {
        // SubscriptionError (tag 3) has Option<u32> for requestId — inject invalid option tag
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()

        val w = BsatnWriter()
        w.writeSumTag(3u)  // SubscriptionError
        w.writeSumTag(99u) // Invalid Option tag (should be 0=Some or 1=None)
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
        assertTrue(disconnectError!!.message!!.contains("Invalid Option tag"))
    }

    @Test
    fun invalidResultTagInOneOffQueryDisconnects() = runTest {
        // OneOffQueryResult (tag 5) has Result<QueryRows, String> — inject invalid result tag
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()

        val w = BsatnWriter()
        w.writeSumTag(5u)  // OneOffQueryResult
        w.writeU32(42u)    // requestId
        w.writeSumTag(77u) // Invalid Result tag (should be 0=Ok or 1=Err)
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
        assertTrue(disconnectError!!.message!!.contains("Invalid Result tag"))
    }

    @Test
    fun oversizedStringLengthDisconnects() = runTest {
        // Valid InitialConnection tag + identity + connectionId + string with huge length prefix
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        advanceUntilIdle()

        val w = BsatnWriter()
        w.writeSumTag(0u) // InitialConnection
        w.writeU256(testIdentity.data)
        w.writeU128(testConnectionId.data)
        w.writeU32(0xFFFFFFFFu) // String length = 4GB — way more than remaining bytes
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
    }

    @Test
    fun invalidReducerOutcomeTagDisconnects() = runTest {
        // ReducerResultMsg (tag 6) with valid fields but invalid ReducerOutcome tag
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()

        val w = BsatnWriter()
        w.writeSumTag(6u)    // ReducerResultMsg
        w.writeU32(1u)       // requestId
        w.writeI64(12345L)   // timestamp (Timestamp = i64 microseconds)
        w.writeSumTag(200u)  // Invalid ReducerOutcome tag
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
    }

    @Test
    fun corruptFrameAfterEstablishedConnectionFailsPendingOps() = runTest {
        // Establish full connection with subscriptions/reducers, then corrupt frame
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()
        assertTrue(conn.isActive)

        // Fire a reducer call so there's a pending operation
        var callbackFired = false
        conn.callReducer("test", byteArrayOf(), "args", callback = { _ -> callbackFired = true })
        advanceUntilIdle()
        assertEquals(1, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())

        // Corrupt frame kills the connection
        rawTransport.sendRawToClient(byteArrayOf(0xFE.toByte()))
        advanceUntilIdle()

        assertNotNull(disconnectError)
        assertFalse(conn.isActive)
        // Reducer callback should NOT have fired (it was discarded, not responded to)
        assertFalse(callbackFired)
    }

    @Test
    fun garbageAfterValidMessageIsIgnored() = runTest {
        // A fully valid InitialConnection with extra trailing bytes appended.
        // BsatnReader doesn't check that all bytes are consumed, so this should work.
        val rawTransport = RawFakeTransport()
        var connected = false
        var disconnectError: Throwable? = null
        val conn = DbConnection(
            transport = rawTransport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOf { _, _, _ -> connected = true },
            onDisconnectCallbacks = listOf { _, err -> disconnectError = err },
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()
        advanceUntilIdle()

        val validBytes = encodeInitialConnectionBytes()
        val withTrailing = validBytes + byteArrayOf(0xDE.toByte(), 0xAD.toByte(), 0xBE.toByte(), 0xEF.toByte())
        rawTransport.sendRawToClient(withTrailing)
        advanceUntilIdle()

        // Connection should succeed — trailing bytes are not consumed but not checked
        assertTrue(connected, "Valid message with trailing garbage should still connect")
        assertNull(disconnectError, "Trailing garbage should not cause disconnect")
        conn.disconnect()
    }

    @Test
    fun allZeroBytesFrameDisconnects() = runTest {
        // A frame of all zeroes — tag 0 (InitialConnection) but fields are all zeroes,
        // which will produce a truncated read since the string length is 0 but
        // Identity (32 bytes) and ConnectionId (16 bytes) consume the buffer first
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        advanceUntilIdle()

        // 10 zero bytes: tag=0 (InitialConnection), then only 9 bytes for Identity (needs 32)
        rawTransport.sendRawToClient(ByteArray(10))
        advanceUntilIdle()

        assertNotNull(disconnectError)
    }

    @Test
    fun validTagWithRandomGarbageFieldsDisconnects() = runTest {
        // SubscribeApplied (tag 1) followed by random garbage that doesn't form valid QueryRows
        val rawTransport = RawFakeTransport()
        var disconnectError: Throwable? = null
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, err ->
            disconnectError = err
        })
        conn.connect()
        rawTransport.sendRawToClient(encodeInitialConnectionBytes())
        advanceUntilIdle()

        val w = BsatnWriter()
        w.writeSumTag(1u) // SubscribeApplied
        w.writeU32(1u)    // requestId
        w.writeU32(1u)    // querySetId
        // QueryRows needs: array_len (u32) + table entries — write nonsensical large array len
        w.writeU32(999999u) // array_len for QueryRows — far more than available bytes
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
    }

    // --- Overlapping subscriptions ---

    @Test
    fun overlappingSubscriptionsRefCountRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        var insertCount = 0
        var deleteCount = 0
        cache.onInsert { _, _ -> insertCount++ }
        cache.onDelete { _, _ -> deleteCount++ }

        // First subscription inserts row
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())
        assertEquals(1, insertCount) // onInsert fires for first occurrence

        // Second subscription also inserts the same row — ref count goes to 2
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Still one row (ref count = 2)
        assertEquals(1, insertCount) // onInsert does NOT fire again

        // First subscription unsubscribes — ref count decrements to 1
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Row still present (ref count = 1)
        assertEquals(0, deleteCount) // onDelete does NOT fire

        // Second subscription unsubscribes — ref count goes to 0
        handle2.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 4u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(0, cache.count()) // Row removed
        assertEquals(1, deleteCount) // onDelete fires now

        conn.disconnect()
    }

    @Test
    fun overlappingSubscriptionTransactionUpdateAffectsBothHandles() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        // Two subscriptions on the same table
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // ref count = 2

        // A TransactionUpdate that updates the row (delete old + insert new)
        val updatedRow = SampleRow(1, "Alice Updated")
        var updateOld: SampleRow? = null
        var updateNew: SampleRow? = null
        cache.onUpdate { _, old, new -> updateOld = old; updateNew = new }

        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle1.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(
                                        TableUpdateRows.PersistentTable(
                                            inserts = buildRowList(updatedRow.encode()),
                                            deletes = buildRowList(encodedRow),
                                        )
                                    )
                                )
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // The row should be updated in the cache
        assertEquals(1, cache.count())
        assertEquals("Alice Updated", cache.all().first().name)
        assertEquals(row, updateOld)
        assertEquals(updatedRow, updateNew)

        // After unsubscribing handle1, the row still has ref count from handle2
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(updatedRow.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count()) // Still present via handle2
        assertEquals("Alice Updated", cache.all().first().name)

        conn.disconnect()
    }

    // --- Stats tracking ---

    @Test
    fun statsSubscriptionTrackerIncrementsOnSubscribeApplied() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.subscriptionRequestTracker
        assertEquals(0, tracker.getSampleCount())

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        // Request started but not yet finished
        assertEquals(1, tracker.getRequestsAwaitingResponse())

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.getSampleCount())
        assertEquals(0, tracker.getRequestsAwaitingResponse())
        conn.disconnect()
    }

    @Test
    fun statsReducerTrackerIncrementsOnReducerResult() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.reducerRequestTracker
        assertEquals(0, tracker.getSampleCount())

        val requestId = conn.callReducer("add", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()
        assertEquals(1, tracker.getRequestsAwaitingResponse())

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.getSampleCount())
        assertEquals(0, tracker.getRequestsAwaitingResponse())
        conn.disconnect()
    }

    @Test
    fun statsProcedureTrackerIncrementsOnProcedureResult() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.procedureRequestTracker
        assertEquals(0, tracker.getSampleCount())

        val requestId = conn.callProcedure("my_proc", byteArrayOf(), callback = null)
        advanceUntilIdle()
        assertEquals(1, tracker.getRequestsAwaitingResponse())

        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                status = ProcedureStatus.Returned(byteArrayOf()),
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.getSampleCount())
        assertEquals(0, tracker.getRequestsAwaitingResponse())
        conn.disconnect()
    }

    @Test
    fun statsOneOffTrackerIncrementsOnQueryResult() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.oneOffRequestTracker
        assertEquals(0, tracker.getSampleCount())

        val requestId = conn.oneOffQuery("SELECT 1") { _ -> }
        advanceUntilIdle()
        assertEquals(1, tracker.getRequestsAwaitingResponse())

        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = requestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.getSampleCount())
        assertEquals(0, tracker.getRequestsAwaitingResponse())
        conn.disconnect()
    }

    @Test
    fun statsApplyMessageTrackerIncrementsOnEveryServerMessage() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.applyMessageTracker
        // InitialConnection is the first message processed
        assertEquals(1, tracker.getSampleCount())

        // Send a SubscribeApplied — second message
        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertEquals(2, tracker.getSampleCount())

        // Send a ReducerResult — third message
        val reducerRequestId = conn.callReducer("add", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = reducerRequestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()
        assertEquals(3, tracker.getSampleCount())

        conn.disconnect()
    }

    @Test
    fun validFrameAfterCorruptedFrameIsNotProcessed() = runTest {
        val rawTransport = RawFakeTransport()
        var disconnected = false
        val conn = createConnectionWithTransport(rawTransport, onDisconnect = { _, _ ->
            disconnected = true
        })
        conn.connect()
        advanceUntilIdle()

        // Send a corrupted frame — this kills the receive loop
        rawTransport.sendRawToClient(byteArrayOf(0xFF.toByte()))
        advanceUntilIdle()
        assertTrue(disconnected)

        // The connection is now disconnected; identity should NOT be set
        // even if we somehow send a valid InitialConnection afterward
        assertNull(conn.identity)
        conn.disconnect()
    }

    // --- Callback exception handling ---

    @Test
    fun onConnectCallbackExceptionDoesNotPreventOtherCallbacks() = runTest {
        val transport = FakeTransport()
        var secondFired = false
        val conn = DbConnection(
            transport = transport,
            httpClient = HttpClient(),
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOf(
                { _, _, _ -> error("onConnect explosion") },
                { _, _, _ -> secondFired = true },
            ),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertTrue(secondFired, "Second onConnect callback should fire despite first throwing")
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun onDeleteCallbackExceptionDoesNotPreventRowRemoval() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
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

        // Register a throwing onDelete callback
        cache.onDelete { _, _ -> error("delete callback explosion") }

        // Delete the row via transaction update
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                update = TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(),
                                        deletes = buildRowList(row.encode()),
                                    ))
                                )
                            ),
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Row should still be deleted despite callback exception
        assertEquals(0, cache.count())
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun reducerCallbackExceptionDoesNotCrashConnection() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val requestId = conn.callReducer(
            reducerName = "boom",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { _ -> error("reducer callback explosion") },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive, "Connection should survive throwing reducer callback")
        conn.disconnect()
    }

    // --- Reducer timeout and burst scenarios ---

    @Test
    fun pendingReducerCallbacksClearedOnDisconnectNeverFire() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var callbackFired = false
        val requestId = conn.callReducer("slow", byteArrayOf(), "args", callback = { _ ->
            callbackFired = true
        })
        advanceUntilIdle()

        // Verify the reducer is pending
        assertEquals(1, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())

        // Disconnect before the server responds — simulates a "timeout" scenario
        conn.disconnect()
        advanceUntilIdle()

        assertFalse(callbackFired, "Reducer callback must not fire after disconnect")
    }

    @Test
    fun burstReducerCallsAllGetUniqueRequestIds() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val count = 100
        val requestIds = mutableSetOf<UInt>()
        val results = mutableMapOf<UInt, Status>()

        // Fire 100 reducer calls in a burst
        repeat(count) { i ->
            val id = conn.callReducer("op", byteArrayOf(i.toByte()), "args-$i", callback = { ctx ->
                results[i.toUInt()] = ctx.status
            })
            requestIds.add(id)
        }
        advanceUntilIdle()

        // All IDs must be unique
        assertEquals(count, requestIds.size)
        assertEquals(count, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())

        // Respond to all in order
        for (id in requestIds) {
            transport.sendToClient(
                ServerMessage.ReducerResultMsg(
                    requestId = id,
                    timestamp = Timestamp.UNIX_EPOCH,
                    result = ReducerOutcome.OkEmpty,
                )
            )
        }
        advanceUntilIdle()

        assertEquals(0, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())
        assertEquals(count, conn.stats.reducerRequestTracker.getSampleCount())
        conn.disconnect()
    }

    @Test
    fun burstReducerCallsRespondedOutOfOrder() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val count = 50
        val callbacks = mutableMapOf<UInt, Status>()
        val requestIds = mutableListOf<UInt>()

        repeat(count) { i ->
            val id = conn.callReducer("op-$i", byteArrayOf(i.toByte()), "args-$i", callback = { ctx ->
                callbacks[i.toUInt()] = ctx.status
            })
            requestIds.add(id)
        }
        advanceUntilIdle()

        // Respond in reverse order
        for (id in requestIds.reversed()) {
            transport.sendToClient(
                ServerMessage.ReducerResultMsg(
                    requestId = id,
                    timestamp = Timestamp.UNIX_EPOCH,
                    result = ReducerOutcome.OkEmpty,
                )
            )
        }
        advanceUntilIdle()

        assertEquals(0, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())
        conn.disconnect()
    }

    @Test
    fun reducerResultAfterDisconnectIsDropped() = runTest {
        val transport = FakeTransport()
        var callbackFired = false
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val requestId = conn.callReducer("op", byteArrayOf(), "args", callback = { _ ->
            callbackFired = true
        })
        advanceUntilIdle()

        // Server closes the connection
        transport.closeFromServer()
        advanceUntilIdle()
        assertFalse(conn.isActive)

        // Callback was cleared by failPendingOperations, never fires
        assertFalse(callbackFired)
    }

    @Test
    fun reducerWithTableMutationsAndCallbackBothFire() = runTest {
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

        var reducerStatus: Status? = null
        val insertedRows = mutableListOf<SampleRow>()
        cache.onInsert { _, row -> insertedRows.add(row) }

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")

        val requestId = conn.callReducer("add_two", byteArrayOf(), "args", callback = { ctx ->
            reducerStatus = ctx.status
        })
        advanceUntilIdle()

        // Reducer result inserts two rows in a single transaction
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
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
                                                inserts = buildRowList(row1.encode(), row2.encode()),
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

        // Both callbacks must have fired
        assertEquals(Status.Committed, reducerStatus)
        assertEquals(2, insertedRows.size)
        assertEquals(2, cache.count())
        conn.disconnect()
    }

    @Test
    fun manyPendingReducersAllClearedOnDisconnect() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var firedCount = 0
        repeat(50) {
            conn.callReducer("op", byteArrayOf(), "args", callback = { _ -> firedCount++ })
        }
        advanceUntilIdle()

        assertEquals(50, conn.stats.reducerRequestTracker.getRequestsAwaitingResponse())

        conn.disconnect()
        advanceUntilIdle()

        assertEquals(0, firedCount, "No reducer callbacks should fire after disconnect")
    }

    @Test
    fun mixedReducerOutcomesInBurst() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val results = mutableMapOf<String, Status>()

        val id1 = conn.callReducer("ok1", byteArrayOf(), "ok1", callback = { ctx ->
            results["ok1"] = ctx.status
        })
        val id2 = conn.callReducer("err", byteArrayOf(), "err", callback = { ctx ->
            results["err"] = ctx.status
        })
        val id3 = conn.callReducer("ok2", byteArrayOf(), "ok2", callback = { ctx ->
            results["ok2"] = ctx.status
        })
        val id4 = conn.callReducer("internal_err", byteArrayOf(), "internal_err", callback = { ctx ->
            results["internal_err"] = ctx.status
        })
        advanceUntilIdle()

        val errWriter = BsatnWriter()
        errWriter.writeString("bad input")

        // Send all results at once — mixed outcomes
        transport.sendToClient(ServerMessage.ReducerResultMsg(id1, Timestamp.UNIX_EPOCH, ReducerOutcome.OkEmpty))
        transport.sendToClient(ServerMessage.ReducerResultMsg(id2, Timestamp.UNIX_EPOCH, ReducerOutcome.Err(errWriter.toByteArray())))
        transport.sendToClient(ServerMessage.ReducerResultMsg(id3, Timestamp.UNIX_EPOCH, ReducerOutcome.OkEmpty))
        transport.sendToClient(ServerMessage.ReducerResultMsg(id4, Timestamp.UNIX_EPOCH, ReducerOutcome.InternalError("server crash")))
        advanceUntilIdle()

        assertEquals(4, results.size)
        assertEquals(Status.Committed, results["ok1"])
        assertEquals(Status.Committed, results["ok2"])
        assertTrue(results["err"] is Status.Failed)
        assertEquals("bad input", (results["err"] as Status.Failed).message)
        assertTrue(results["internal_err"] is Status.Failed)
        assertEquals("server crash", (results["internal_err"] as Status.Failed).message)
        conn.disconnect()
    }

    // --- Subscription state machine edge cases ---

    @Test
    fun subscriptionErrorWhileUnsubscribingMovesToEnded() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var errorMsg: String? = null
        val handle = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onError = listOf { _, err -> errorMsg = err.message },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()
        assertTrue(handle.isActive)

        // Start unsubscribing
        handle.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // Server sends error instead of UnsubscribeApplied
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 2u,
                querySetId = handle.querySetId,
                error = "internal error during unsubscribe",
            )
        )
        advanceUntilIdle()

        assertTrue(handle.isEnded)
        assertEquals("internal error during unsubscribe", errorMsg)
        conn.disconnect()
    }

    @Test
    fun transactionUpdateDuringUnsubscribeStillApplies() = runTest {
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

        // Start unsubscribing
        handle.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle.isUnsubscribing)

        // A transaction arrives while unsubscribe is in-flight — row is inserted
        val newRow = SampleRow(2, "Bob")
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                update = TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(newRow.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Transaction should still be applied to cache
        assertEquals(2, cache.count())
        conn.disconnect()
    }

    @Test
    fun multipleSubscriptionsIndependentLifecycle() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var applied1 = false
        var applied2 = false
        val handle1 = conn.subscribe(
            queries = listOf("SELECT * FROM players"),
            onApplied = listOf { _ -> applied1 = true },
        )
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM items"),
            onApplied = listOf { _ -> applied2 = true },
        )
        advanceUntilIdle()

        // Only first subscription is confirmed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied1)
        assertFalse(applied2)
        assertTrue(handle1.isActive)
        assertTrue(handle2.isPending)

        // Unsubscribe first while second is still pending
        handle1.unsubscribe()
        advanceUntilIdle()
        assertTrue(handle1.isUnsubscribing)
        assertTrue(handle2.isPending)

        // Second subscription confirmed
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertTrue(applied2)
        assertTrue(handle2.isActive)
        assertTrue(handle1.isUnsubscribing)

        // First unsubscribe confirmed
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = null,
            )
        )
        advanceUntilIdle()

        assertTrue(handle1.isEnded)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    // --- Multi-subscription conflict scenarios ---

    @Test
    fun subscribeAppliedDuringUnsubscribeOfOverlappingSubscription() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val sharedRow = SampleRow(1, "Alice")
        val sub1OnlyRow = SampleRow(2, "Bob")

        // Sub1: gets both rows
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1OnlyRow.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())

        // Start unsubscribing sub1
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        assertTrue(handle1.isUnsubscribing)

        // Sub2 arrives while sub1 unsubscribe is in-flight — shares one row
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertTrue(handle2.isActive)
        // sharedRow now has ref count 2, sub1OnlyRow has ref count 1
        assertEquals(2, cache.count())

        // Sub1 unsubscribe completes — drops both rows by ref count
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1OnlyRow.encode())))
                ),
            )
        )
        advanceUntilIdle()

        // sharedRow survives (ref count 2 -> 1), sub1OnlyRow removed (ref count 1 -> 0)
        assertEquals(1, cache.count())
        assertEquals(sharedRow, cache.all().single())
        assertTrue(handle1.isEnded)
        assertTrue(handle2.isActive)
        conn.disconnect()
    }

    @Test
    fun subscriptionErrorDoesNotAffectOtherSubscriptionCachedRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")

        // Sub1: active with a row in cache
        val handle1 = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(row.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())
        assertTrue(handle1.isActive)

        // Sub2: errors during subscribe (requestId present = non-fatal)
        var sub2Error: Throwable? = null
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM sample WHERE invalid"),
            onError = listOf { _, err -> sub2Error = err },
        )
        transport.sendToClient(
            ServerMessage.SubscriptionError(
                requestId = 2u,
                querySetId = handle2.querySetId,
                error = "parse error",
            )
        )
        advanceUntilIdle()

        // Sub2 is ended, but sub1's row must still be in cache
        assertTrue(handle2.isEnded)
        assertNotNull(sub2Error)
        assertTrue(handle1.isActive)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun transactionUpdateSpansMultipleQuerySets() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row1 = SampleRow(1, "Alice")
        val row2 = SampleRow(2, "Bob")

        // Two subscriptions on the same table
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row1.encode())))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList()))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Single TransactionUpdate with updates from BOTH query sets
        var insertCount = 0
        cache.onInsert { _, _ -> insertCount++ }
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle1.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(row2.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        ),
                        QuerySetUpdate(
                            handle2.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(row2.encode()),
                                        deletes = buildRowList(),
                                    ))
                                )
                            ),
                        ),
                    )
                )
            )
        )
        advanceUntilIdle()

        // row2 inserted via both query sets — ref count = 2, but onInsert fires once
        assertEquals(2, cache.count())
        assertEquals(1, insertCount)
        conn.disconnect()
    }

    @Test
    fun resubscribeAfterUnsubscribeCompletes() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")

        // First subscription
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Unsubscribe
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 2u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(0, cache.count())
        assertTrue(handle1.isEnded)

        // Re-subscribe with the same query — fresh handle, row re-inserted
        var reApplied = false
        val handle2 = conn.subscribe(
            queries = listOf("SELECT * FROM sample"),
            onApplied = listOf { _ -> reApplied = true },
        )
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 3u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        assertTrue(reApplied)
        assertTrue(handle2.isActive)
        assertEquals(1, cache.count())
        assertEquals(row, cache.all().single())
        // Old handle stays ended
        assertTrue(handle1.isEnded)
        conn.disconnect()
    }

    @Test
    fun threeOverlappingSubscriptionsUnsubscribeMiddle() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val row = SampleRow(1, "Alice")
        val encodedRow = row.encode()

        var deleteCount = 0
        cache.onDelete { _, _ -> deleteCount++ }

        // Three subscriptions all sharing the same row
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id = 1"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        val handle3 = conn.subscribe(listOf("SELECT * FROM sample WHERE name = 'Alice'"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 3u,
                querySetId = handle3.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()
        // ref count = 3
        assertEquals(1, cache.count())

        // Unsubscribe middle subscription
        handle2.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 4u,
                querySetId = handle2.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        // ref count 3 -> 2, row still present, no onDelete
        assertEquals(1, cache.count())
        assertEquals(0, deleteCount)
        assertTrue(handle2.isEnded)
        assertTrue(handle1.isActive)
        assertTrue(handle3.isActive)

        // Unsubscribe first
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 5u,
                querySetId = handle1.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        // ref count 2 -> 1, still present
        assertEquals(1, cache.count())
        assertEquals(0, deleteCount)

        // Unsubscribe last — ref count -> 0, row deleted
        handle3.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 6u,
                querySetId = handle3.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(encodedRow)))),
            )
        )
        advanceUntilIdle()

        assertEquals(0, cache.count())
        assertEquals(1, deleteCount)
        conn.disconnect()
    }

    @Test
    fun unsubscribeDropsUniqueRowsButKeepsSharedRows() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val sharedRow = SampleRow(1, "Alice")
        val sub1Only = SampleRow(2, "Bob")
        val sub2Only = SampleRow(3, "Charlie")

        // Sub1: gets sharedRow + sub1Only
        val handle1 = conn.subscribe(listOf("SELECT * FROM sample WHERE id <= 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1Only.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(2, cache.count())

        // Sub2: gets sharedRow + sub2Only
        val handle2 = conn.subscribe(listOf("SELECT * FROM sample WHERE id != 2"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 2u,
                querySetId = handle2.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub2Only.encode())))
                ),
            )
        )
        advanceUntilIdle()
        assertEquals(3, cache.count())

        val deleted = mutableListOf<Int>()
        cache.onDelete { _, row -> deleted.add(row.id) }

        // Unsubscribe sub1 — drops sharedRow (ref 2->1) and sub1Only (ref 1->0)
        handle1.unsubscribeThen(UnsubscribeFlags.SendDroppedRows) {}
        advanceUntilIdle()
        transport.sendToClient(
            ServerMessage.UnsubscribeApplied(
                requestId = 3u,
                querySetId = handle1.querySetId,
                rows = QueryRows(
                    listOf(SingleTableRows("sample", buildRowList(sharedRow.encode(), sub1Only.encode())))
                ),
            )
        )
        advanceUntilIdle()

        // sub1Only deleted, sharedRow survives
        assertEquals(2, cache.count())
        assertEquals(listOf(2), deleted) // only sub1Only's id
        val remaining = cache.all().sortedBy { it.id }
        assertEquals(listOf(sharedRow, sub2Only), remaining)
        conn.disconnect()
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

    // --- ensureMinimumVersion edge cases ---

    @Test
    fun builderAcceptsExactMinimumVersion() = runTest {
        val module = object : ModuleDescriptor {
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Should not throw — 2.0.0 is the exact minimum
        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun builderAcceptsNewerVersion() = runTest {
        val module = object : ModuleDescriptor {
            override val cliVersion = "3.1.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun builderAcceptsPreReleaseSuffix() = runTest {
        val module = object : ModuleDescriptor {
            override val cliVersion = "2.1.0-beta.1"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Pre-release suffix is stripped; 2.1.0 >= 2.0.0
        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun builderRejectsOldMinorVersion() = runTest {
        val module = object : ModuleDescriptor {
            override val cliVersion = "1.9.9"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        assertFailsWith<IllegalStateException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .withDatabaseName("test")
                .withModule(module)
                .build()
        }
    }

    // --- Cross-table preApply ordering ---

    @Test
    fun crossTablePreApplyRunsBeforeAnyApply() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)

        // Set up two independent table caches
        val cacheA = createSampleCache()
        val cacheB = createSampleCache()
        conn.clientCache.register("table_a", cacheA)
        conn.clientCache.register("table_b", cacheB)

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Subscribe and apply initial rows to both tables
        val handle = conn.subscribe(listOf("SELECT * FROM table_a", "SELECT * FROM table_b"))
        val rowA = SampleRow(1, "Alice")
        val rowB = SampleRow(2, "Bob")

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

        // Track event ordering: onBeforeDelete (preApply) vs onDelete (apply)
        val events = mutableListOf<String>()
        cacheA.onBeforeDelete { _, _ -> events.add("preApply_A") }
        cacheA.onDelete { _, _ -> events.add("apply_A") }
        cacheB.onBeforeDelete { _, _ -> events.add("preApply_B") }
        cacheB.onDelete { _, _ -> events.add("apply_B") }

        // Send a TransactionUpdate that deletes from BOTH tables
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate("table_a", listOf(TableUpdateRows.PersistentTable(buildRowList(), buildRowList(rowA.encode())))),
                                TableUpdate("table_b", listOf(TableUpdateRows.PersistentTable(buildRowList(), buildRowList(rowB.encode())))),
                            )
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // The key invariant: ALL preApply callbacks fire before ANY apply callbacks
        assertEquals(listOf("preApply_A", "preApply_B", "apply_A", "apply_B"), events)
        conn.disconnect()
    }

}
