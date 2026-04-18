package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ScheduleAt
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import module_bindings.Message
import module_bindings.Note
import module_bindings.Reminder
import module_bindings.User
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.seconds

/**
 * BSATN binary serialization roundtrip tests.
 */
class BsatnRoundtripTest {

    // --- Primitive type roundtrips ---

    @Test
    fun `bool roundtrip`() {
        for (value in listOf(true, false)) {
            val writer = BsatnWriter()
            writer.writeBool(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readBool())
        }
    }

    @Test
    fun `byte and ubyte roundtrip`() {
        val writer = BsatnWriter()
        writer.writeByte(0x7F)
        writer.writeU8(0xFFu.toUByte())
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0x7F.toByte(), reader.readByte())
        assertEquals(0xFFu.toUByte(), reader.readU8())
    }

    @Test
    fun `i8 roundtrip`() {
        for (value in listOf(Byte.MIN_VALUE, 0.toByte(), Byte.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeI8(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readI8())
        }
    }

    @Test
    fun `u8 roundtrip`() {
        for (value in listOf(UByte.MIN_VALUE, UByte.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeU8(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readU8())
        }
    }

    @Test
    fun `i16 roundtrip`() {
        for (value in listOf(Short.MIN_VALUE, 0.toShort(), Short.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeI16(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readI16())
        }
    }

    @Test
    fun `u16 roundtrip`() {
        for (value in listOf(UShort.MIN_VALUE, UShort.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeU16(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readU16())
        }
    }

    @Test
    fun `i32 roundtrip`() {
        for (value in listOf(Int.MIN_VALUE, 0, Int.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeI32(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readI32())
        }
    }

    @Test
    fun `u32 roundtrip`() {
        for (value in listOf(UInt.MIN_VALUE, UInt.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeU32(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readU32())
        }
    }

    @Test
    fun `i64 roundtrip`() {
        for (value in listOf(Long.MIN_VALUE, 0L, Long.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeI64(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readI64())
        }
    }

    @Test
    fun `u64 roundtrip`() {
        for (value in listOf(ULong.MIN_VALUE, ULong.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeU64(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readU64())
        }
    }

    @Test
    fun `f32 roundtrip`() {
        for (value in listOf(0.0f, 1.5f, -3.14f, Float.MIN_VALUE, Float.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeF32(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readF32())
        }
    }

    @Test
    fun `f64 roundtrip`() {
        for (value in listOf(0.0, 2.718281828, -1.0e100, Double.MIN_VALUE, Double.MAX_VALUE)) {
            val writer = BsatnWriter()
            writer.writeF64(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readF64())
        }
    }

    @Test
    fun `string roundtrip`() {
        for (value in listOf("", "hello", "O'Reilly", "emoji: \uD83D\uDE00", "line\nnewline")) {
            val writer = BsatnWriter()
            writer.writeString(value)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(value, reader.readString())
        }
    }

    @Test
    fun `bytearray roundtrip`() {
        val value = byteArrayOf(0, 1, 127, -128, -1)
        val writer = BsatnWriter()
        writer.writeByteArray(value)
        val reader = BsatnReader(writer.toByteArray())
        assertTrue(value.contentEquals(reader.readByteArray()))
    }

    // --- Multiple values in sequence ---

    @Test
    fun `multiple primitives in sequence`() {
        val writer = BsatnWriter()
        writer.writeBool(true)
        writer.writeI32(42)
        writer.writeU64(999UL)
        writer.writeString("test")
        writer.writeF64(3.14)

        val reader = BsatnReader(writer.toByteArray())
        assertEquals(true, reader.readBool())
        assertEquals(42, reader.readI32())
        assertEquals(999UL, reader.readU64())
        assertEquals("test", reader.readString())
        assertEquals(3.14, reader.readF64())
    }

    // --- SDK type roundtrips ---

    @Test
    fun `Identity encode-decode roundtrip`() {
        val original = Identity.fromHexString("ab".repeat(32))
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Identity.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `Identity zero encode-decode roundtrip`() {
        val original = Identity.zero()
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Identity.decode(reader)
        assertEquals(original, decoded)
        assertEquals("00".repeat(32), decoded.toHexString())
    }

    @Test
    fun `ConnectionId encode-decode roundtrip`() {
        val original = ConnectionId.random()
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = ConnectionId.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `ConnectionId zero encode-decode roundtrip`() {
        val original = ConnectionId.zero()
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = ConnectionId.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `Timestamp encode-decode roundtrip`() {
        val original = Timestamp.now()
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Timestamp.decode(reader)
        assertEquals(original.microsSinceUnixEpoch, decoded.microsSinceUnixEpoch)
    }

    @Test
    fun `Timestamp UNIX_EPOCH encode-decode roundtrip`() {
        val original = Timestamp.UNIX_EPOCH
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Timestamp.decode(reader)
        assertEquals(original, decoded)
        assertEquals(0L, decoded.microsSinceUnixEpoch)
    }

    @Test
    fun `ScheduleAt Time encode-decode roundtrip`() {
        val original = ScheduleAt.Time(Timestamp.fromMillis(1700000000000L))
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = ScheduleAt.decode(reader)
        assertIs<ScheduleAt.Time>(decoded, "Should decode as Time")
        assertEquals(
            original.timestamp.microsSinceUnixEpoch,
            decoded.timestamp.microsSinceUnixEpoch
        )
    }

    @Test
    fun `ScheduleAt Interval encode-decode roundtrip`() {
        val original = ScheduleAt.interval(5.seconds)
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = ScheduleAt.decode(reader)
        assertEquals(original, decoded)
    }

    // --- Generated type roundtrips ---

    @Test
    fun `User encode-decode roundtrip with name`() {
        val original = User(
            identity = Identity.fromHexString("ab".repeat(32)),
            name = "Alice",
            online = true
        )
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = User.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `User encode-decode roundtrip with null name`() {
        val original = User(
            identity = Identity.fromHexString("cd".repeat(32)),
            name = null,
            online = false
        )
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = User.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `Message encode-decode roundtrip`() {
        val original = Message(
            id = 42UL,
            sender = Identity.fromHexString("ab".repeat(32)),
            sent = Timestamp.fromMillis(1700000000000L),
            text = "Hello, world!"
        )
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Message.decode(reader)
        assertEquals(original.id, decoded.id)
        assertEquals(original.sender, decoded.sender)
        assertEquals(original.sent.microsSinceUnixEpoch, decoded.sent.microsSinceUnixEpoch)
        assertEquals(original.text, decoded.text)
    }

    @Test
    fun `Note encode-decode roundtrip`() {
        val original = Note(
            id = 7UL,
            owner = Identity.fromHexString("ef".repeat(32)),
            content = "Test note with special chars: O'Reilly & \"quotes\"",
            tag = "test-tag"
        )
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Note.decode(reader)
        assertEquals(original, decoded)
    }

    @Test
    fun `Reminder encode-decode roundtrip`() {
        val original = Reminder(
            scheduledId = 100UL,
            scheduledAt = ScheduleAt.interval(10.seconds),
            text = "Don't forget!",
            owner = Identity.fromHexString("11".repeat(32))
        )
        val writer = BsatnWriter()
        original.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = Reminder.decode(reader)
        assertEquals(original, decoded)
    }

    // --- Writer utilities ---

    @Test
    fun `writer toByteArray returns correct length`() {
        val writer = BsatnWriter()
        writer.writeI32(42)
        assertEquals(4, writer.toByteArray().size, "i32 should be 4 bytes")
    }

    @Test
    fun `writer toBase64 produces non-empty string`() {
        val writer = BsatnWriter()
        writer.writeString("hello")
        val base64 = writer.toBase64()
        assertTrue(base64.isNotEmpty(), "Base64 should not be empty")
    }

    @Test
    fun `writer reset clears data`() {
        val writer = BsatnWriter()
        writer.writeI32(42)
        assertTrue(writer.toByteArray().isNotEmpty())
        writer.reset()
        assertEquals(0, writer.toByteArray().size, "After reset, writer should be empty")
    }

    @Test
    fun `reader remaining tracks bytes left`() {
        val writer = BsatnWriter()
        writer.writeI32(10)
        writer.writeI32(20)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(8, reader.remaining)
        reader.readI32()
        assertEquals(4, reader.remaining)
        reader.readI32()
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `reader offset tracks position`() {
        val writer = BsatnWriter()
        writer.writeI32(10)
        writer.writeI64(20L)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0, reader.offset)
        reader.readI32()
        assertEquals(4, reader.offset)
        reader.readI64()
        assertEquals(12, reader.offset)
    }

    // --- SumTag and ArrayLen ---

    @Test
    fun `sumTag roundtrip`() {
        for (tag in listOf(0u.toUByte(), 1u.toUByte(), 255u.toUByte())) {
            val writer = BsatnWriter()
            writer.writeSumTag(tag)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(tag, reader.readSumTag())
        }
    }

    @Test
    fun `arrayLen roundtrip`() {
        for (len in listOf(0, 1, 100, 65535)) {
            val writer = BsatnWriter()
            writer.writeArrayLen(len)
            val reader = BsatnReader(writer.toByteArray())
            assertEquals(len, reader.readArrayLen())
        }
    }

    // --- Little-endian byte order verification ---

    @Test
    fun `i32 is little-endian`() {
        val writer = BsatnWriter()
        writer.writeI32(1)
        val bytes = writer.toByteArray()
        assertEquals(4, bytes.size)
        // 1 in little-endian i32 = [0x01, 0x00, 0x00, 0x00]
        assertEquals(0x01.toByte(), bytes[0])
        assertEquals(0x00.toByte(), bytes[1])
        assertEquals(0x00.toByte(), bytes[2])
        assertEquals(0x00.toByte(), bytes[3])
    }

    @Test
    fun `u16 is little-endian`() {
        val writer = BsatnWriter()
        writer.writeU16(0x0102u.toUShort())
        val bytes = writer.toByteArray()
        assertEquals(2, bytes.size)
        // 0x0102 in little-endian = [0x02, 0x01]
        assertEquals(0x02.toByte(), bytes[0])
        assertEquals(0x01.toByte(), bytes[1])
    }

    @Test
    fun `f64 is little-endian IEEE 754`() {
        val writer = BsatnWriter()
        writer.writeF64(1.0)
        val bytes = writer.toByteArray()
        assertEquals(8, bytes.size)
        // 1.0 as IEEE 754 double LE = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F]
        assertEquals(0x00.toByte(), bytes[0])
        assertEquals(0x3F.toByte(), bytes[7])
        assertEquals(0xF0.toByte(), bytes[6])
    }
}
