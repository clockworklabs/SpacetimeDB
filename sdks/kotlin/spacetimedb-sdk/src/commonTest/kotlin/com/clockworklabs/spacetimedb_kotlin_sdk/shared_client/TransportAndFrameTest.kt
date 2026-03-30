package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.SpacetimeTransport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import io.ktor.client.HttpClient
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertContains
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class TransportAndFrameTest {

    // --- Mid-stream transport failures ---

    @Test
    fun `transport error fires on disconnect with error`() = runTest {
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
    fun `transport error fails pending subscription`() = runTest {
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
    fun `transport error fails pending reducer callback`() = runTest {
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
    fun `send error does not crash receive loop`() = runTest {
        val transport = FakeTransport()
        // Use a CoroutineExceptionHandler so the unhandled send-loop exception
        // doesn't propagate to runTest — we're testing that the receive loop survives.
        val handler = kotlinx.coroutines.CoroutineExceptionHandler { _, _ -> }
        val conn = DbConnection(
            transport = transport,
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
    fun `truncated bsatn frame fires on disconnect`() = runTest {
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
        writer.writeU256(TEST_IDENTITY.data) // identity
        writer.writeU128(TEST_CONNECTION_ID.data) // connectionId
        writer.writeString(TEST_TOKEN) // token
        rawTransport.sendRawToClient(writer.toByteArray())
        advanceUntilIdle()

        // Now send a truncated frame — only the tag byte, missing all fields
        rawTransport.sendRawToClient(byteArrayOf(0x00))
        advanceUntilIdle()

        assertNotNull(disconnectError)
        conn.disconnect()
    }

    @Test
    fun `invalid server message tag fires on disconnect`() = runTest {
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
    fun `empty frame fires on disconnect`() = runTest {
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

    @Test
    fun `truncated mid field disconnects`() = runTest {
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
    fun `invalid nested option tag disconnects`() = runTest {
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
    fun `invalid result tag in one off query disconnects`() = runTest {
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
    fun `oversized string length disconnects`() = runTest {
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
        w.writeU256(TEST_IDENTITY.data)
        w.writeU128(TEST_CONNECTION_ID.data)
        w.writeU32(0xFFFFFFFFu) // String length = 4GB — way more than remaining bytes
        rawTransport.sendRawToClient(w.toByteArray())
        advanceUntilIdle()

        assertNotNull(disconnectError)
    }

    @Test
    fun `invalid reducer outcome tag disconnects`() = runTest {
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
    fun `corrupt frame after established connection fails pending ops`() = runTest {
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
        assertEquals(1, conn.stats.reducerRequestTracker.requestsAwaitingResponse)

        // Corrupt frame kills the connection
        rawTransport.sendRawToClient(byteArrayOf(0xFE.toByte()))
        advanceUntilIdle()

        assertNotNull(disconnectError)
        assertFalse(conn.isActive)
        // Reducer callback should NOT have fired (it was discarded, not responded to)
        assertFalse(callbackFired)
    }

    @Test
    fun `garbage after valid message is ignored`() = runTest {
        // A fully valid InitialConnection with extra trailing bytes appended.
        // BsatnReader doesn't check that all bytes are consumed, so this should work.
        val rawTransport = RawFakeTransport()
        var connected = false
        var disconnectError: Throwable? = null
        val conn = DbConnection(
            transport = rawTransport,
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
    fun `all zero bytes frame disconnects`() = runTest {
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
    fun `valid tag with random garbage fields disconnects`() = runTest {
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

    @Test
    fun `valid frame after corrupted frame is not processed`() = runTest {
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

    // --- Protocol validation ---

    @Test
    fun `invalid protocol throws on connect`() = runTest {
        val transport = SpacetimeTransport(
            client = HttpClient(),
            baseUrl = "ftp://example.com",
            nameOrAddress = "test",
            connectionId = ConnectionId.random(),
        )
        val conn = DbConnection(
            transport = transport,
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = emptyList(),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        var connectError: Throwable? = null
        conn.onConnectError { _, err -> connectError = err }

        conn.connect()
        advanceUntilIdle()

        val err = assertNotNull(connectError)
        assertContains(assertNotNull(err.message), "Unsupported protocol")
        assertFalse(conn.isActive)
    }
}
