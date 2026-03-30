package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Duration

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class StatsIntegrationTest {

    // --- Stats tracking ---

    @Test
    fun `stats subscription tracker increments on subscribe applied`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.subscriptionRequestTracker
        assertEquals(0, tracker.sampleCount)

        val handle = conn.subscribe(listOf("SELECT * FROM player"))
        // Request started but not yet finished
        assertEquals(1, tracker.requestsAwaitingResponse)

        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = emptyQueryRows(),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.sampleCount)
        assertEquals(0, tracker.requestsAwaitingResponse)
        conn.disconnect()
    }

    @Test
    fun `stats reducer tracker increments on reducer result`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.reducerRequestTracker
        assertEquals(0, tracker.sampleCount)

        val requestId = conn.callReducer("add", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()
        assertEquals(1, tracker.requestsAwaitingResponse)

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.sampleCount)
        assertEquals(0, tracker.requestsAwaitingResponse)
        conn.disconnect()
    }

    @Test
    fun `stats procedure tracker increments on procedure result`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.procedureRequestTracker
        assertEquals(0, tracker.sampleCount)

        val requestId = conn.callProcedure("my_proc", byteArrayOf(), callback = null)
        advanceUntilIdle()
        assertEquals(1, tracker.requestsAwaitingResponse)

        transport.sendToClient(
            ServerMessage.ProcedureResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                status = ProcedureStatus.Returned(byteArrayOf()),
                totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.sampleCount)
        assertEquals(0, tracker.requestsAwaitingResponse)
        conn.disconnect()
    }

    @Test
    fun `stats one off tracker increments on query result`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.oneOffRequestTracker
        assertEquals(0, tracker.sampleCount)

        val requestId = conn.oneOffQuery("SELECT 1") { _ -> }
        advanceUntilIdle()
        assertEquals(1, tracker.requestsAwaitingResponse)

        transport.sendToClient(
            ServerMessage.OneOffQueryResult(
                requestId = requestId,
                result = QueryResult.Ok(emptyQueryRows()),
            )
        )
        advanceUntilIdle()

        assertEquals(1, tracker.sampleCount)
        assertEquals(0, tracker.requestsAwaitingResponse)
        conn.disconnect()
    }

    @Test
    fun `stats apply message tracker increments on every server message`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val tracker = conn.stats.applyMessageTracker
        // InitialConnection is the first message processed
        assertEquals(1, tracker.sampleCount)

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
        assertEquals(2, tracker.sampleCount)

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
        assertEquals(3, tracker.sampleCount)

        conn.disconnect()
    }
}
