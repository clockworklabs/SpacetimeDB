package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class BsatnTest {

    @Test
    fun roundTripBool() {
        val writer = BsatnWriter()
        writer.writeBool(true)
        writer.writeBool(false)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(true, reader.readBool())
        assertEquals(false, reader.readBool())
    }

    @Test
    fun roundTripU8() {
        val writer = BsatnWriter()
        writer.writeU8(0u)
        writer.writeU8(255u)
        writer.writeU8(42u)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0u.toUByte(), reader.readU8())
        assertEquals(255u.toUByte(), reader.readU8())
        assertEquals(42u.toUByte(), reader.readU8())
    }

    @Test
    fun roundTripI32() {
        val writer = BsatnWriter()
        writer.writeI32(0)
        writer.writeI32(Int.MAX_VALUE)
        writer.writeI32(Int.MIN_VALUE)
        writer.writeI32(-1)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0, reader.readI32())
        assertEquals(Int.MAX_VALUE, reader.readI32())
        assertEquals(Int.MIN_VALUE, reader.readI32())
        assertEquals(-1, reader.readI32())
    }

    @Test
    fun roundTripU32() {
        val writer = BsatnWriter()
        writer.writeU32(0u)
        writer.writeU32(UInt.MAX_VALUE)
        writer.writeU32(12345u)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0u, reader.readU32())
        assertEquals(UInt.MAX_VALUE, reader.readU32())
        assertEquals(12345u, reader.readU32())
    }

    @Test
    fun roundTripI64() {
        val writer = BsatnWriter()
        writer.writeI64(0L)
        writer.writeI64(Long.MAX_VALUE)
        writer.writeI64(Long.MIN_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0L, reader.readI64())
        assertEquals(Long.MAX_VALUE, reader.readI64())
        assertEquals(Long.MIN_VALUE, reader.readI64())
    }

    @Test
    fun roundTripU64() {
        val writer = BsatnWriter()
        writer.writeU64(0u)
        writer.writeU64(ULong.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(0u.toULong(), reader.readU64())
        assertEquals(ULong.MAX_VALUE, reader.readU64())
    }

    @Test
    fun roundTripF32() {
        val writer = BsatnWriter()
        writer.writeF32(3.14f)
        writer.writeF32(0.0f)
        writer.writeF32(-1.0f)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(3.14f, reader.readF32())
        assertEquals(0.0f, reader.readF32())
        assertEquals(-1.0f, reader.readF32())
    }

    @Test
    fun roundTripF64() {
        val writer = BsatnWriter()
        writer.writeF64(3.141592653589793)
        writer.writeF64(Double.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(3.141592653589793, reader.readF64())
        assertEquals(Double.MAX_VALUE, reader.readF64())
    }

    @Test
    fun roundTripString() {
        val writer = BsatnWriter()
        writer.writeString("hello")
        writer.writeString("")
        writer.writeString("unicode: 日本語 🚀")
        val reader = BsatnReader(writer.toByteArray())
        assertEquals("hello", reader.readString())
        assertEquals("", reader.readString())
        assertEquals("unicode: 日本語 🚀", reader.readString())
    }

    @Test
    fun roundTripByteArray() {
        val writer = BsatnWriter()
        val data = byteArrayOf(1, 2, 3, 4, 5)
        writer.writeByteArray(data)
        writer.writeByteArray(ByteArray(0))
        val reader = BsatnReader(writer.toByteArray())
        assertTrue(data.contentEquals(reader.readByteArray()))
        assertTrue(ByteArray(0).contentEquals(reader.readByteArray()))
    }

    @Test
    fun roundTripArray() {
        val writer = BsatnWriter()
        writer.writeArray(listOf(10, 20, 30)) { w, v -> w.writeI32(v) }
        val reader = BsatnReader(writer.toByteArray())
        val result = reader.readArray { it.readI32() }
        assertEquals(listOf(10, 20, 30), result)
    }

    @Test
    fun roundTripOption() {
        val writer = BsatnWriter()
        writer.writeOption(42) { w, v -> w.writeI32(v) }
        writer.writeOption<Int>(null) { w, v -> w.writeI32(v) }
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(42, reader.readOption { it.readI32() })
        assertNull(reader.readOption { it.readI32() })
    }

    @Test
    fun littleEndianEncoding() {
        val writer = BsatnWriter()
        writer.writeU32(0x04030201u)
        val bytes = writer.toByteArray()
        assertEquals(1, bytes[0].toInt())
        assertEquals(2, bytes[1].toInt())
        assertEquals(3, bytes[2].toInt())
        assertEquals(4, bytes[3].toInt())
    }

    @Test
    fun stringEncodingFormat() {
        val writer = BsatnWriter()
        writer.writeString("AB")
        val bytes = writer.toByteArray()
        assertEquals(6, bytes.size)
        assertEquals(2, bytes[0].toInt())
        assertEquals(0, bytes[1].toInt())
        assertEquals(0, bytes[2].toInt())
        assertEquals(0, bytes[3].toInt())
        assertEquals(0x41, bytes[4].toInt())
        assertEquals(0x42, bytes[5].toInt())
    }
}
