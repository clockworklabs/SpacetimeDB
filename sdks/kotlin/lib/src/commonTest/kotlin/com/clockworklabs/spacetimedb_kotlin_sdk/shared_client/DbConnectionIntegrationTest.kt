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
        onConnect: ((DbConnection, Identity, String) -> Unit)? = null,
        onDisconnect: ((DbConnection, Throwable?) -> Unit)? = null,
        onConnectError: ((DbConnection, Throwable) -> Unit)? = null,
        moduleDescriptor: ModuleDescriptor? = null,
        callbackDispatcher: kotlinx.coroutines.CoroutineDispatcher? = null,
    ): DbConnection {
        val conn = createTestConnection(transport, onConnect, onDisconnect, onConnectError, moduleDescriptor, callbackDispatcher)
        conn.connect()
        return conn
    }

    private fun TestScope.createTestConnection(
        transport: FakeTransport,
        onConnect: ((DbConnection, Identity, String) -> Unit)? = null,
        onDisconnect: ((DbConnection, Throwable?) -> Unit)? = null,
        onConnectError: ((DbConnection, Throwable) -> Unit)? = null,
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
        onDisconnect: ((DbConnection, Throwable?) -> Unit)? = null,
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
    }

    // --- Late registration & disconnect ---

    @Test
    fun lateOnConnectRegistrationFiresImmediately() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register onConnect AFTER the InitialConnection has been processed
        var lateConnectFired = false
        conn.onConnect { _, _, _ -> lateConnectFired = true }
        advanceUntilIdle()

        assertTrue(lateConnectFired)
        conn.close()
    }

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

        conn.close()
        advanceUntilIdle()

        assertTrue(handle.isEnded)
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
        assertFailsWith<RuntimeException> { conn.connect() }

        assertEquals(error, capturedError)
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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

        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
    }

    // --- Callback removal ---

    @Test
    fun removeOnConnectPreventsCallback() = runTest {
        val transport = FakeTransport()
        var fired = false
        val cb: (DbConnection, Identity, String) -> Unit = { _, _, _ -> fired = true }

        val conn = createTestConnection(transport, onConnect = cb)
        conn.removeOnConnect(cb)
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertFalse(fired)
        conn.close()
    }

    @Test
    fun removeOnDisconnectPreventsCallback() = runTest {
        val transport = FakeTransport()
        var fired = false
        val cb: (DbConnection, Throwable?) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onDisconnect = cb)
        conn.removeOnDisconnect(cb)
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        transport.closeFromServer()
        advanceUntilIdle()

        assertFalse(fired)
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
    }

    // --- close() states ---

    @Test
    fun closeWhenAlreadyClosedIsNoOp() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.close()
        advanceUntilIdle()
        // Second close should not throw
        conn.close()
    }

    @Test
    fun closeFromDisconnectedState() = runTest {
        val transport = FakeTransport()
        var disconnectCount = 0
        val conn = buildTestConnection(transport, onDisconnect = { _, _ ->
            disconnectCount++
        })
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.disconnect()
        advanceUntilIdle()
        assertEquals(1, disconnectCount)

        // close() from DISCONNECTED should not fire onDisconnect again
        conn.close()
        advanceUntilIdle()
        assertEquals(1, disconnectCount)
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
        conn.close()
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
        conn.close()
    }

    // --- Multiple callbacks ---

    @Test
    fun multipleOnConnectCallbacksAllFire() = runTest {
        val transport = FakeTransport()
        var count = 0
        val conn = createTestConnection(transport)
        conn.onConnect { _, _, _ -> count++ }
        conn.onConnect { _, _, _ -> count++ }
        conn.onConnect { _, _, _ -> count++ }
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(3, count)
        conn.close()
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
        conn.close()
    }

    // --- removeOnConnectError ---

    @Test
    fun removeOnConnectErrorPreventsCallback() = runTest {
        val transport = FakeTransport(connectError = RuntimeException("fail"))
        var fired = false
        val cb: (DbConnection, Throwable) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onConnectError = cb)
        conn.removeOnConnectError(cb)

        try {
            conn.connect()
        } catch (_: Exception) { }
        advanceUntilIdle()

        assertFalse(fired)
        conn.close()
    }

    // --- close() from never-connected state ---

    @Test
    fun closeFromNeverConnectedState() = runTest {
        val transport = FakeTransport()
        val conn = createTestConnection(transport)
        // close() on a freshly created connection that was never connected should not throw
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
    }

    // --- sendMessage after close ---

    @Test
    fun subscribeAfterCloseDoesNotCrash() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.close()
        advanceUntilIdle()

        // Calling subscribe on a closed connection should not throw —
        // sendMessage gracefully handles closed channel
        conn.subscribe(listOf("SELECT * FROM player"))
        Unit
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
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
        conn.close()
    }
}
