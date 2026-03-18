package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ReducerIntegrationTest {

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
        assertEquals(1, conn.stats.reducerRequestTracker.requestsAwaitingResponse)

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
        assertEquals(count, conn.stats.reducerRequestTracker.requestsAwaitingResponse)

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

        assertEquals(0, conn.stats.reducerRequestTracker.requestsAwaitingResponse)
        assertEquals(count, conn.stats.reducerRequestTracker.sampleCount)
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

        assertEquals(0, conn.stats.reducerRequestTracker.requestsAwaitingResponse)
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

        assertEquals(50, conn.stats.reducerRequestTracker.requestsAwaitingResponse)

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

    // --- typedArgs round-trip through ReducerCallInfo ---

    @Test
    fun callReducerTypedArgsRoundTripThroughCallInfo() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        data class MyArgs(val x: Int, val y: String)
        val original = MyArgs(42, "hello")
        var receivedArgs: MyArgs? = null
        val requestId = conn.callReducer(
            reducerName = "typed_op",
            encodedArgs = byteArrayOf(),
            typedArgs = original,
            callback = { ctx -> receivedArgs = ctx.args },
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

        // The typed args must survive the round-trip through ReducerCallInfo(Any)
        // back to EventContext.Reducer<MyArgs>.args without corruption.
        assertEquals(original, receivedArgs)
        conn.disconnect()
    }
}
