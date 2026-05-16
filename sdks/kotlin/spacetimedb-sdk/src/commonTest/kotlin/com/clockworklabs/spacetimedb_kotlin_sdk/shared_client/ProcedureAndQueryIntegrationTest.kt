package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class ProcedureAndQueryIntegrationTest {

    // --- Procedures ---

    @Test
    fun `call procedure sends client message`() = runTest {
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
    fun `procedure result fires callback`() = runTest {
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
    fun `procedure result internal error fires callback`() = runTest {
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

    // --- One-off queries ---

    @Test
    fun `one off query callback receives result`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var result: OneOffQueryResult? = null
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
        assertTrue(capturedResult is SdkResult.Success)
        conn.disconnect()
    }

    @Test
    fun `one off query suspend returns result`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Retrieve the requestId that will be assigned by inspecting sentMessages
        val beforeCount = transport.sentMessages.size
        // Launch the suspend query in a separate coroutine since it suspends
        var queryResult: OneOffQueryResult? = null
        launch {
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
        assertTrue(capturedQueryResult is SdkResult.Success)
        conn.disconnect()
    }

    // --- One-off query error ---

    @Test
    fun `one off query callback receives error`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        var result: OneOffQueryResult? = null
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
        assertTrue(capturedResult is SdkResult.Failure)
        val queryError = capturedResult.error
        assertTrue(queryError is QueryError.ServerError)
        assertEquals("syntax error", queryError.message)
        conn.disconnect()
    }

    // --- oneOffQuery cancellation ---

    @Test
    fun `one off query suspend cancellation cleans up callback`() = runTest {
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

    // --- oneOffQuery suspend with finite timeout ---

    @Test
    fun `one off query suspend times out when no response`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertFailsWith<kotlinx.coroutines.TimeoutCancellationException> {
            conn.oneOffQuery("SELECT * FROM sample", timeout = 1.milliseconds)
        }

        conn.disconnect()
    }

    // --- callProcedure without callback (fire-and-forget) ---

    @Test
    fun `call procedure without callback sends message`() = runTest {
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

    // --- Procedure result before identity is set ---

    @Test
    fun `procedure result before identity set is ignored`() = runTest {
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
}
