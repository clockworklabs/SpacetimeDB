package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.protocol.ClientMessage
import com.clockworklabs.spacetimedb.protocol.QuerySetId
import com.clockworklabs.spacetimedb.protocol.ServerMessage
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ProtocolTest {

    @Test
    fun encodeSubscribeMessage() {
        val msg = ClientMessage.Subscribe(
            requestId = 1u,
            querySetId = QuerySetId(100u),
            queryStrings = listOf("SELECT * FROM users"),
        )
        val bytes = msg.encode()
        val reader = BsatnReader(bytes)
        assertEquals(0, reader.readTag().toInt())
        assertEquals(1u, reader.readU32())
        assertEquals(100u, reader.readU32())
        val queries = reader.readArray { it.readString() }
        assertEquals(listOf("SELECT * FROM users"), queries)
    }

    @Test
    fun encodeCallReducerMessage() {
        val args = byteArrayOf(10, 20, 30)
        val msg = ClientMessage.CallReducer(
            requestId = 5u,
            reducer = "add_user",
            args = args,
        )
        val bytes = msg.encode()
        val reader = BsatnReader(bytes)
        assertEquals(3, reader.readTag().toInt())
        assertEquals(5u, reader.readU32())
        assertEquals(0u.toUByte(), reader.readU8())
        assertEquals("add_user", reader.readString())
        assertTrue(args.contentEquals(reader.readByteArray()))
    }

    @Test
    fun encodeUnsubscribeMessage() {
        val msg = ClientMessage.Unsubscribe(
            requestId = 2u,
            querySetId = QuerySetId(50u),
        )
        val bytes = msg.encode()
        val reader = BsatnReader(bytes)
        assertEquals(1, reader.readTag().toInt())
        assertEquals(2u, reader.readU32())
        assertEquals(50u, reader.readU32())
    }

    @Test
    fun encodeOneOffQueryMessage() {
        val msg = ClientMessage.OneOffQuery(
            requestId = 3u,
            queryString = "SELECT count(*) FROM users",
        )
        val bytes = msg.encode()
        val reader = BsatnReader(bytes)
        assertEquals(2, reader.readTag().toInt())
        assertEquals(3u, reader.readU32())
        assertEquals("SELECT count(*) FROM users", reader.readString())
    }

    @Test
    fun decodeInitialConnection() {
        val writer = BsatnWriter()
        writer.writeTag(0u)
        writer.writeBytes(ByteArray(32) { it.toByte() })
        writer.writeBytes(ByteArray(16) { (it + 100).toByte() })
        writer.writeString("test-token-abc")

        val msg = ServerMessage.decode(writer.toByteArray())
        assertTrue(msg is ServerMessage.InitialConnection)
        assertEquals("test-token-abc", msg.token)
        assertEquals(ByteArray(32) { it.toByte() }.toList(), msg.identity.bytes.toList())
        assertEquals(ByteArray(16) { (it + 100).toByte() }.toList(), msg.connectionId.bytes.toList())
    }

    @Test
    fun identityFromHex() {
        val hex = "00" + "01" + "02" + "03" + "04" + "05" + "06" + "07" +
            "08" + "09" + "0a" + "0b" + "0c" + "0d" + "0e" + "0f" +
            "10" + "11" + "12" + "13" + "14" + "15" + "16" + "17" +
            "18" + "19" + "1a" + "1b" + "1c" + "1d" + "1e" + "1f"
        val identity = Identity.fromHex(hex)
        assertEquals(0, identity.bytes[0].toInt())
        assertEquals(31, identity.bytes[31].toInt())
        assertEquals(hex, identity.toHex())
    }

    @Test
    fun identityBsatnRoundTrip() {
        val original = Identity(ByteArray(32) { (it * 7).toByte() })
        val writer = BsatnWriter()
        Identity.write(writer, original)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Identity.read(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun connectionIdBsatnRoundTrip() {
        val original = ConnectionId(ByteArray(16) { (it * 3).toByte() })
        val writer = BsatnWriter()
        ConnectionId.write(writer, original)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = ConnectionId.read(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun timestampBsatnRoundTrip() {
        val original = Timestamp(1234567890123L)
        val writer = BsatnWriter()
        Timestamp.write(writer, original)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Timestamp.read(reader)
        assertEquals(original, decoded)
    }
}
