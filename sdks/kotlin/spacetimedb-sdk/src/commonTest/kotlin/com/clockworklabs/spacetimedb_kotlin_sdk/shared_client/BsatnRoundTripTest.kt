package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
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
    fun `bool true`() {
        val result = roundTrip({ it.writeBool(true) }, { it.readBool() })
        assertTrue(result as Boolean)
    }

    @Test
    fun `bool false`() {
        val result = roundTrip({ it.writeBool(false) }, { it.readBool() })
        assertFalse(result as Boolean)
    }

    // ---- I8 / U8 ----

    @Test
    fun `i8 round trip`() {
        for (v in listOf(Byte.MIN_VALUE, -1, 0, 1, Byte.MAX_VALUE)) {
            val result = roundTrip({ it.writeI8(v) }, { it.readI8() })
            assertEquals(v, result)
        }
    }

    @Test
    fun `u8 round trip`() {
        for (v in listOf(0u, 1u, 127u, 255u)) {
            val result = roundTrip({ it.writeU8(v.toUByte()) }, { it.readU8() })
            assertEquals(v.toUByte(), result)
        }
    }

    // ---- I16 / U16 ----

    @Test
    fun `i16 round trip`() {
        for (v in listOf(Short.MIN_VALUE, -1, 0, 1, Short.MAX_VALUE)) {
            val result = roundTrip({ it.writeI16(v) }, { it.readI16() })
            assertEquals(v, result)
        }
    }

    @Test
    fun `u16 round trip`() {
        for (v in listOf(0u, 1u, 32767u, 65535u)) {
            val result = roundTrip({ it.writeU16(v.toUShort()) }, { it.readU16() })
            assertEquals(v.toUShort(), result)
        }
    }

    // ---- I32 / U32 ----

    @Test
    fun `i32 round trip`() {
        for (v in listOf(Int.MIN_VALUE, -1, 0, 1, Int.MAX_VALUE)) {
            val result = roundTrip({ it.writeI32(v) }, { it.readI32() })
            assertEquals(v, result)
        }
    }

    @Test
    fun `u32 round trip`() {
        for (v in listOf(0u, 1u, UInt.MAX_VALUE)) {
            val result = roundTrip({ it.writeU32(v) }, { it.readU32() })
            assertEquals(v, result)
        }
    }

    // ---- I64 / U64 ----

    @Test
    fun `i64 round trip`() {
        for (v in listOf(Long.MIN_VALUE, -1L, 0L, 1L, Long.MAX_VALUE)) {
            val result = roundTrip({ it.writeI64(v) }, { it.readI64() })
            assertEquals(v, result)
        }
    }

    @Test
    fun `u64 round trip`() {
        for (v in listOf(0uL, 1uL, ULong.MAX_VALUE)) {
            val result = roundTrip({ it.writeU64(v) }, { it.readU64() })
            assertEquals(v, result)
        }
    }

    // ---- F32 / F64 ----

    @Test
    fun `f32 round trip`() {
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
    fun `f64 round trip`() {
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
    fun `i128 round trip`() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            BigInteger(-1),
            BigInteger.parseString("170141183460469231731687303715884105727"), // I128 max (2^127 - 1)
            BigInteger.parseString("-170141183460469231731687303715884105728"), // I128 min (-2^127)
        )
        for (v in values) {
            val result = roundTrip({ it.writeI128(v) }, { it.readI128() })
            assertEquals(v, result, "I128 round-trip failed for $v")
        }
    }

    @Test
    fun `i128 negative edge cases`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            BigInteger(-2),                                 // 0xFF...FE — near -1
            -ONE.shl(63),                                   // -2^63: p0=Long.MIN_VALUE as unsigned, p1=-1
            -ONE.shl(63) + ONE,                             // -2^63 + 1: p0 high bit set
            -ONE.shl(63) - ONE,                             // -2^63 - 1: borrow from p1 into p0
            -ONE.shl(64),                                   // -2^64: p0=0, p1=-1 — exact chunk boundary
            -ONE.shl(64) + ONE,                             // -2^64 + 1: p0 = ULong.MAX_VALUE, p1 = -2
            -ONE.shl(64) - ONE,                             // -2^64 - 1: just past chunk boundary
            BigInteger.parseString("-9223372036854775808"),  // -2^63 as decimal
            BigInteger.parseString("-18446744073709551616"), // -2^64 as decimal
        )
        for (v in values) {
            val result = roundTrip({ it.writeI128(v) }, { it.readI128() })
            assertEquals(v, result, "I128 negative edge case failed for $v")
        }
    }

    @Test
    fun `i128 chunk boundary values`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63) - ONE,   // 2^63 - 1 = Long.MAX_VALUE in p0
            ONE.shl(63),         // 2^63: p0 bit 63 set (unsigned), p1=0
            ONE.shl(64) - ONE,   // 2^64 - 1: p0 = all ones (unsigned), p1 = 0
            ONE.shl(64),         // 2^64: p0 = 0, p1 = 1
            ONE.shl(64) + ONE,   // 2^64 + 1: p0 = 1, p1 = 1
        )
        for (v in values) {
            val result = roundTrip({ it.writeI128(v) }, { it.readI128() })
            assertEquals(v, result, "I128 chunk boundary failed for $v")
        }
    }

    @Test
    fun `u128 round trip`() {
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

    @Test
    fun `u128 chunk boundary values`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63) - ONE,   // 2^63 - 1: p0 just below Long sign bit
            ONE.shl(63),         // 2^63: p0 has high bit set (read as negative Long)
            ONE.shl(64) - ONE,   // 2^64 - 1: p0 all ones, p1 = 0
            ONE.shl(64),         // 2^64: p0 = 0, p1 = 1
            ONE.shl(127),        // 2^127: p1 high bit set (read as negative Long)
        )
        for (v in values) {
            val result = roundTrip({ it.writeU128(v) }, { it.readU128() })
            assertEquals(v, result, "U128 chunk boundary failed for $v")
        }
    }

    // ---- I256 / U256 ----

    @Test
    fun `i256 round trip`() {
        val values = listOf(
            BigInteger.ZERO,
            BigInteger.ONE,
            BigInteger(-1),
            // I256 max: 2^255 - 1
            BigInteger.parseString("57896044618658097711785492504343953926634992332820282019728792003956564819967"),
            // I256 min: -2^255
            BigInteger.parseString("-57896044618658097711785492504343953926634992332820282019728792003956564819968"),
        )
        for (v in values) {
            val result = roundTrip({ it.writeI256(v) }, { it.readI256() })
            assertEquals(v, result, "I256 round-trip failed for $v")
        }
    }

    @Test
    fun `i256 negative edge cases`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            BigInteger(-2),                                 // near -1
            -ONE.shl(63),                                   // -2^63: chunk 0 boundary
            -ONE.shl(64),                                   // -2^64: exact chunk 0/1 boundary
            -ONE.shl(64) - ONE,                             // -2^64 - 1: just past first chunk boundary
            -ONE.shl(127),                                  // -2^127: chunk 1/2 boundary
            -ONE.shl(128),                                  // -2^128: exact chunk 2 boundary
            -ONE.shl(128) + ONE,                            // -2^128 + 1
            -ONE.shl(191),                                  // -2^191: chunk 2/3 boundary
            -ONE.shl(192),                                  // -2^192: exact chunk 3 boundary
            -ONE.shl(192) - ONE,                            // -2^192 - 1
            // Large negative with mixed chunk values
            BigInteger.parseString("-1000000000000000000000000000000000000000"),
        )
        for (v in values) {
            val result = roundTrip({ it.writeI256(v) }, { it.readI256() })
            assertEquals(v, result, "I256 negative edge case failed for $v")
        }
    }

    @Test
    fun `i256 chunk boundary values`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63),    // chunk 0 high bit
            ONE.shl(64),    // chunk 0→1 boundary
            ONE.shl(127),   // chunk 1 high bit
            ONE.shl(128),   // chunk 1→2 boundary
            ONE.shl(191),   // chunk 2 high bit
            ONE.shl(192),   // chunk 2→3 boundary
        )
        for (v in values) {
            val result = roundTrip({ it.writeI256(v) }, { it.readI256() })
            assertEquals(v, result, "I256 chunk boundary failed for $v")
        }
    }

    @Test
    fun `u256 round trip`() {
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

    @Test
    fun `u256 chunk boundary values`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63),    // chunk 0 high bit (read as negative Long)
            ONE.shl(64),    // chunk 0→1 boundary
            ONE.shl(127),   // chunk 1 high bit
            ONE.shl(128),   // chunk 1→2 boundary
            ONE.shl(191),   // chunk 2 high bit
            ONE.shl(192),   // chunk 2→3 boundary
            ONE.shl(255),   // chunk 3 high bit (read as negative Long)
        )
        for (v in values) {
            val result = roundTrip({ it.writeU256(v) }, { it.readU256() })
            assertEquals(v, result, "U256 chunk boundary failed for $v")
        }
    }

    // ---- Overflow detection ----

    @Test
    fun `i128 overflow rejects`() {
        val ONE = BigInteger.ONE
        val tooLarge = ONE.shl(127)            // 2^127 = I128 max + 1
        val tooSmall = -ONE.shl(127) - ONE     // -2^127 - 1
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeI128(tooLarge)
        }
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeI128(tooSmall)
        }
    }

    @Test
    fun `u128 overflow rejects`() {
        val tooLarge = BigInteger.ONE.shl(128)  // 2^128 = U128 max + 1
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeU128(tooLarge)
        }
    }

    @Test
    fun `u128 negative rejects`() {
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeU128(BigInteger(-1))
        }
    }

    @Test
    fun `i256 overflow rejects`() {
        val ONE = BigInteger.ONE
        val tooLarge = ONE.shl(255)            // 2^255 = I256 max + 1
        val tooSmall = -ONE.shl(255) - ONE     // -2^255 - 1
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeI256(tooLarge)
        }
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeI256(tooSmall)
        }
    }

    @Test
    fun `u256 overflow rejects`() {
        val tooLarge = BigInteger.ONE.shl(256)  // 2^256 = U256 max + 1
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeU256(tooLarge)
        }
    }

    @Test
    fun `u256 negative rejects`() {
        assertFailsWith<IllegalArgumentException> {
            val writer = BsatnWriter()
            writer.writeU256(BigInteger(-1))
        }
    }

    // ---- String ----

    @Test
    fun `string empty`() {
        val result = roundTrip({ it.writeString("") }, { it.readString() })
        assertEquals("", result)
    }

    @Test
    fun `string ascii`() {
        val result = roundTrip({ it.writeString("hello world") }, { it.readString() })
        assertEquals("hello world", result)
    }

    @Test
    fun `string multi byte utf8`() {
        val s = "\u00E9\u00F1\u00FC\u2603\uD83D\uDE00" // e-acute, n-tilde, u-umlaut, snowman, emoji
        val result = roundTrip({ it.writeString(s) }, { it.readString() })
        assertEquals(s, result)
    }

    // ---- ByteArray ----

    @Test
    fun `byte array empty`() {
        val result = roundTrip({ it.writeByteArray(byteArrayOf()) }, { it.readByteArray() })
        assertTrue((result as ByteArray).isEmpty())
    }

    @Test
    fun `byte array non empty`() {
        val input = byteArrayOf(0, 1, 127, -128, -1)
        val result = roundTrip({ it.writeByteArray(input) }, { it.readByteArray() })
        assertTrue(input.contentEquals(result as ByteArray))
    }

    // ---- ArrayLen ----

    @Test
    fun `array len round trip`() {
        for (v in listOf(0, 1, 1000, Int.MAX_VALUE)) {
            val result = roundTrip({ it.writeArrayLen(v) }, { it.readArrayLen() })
            assertEquals(v, result)
        }
    }

    @Test
    fun `array len rejects negative`() {
        val writer = BsatnWriter()
        assertFailsWith<IllegalArgumentException> {
            writer.writeArrayLen(-1)
        }
    }

    // ---- Overflow checks ----

    @Test
    fun `read string overflow rejects`() {
        // Encode a length that exceeds Int.MAX_VALUE (use UInt.MAX_VALUE = 4294967295)
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE) // length prefix > Int.MAX_VALUE
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readString()
        }
    }

    @Test
    fun `read byte array overflow rejects`() {
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readByteArray()
        }
    }

    @Test
    fun `read array len overflow rejects`() {
        val writer = BsatnWriter()
        writer.writeU32(UInt.MAX_VALUE)
        val reader = BsatnReader(writer.toByteArray())
        assertFailsWith<IllegalStateException> {
            reader.readArrayLen()
        }
    }

    // ---- Reader underflow ----

    @Test
    fun `reader underflow throws`() {
        val reader = BsatnReader(byteArrayOf())
        assertFailsWith<IllegalStateException> {
            reader.readByte()
        }
    }

    @Test
    fun `reader remaining tracks correctly`() {
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
    fun `writer reset clears state`() {
        val writer = BsatnWriter()
        writer.writeI32(42)
        assertEquals(4, writer.offset)
        writer.reset()
        assertEquals(0, writer.offset)
    }
}
