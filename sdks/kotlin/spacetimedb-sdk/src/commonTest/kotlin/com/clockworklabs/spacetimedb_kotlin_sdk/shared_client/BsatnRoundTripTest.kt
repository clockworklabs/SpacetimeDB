package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.ionspin.kotlin.bignum.integer.BigInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class BsatnRoundTripTest {
    private fun roundTrip(write: (BsatnWriter) -> Unit, read: (BsatnReader) -> Any?): Any? {
        val writer = BsatnWriter()
        write(writer)
        val reader = BsatnReader(writer.toByteArray())
        val result = read(reader)
        assertEquals(0, reader.remaining, "All bytes should be consumed")
        return result
    }

    // ---- Bool ----

    @Test
    fun boolTrue() {
        val result = roundTrip({ it.writeBool(true) }, { it.readBool() })
        assertTrue(result as Boolean)
    }

    @Test
    fun boolFalse() {
        val result = roundTrip({ it.writeBool(false) }, { it.readBool() })
        assertFalse(result as Boolean)
    }

    // ---- I8 / U8 ----

    @Test
    fun i8RoundTrip() {
        for (v in listOf<Byte>(Byte.MIN_VALUE, -1, 0, 1, Byte.MAX_VALUE)) {
            val result = roundTrip({ it.writeI8(v) }, { it.readI8() })
            assertEquals(v, result)
        }
    }

    @Test
    fun u8RoundTrip() {
        for (v in listOf(0u, 1u, 127u, 255u)) {
            val result = roundTrip({ it.writeU8(v.toUByte()) }, { it.readU8() })
            assertEquals(v.toUByte(), result)
        }
    }

    // ---- I16 / U16 ----

    @Test
    fun i16RoundTrip() {
        for (v in listOf<Short>(Short.MIN_VALUE, -1, 0, 1, Short.MAX_VALUE)) {
            val result = roundTrip({ it.writeI16(v) }, { it.readI16() })
            assertEquals(v, result)
        }
    }

    @Test
    fun u16RoundTrip() {
        for (v in listOf(0u, 1u, 32767u, 65535u)) {
            val result = roundTrip({ it.writeU16(v.toUShort()) }, { it.readU16() })
            assertEquals(v.toUShort(), result)
        }
    }

    // ---- I32 / U32 ----

    @Test
    fun i32RoundTrip() {
        for (v in listOf(Int.MIN_VALUE, -1, 0, 1, Int.MAX_VALUE)) {
            val result = roundTrip({ it.writeI32(v) }, { it.readI32() })
            assertEquals(v, result)
        }
    }

    @Test
    fun u32RoundTrip() {
        for (v in listOf(0u, 1u, UInt.MAX_VALUE)) {
            val result = roundTrip({ it.writeU32(v) }, { it.readU32() })
            assertEquals(v, result)
        }
    }

    // ---- I64 / U64 ----

    @Test
    fun i64RoundTrip() {
        for (v in listOf(Long.MIN_VALUE, -1L, 0L, 1L, Long.MAX_VALUE)) {
            val result = roundTrip({ it.writeI64(v) }, { it.readI64() })
            assertEquals(v, result)
        }
    }

    @Test
    fun u64RoundTrip() {
        for (v in listOf(0uL, 1uL, ULong.MAX_VALUE)) {
            val result = roundTrip({ it.writeU64(v) }, { it.readU64() })
            assertEquals(v, result)
        }
    }

    // ---- F32 / F64 ----

    @Test
    fun f32RoundTrip() {
        for (v in listOf(0.0f, -1.5f, Float.MAX_VALUE, Float.MIN_VALUE, Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY)) {
            val writer = BsatnWriter()
            writer.writeF32(v)
            val reader = BsatnReader(writer.toByteArray())
            val result = reader.readF32()
            if (v.isNaN()) {
                assertTrue(result.isNaN(), "Expected NaN")
            } else {
                assertEquals(v, result)
            }
        }
    }

    @Test
    fun f64RoundTrip() {
        for (v in listOf(0.0, -1.5, Double.MAX_VALUE, Double.MIN_VALUE, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY)) {
            val writer = BsatnWriter()
            writer.writeF64(v)
            val reader = BsatnReader(writer.toByteArray())
            val result = reader.readF64()
            if (v.isNaN()) {
                assertTrue(result.isNaN(), "Expected NaN")
            } else {
                assertEquals(v, result)
            }
        }
    }

    // ---- I128 / U128 ----

    @Test
    fun i128RoundTrip() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            BigInteger(-1),
            BigInteger.parseString("170141183460469231731687303715884105727"), // I128 max
            BigInteger.parseString("-170141183460469231731687303715884105728"), // I128 min
        )
        for (v in values) {
            val result = roundTrip({ it.writeI128(v) }, { it.readI128() })
            assertEquals(v, result, "I128 round-trip failed for $v")
        }
    }

    @Test
    fun u128RoundTrip() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            BigInteger.parseString("340282366920938463463374607431768211455"), // U128 max
        )
        for (v in values) {
            val result = roundTrip({ it.writeU128(v) }, { it.readU128() })
            assertEquals(v, result, "U128 round-trip failed for $v")
        }
    }

    // ---- I256 / U256 ----

    @Test
    fun i256RoundTrip() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            BigInteger(-1),
        )
        for (v in values) {
            val result = roundTrip({ it.writeI256(v) }, { it.readI256() })
            assertEquals(v, result, "I256 round-trip failed for $v")
        }
    }

    @Test
    fun u256RoundTrip() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            // U256 max: 2^256 - 1
            BigInteger.parseString("115792089237316195423570985008687907853269984665640564039457584007913129639935"),
        )
        for (v in values) {
            val result = roundTrip({ it.writeU256(v) }, { it.readU256() })
            assertEquals(v, result, "U256 round-trip failed for $v")
        }
    }

    // ---- String ----

    @Test
    fun stringEmpty() {
        val result = roundTrip({ it.writeString("") }, { it.readString() })
        assertEquals("", result)
    }

    @Test
    fun stringAscii() {
        val result = roundTrip({ it.writeString("hello world") }, { it.readString() })
        assertEquals("hello world", result)
    }

    @Test
    fun stringMultiByteUtf8() {
        val s = "\u00E9\u00F1\u00FC\u2603\uD83D\uDE00" // e-acute, n-tilde, u-umlaut, snowman, emoji
        val result = roundTrip({ it.writeString(s) }, { it.readString() })
        assertEquals(s, result)
    }

    // ---- ByteArray ----

    @Test
    fun byteArrayEmpty() {
        val result = roundTrip({ it.writeByteArray(byteArrayOf()) }, { it.readByteArray() })
        assertTrue((result as ByteArray).isEmpty())
    }

    @Test
    fun byteArrayNonEmpty() {
        val input = byteArrayOf(0, 1, 127, -128, -1)
        val result = roundTrip({ it.writeByteArray(input) }, { it.readByteArray() })
        assertTrue(input.contentEquals(result as ByteArray))
    }

    // ---- ArrayLen ----

    @Test
    fun arrayLenRoundTrip() {
        for (v in listOf(0, 1, 1000, Int.MAX_VALUE)) {
            val result = roundTrip({ it.writeArrayLen(v) }, { it.readArrayLen() })
            assertEquals(v, result)
        }
    }

    // ---- Overflow checks ----

    @Test
    fun readStringOverflowRejects() {
        // Encode a length that exceeds Int.MAX_VALUE (use UInt.MAX_VALUE = 4294967295)
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE) // length prefix > Int.MAX_VALUE
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readString()
        }
    }

    @Test
    fun readByteArrayOverflowRejects() {
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readByteArray()
        }
    }

    @Test
    fun readArrayLenOverflowRejects() {
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readArrayLen()
        }
    }

    // ---- Reader underflow ----

    @Test
    fun readerUnderflowThrows() {
        val reader = BsatnReader(byteArrayOf())
        assertFailsWith<IllegalStateException> {
            reader.readByte()
        }
    }

    @Test
    fun readerRemainingTracksCorrectly() {
        val writer = BsatnWriter()
        writer.writeI32(42)
        writer.writeI32(99)
        val reader = BsatnReader(writer.toByteArray())
        assertEquals(8, reader.remaining)
        reader.readI32()
        assertEquals(4, reader.remaining)
        reader.readI32()
        assertEquals(0, reader.remaining)
    }

    // ---- Writer reset ----

    @Test
    fun writerResetClearsState() {
        val writer = BsatnWriter()
        writer.writeI32(42)
        assertEquals(4, writer.offset)
        writer.reset()
        assertEquals(0, writer.offset)
    }
}
