package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.UnsubscribeFlags
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ClientMessageTest {

    // ---- Subscribe (tag 0) ----

    @Test
    fun `subscribe encodes correctly`() {
        val msg = ClientMessage.Subscribe(
            requestId = 42u,
            querySetId = QuerySetId(7u),
            queryStrings = listOf("SELECT * FROM Players", "SELECT * FROM Items"),
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(0, reader.readSumTag().toInt(), "tag")
        assertEquals(42u, reader.readU32(), "requestId")
        assertEquals(7u, reader.readU32(), "querySetId")
        assertEquals(2, reader.readArrayLen(), "query count")
        assertEquals("SELECT * FROM Players", reader.readString())
        assertEquals("SELECT * FROM Items", reader.readString())
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `subscribe empty queries`() {
        val msg = ClientMessage.Subscribe(
            requestId = 0u,
            querySetId = QuerySetId(0u),
            queryStrings = emptyList(),
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(0, reader.readSumTag().toInt())
        assertEquals(0u, reader.readU32())
        assertEquals(0u, reader.readU32())
        assertEquals(0, reader.readArrayLen())
        assertEquals(0, reader.remaining)
    }

    // ---- Unsubscribe (tag 1) ----

    @Test
    fun `unsubscribe default flags`() {
        val msg = ClientMessage.Unsubscribe(
            requestId = 10u,
            querySetId = QuerySetId(5u),
            flags = UnsubscribeFlags.Default,
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(1, reader.readSumTag().toInt(), "tag")
        assertEquals(10u, reader.readU32(), "requestId")
        assertEquals(5u, reader.readU32(), "querySetId")
        assertEquals(0, reader.readSumTag().toInt(), "flags = Default")
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `unsubscribe send dropped rows flags`() {
        val msg = ClientMessage.Unsubscribe(
            requestId = 10u,
            querySetId = QuerySetId(5u),
            flags = UnsubscribeFlags.SendDroppedRows,
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(1, reader.readSumTag().toInt())
        assertEquals(10u, reader.readU32())
        assertEquals(5u, reader.readU32())
        assertEquals(1, reader.readSumTag().toInt(), "flags = SendDroppedRows")
        assertEquals(0, reader.remaining)
    }

    // ---- OneOffQuery (tag 2) ----

    @Test
    fun `one off query encodes correctly`() {
        val msg = ClientMessage.OneOffQuery(
            requestId = 99u,
            queryString = "SELECT * FROM Players WHERE id = 1",
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(2, reader.readSumTag().toInt(), "tag")
        assertEquals(99u, reader.readU32(), "requestId")
        assertEquals("SELECT * FROM Players WHERE id = 1", reader.readString())
        assertEquals(0, reader.remaining)
    }

    // ---- CallReducer (tag 3) ----

    @Test
    fun `call reducer encodes correctly`() {
        val args = byteArrayOf(1, 2, 3, 4)
        val msg = ClientMessage.CallReducer(
            requestId = 7u,
            flags = 0u,
            reducer = "add_player",
            args = args,
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(3, reader.readSumTag().toInt(), "tag")
        assertEquals(7u, reader.readU32(), "requestId")
        assertEquals(0u.toUByte(), reader.readU8(), "flags")
        assertEquals("add_player", reader.readString(), "reducer")
        assertTrue(args.contentEquals(reader.readByteArray()), "args")
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `call reducer equality`() {
        val msg1 = ClientMessage.CallReducer(1u, 0u, "test", byteArrayOf(1, 2, 3))
        val msg2 = ClientMessage.CallReducer(1u, 0u, "test", byteArrayOf(1, 2, 3))
        val msg3 = ClientMessage.CallReducer(1u, 0u, "test", byteArrayOf(4, 5, 6))

        assertEquals(msg1, msg2)
        assertEquals(msg1.hashCode(), msg2.hashCode())
        assertTrue(msg1 != msg3)
    }

    // ---- CallProcedure (tag 4) ----

    @Test
    fun `call procedure encodes correctly`() {
        val args = byteArrayOf(10, 20)
        val msg = ClientMessage.CallProcedure(
            requestId = 3u,
            flags = 1u,
            procedure = "get_player_stats",
            args = args,
        )
        val bytes = ClientMessage.encodeToBytes(msg)
        val reader = BsatnReader(bytes)

        assertEquals(4, reader.readSumTag().toInt(), "tag")
        assertEquals(3u, reader.readU32(), "requestId")
        assertEquals(1u.toUByte(), reader.readU8(), "flags")
        assertEquals("get_player_stats", reader.readString(), "procedure")
        assertTrue(args.contentEquals(reader.readByteArray()), "args")
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `call procedure equality`() {
        val msg1 = ClientMessage.CallProcedure(1u, 0u, "proc", byteArrayOf(1))
        val msg2 = ClientMessage.CallProcedure(1u, 0u, "proc", byteArrayOf(1))
        val msg3 = ClientMessage.CallProcedure(1u, 0u, "proc", byteArrayOf(2))

        assertEquals(msg1, msg2)
        assertEquals(msg1.hashCode(), msg2.hashCode())
        assertTrue(msg1 != msg3)
    }
}
