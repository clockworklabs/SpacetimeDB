package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

class BigIntegerTest {

    // ---- Construction from Long ----

    @Test
    fun `construct from zero`() {
        assertEquals("0", BigInteger(0L).toString())
        assertEquals(0, BigInteger(0L).signum())
    }

    @Test
    fun `construct from positive long`() {
        assertEquals("42", BigInteger(42L).toString())
        assertEquals("9223372036854775807", BigInteger(Long.MAX_VALUE).toString())
    }

    @Test
    fun `construct from negative long`() {
        assertEquals("-1", BigInteger(-1L).toString())
        assertEquals("-42", BigInteger(-42L).toString())
        assertEquals("-9223372036854775808", BigInteger(Long.MIN_VALUE).toString())
    }

    @Test
    fun `construct from int`() {
        assertEquals("42", BigInteger(42).toString())
        assertEquals("-1", BigInteger(-1).toString())
    }

    // ---- Constants ----

    @Test
    fun constants() {
        assertEquals("0", BigInteger.ZERO.toString())
        assertEquals("1", BigInteger.ONE.toString())
        assertEquals("2", BigInteger.TWO.toString())
        assertEquals("10", BigInteger.TEN.toString())
    }

    // ---- fromULong ----

    @Test
    fun `from u long zero`() {
        assertEquals(BigInteger.ZERO, BigInteger.fromULong(0UL))
    }

    @Test
    fun `from u long small`() {
        assertEquals(BigInteger(42L), BigInteger.fromULong(42UL))
    }

    @Test
    fun `from u long max`() {
        // ULong.MAX_VALUE = 2^64 - 1 = 18446744073709551615
        val v = BigInteger.fromULong(ULong.MAX_VALUE)
        assertEquals("18446744073709551615", v.toString())
        assertEquals(1, v.signum())
    }

    @Test
    fun `from u long high bit set`() {
        // 2^63 = 9223372036854775808 (high bit of Long set, but unsigned)
        val v = BigInteger.fromULong(9223372036854775808UL)
        assertEquals("9223372036854775808", v.toString())
        assertEquals(1, v.signum())
    }

    // ---- parseString decimal ----

    @Test
    fun `parse decimal zero`() {
        assertEquals(BigInteger.ZERO, BigInteger.parseString("0"))
    }

    @Test
    fun `parse decimal positive`() {
        assertEquals(BigInteger(42L), BigInteger.parseString("42"))
    }

    @Test
    fun `parse decimal negative`() {
        assertEquals(BigInteger(-42L), BigInteger.parseString("-42"))
    }

    @Test
    fun `parse decimal large positive`() {
        // 2^127 - 1 = I128 max
        val s = "170141183460469231731687303715884105727"
        val v = BigInteger.parseString(s)
        assertEquals(s, v.toString())
    }

    @Test
    fun `parse decimal large negative`() {
        // -2^127 = I128 min
        val s = "-170141183460469231731687303715884105728"
        val v = BigInteger.parseString(s)
        assertEquals(s, v.toString())
    }

    @Test
    fun `parse decimal u256 max`() {
        // 2^256 - 1
        val s = "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        val v = BigInteger.parseString(s)
        assertEquals(s, v.toString())
    }

    // ---- parseString hex ----

    @Test
    fun `parse hex zero`() {
        assertEquals(BigInteger.ZERO, BigInteger.parseString("0", 16))
    }

    @Test
    fun `parse hex small`() {
        assertEquals(BigInteger(255L), BigInteger.parseString("ff", 16))
        assertEquals(BigInteger(256L), BigInteger.parseString("100", 16))
    }

    @Test
    fun `parse hex upper case`() {
        assertEquals(BigInteger(255L), BigInteger.parseString("FF", 16))
    }

    @Test
    fun `parse hex negative`() {
        assertEquals(BigInteger(-255L), BigInteger.parseString("-ff", 16))
    }

    @Test
    fun `parse hex large`() {
        // 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF = U128 max
        val v = BigInteger.parseString("ffffffffffffffffffffffffffffffff", 16)
        assertEquals("340282366920938463463374607431768211455", v.toString())
    }

    // ---- toString hex ----

    @Test
    fun `to string hex zero`() {
        assertEquals("0", BigInteger.ZERO.toString(16))
    }

    @Test
    fun `to string hex positive`() {
        assertEquals("ff", BigInteger(255L).toString(16))
        assertEquals("100", BigInteger(256L).toString(16))
        assertEquals("1", BigInteger(1L).toString(16))
    }

    @Test
    fun `to string hex negative`() {
        assertEquals("-1", BigInteger(-1L).toString(16))
        assertEquals("-ff", BigInteger(-255L).toString(16))
    }

    @Test
    fun `hex round trip`() {
        val original = "deadbeef01234567890abcdef"
        val v = BigInteger.parseString(original, 16)
        assertEquals(original, v.toString(16))
    }

    // ---- Arithmetic: shl ----

    @Test
    fun `shl zero`() {
        assertEquals(BigInteger(1L), BigInteger(1L).shl(0))
    }

    @Test
    fun `shl by one`() {
        assertEquals(BigInteger(2L), BigInteger(1L).shl(1))
        assertEquals(BigInteger(254L), BigInteger(127L).shl(1))
    }

    @Test
    fun `shl by eight`() {
        assertEquals(BigInteger(256L), BigInteger(1L).shl(8))
    }

    @Test
    fun `shl large`() {
        // 1 << 127 = 2^127
        val v = BigInteger.ONE.shl(127)
        assertEquals("170141183460469231731687303715884105728", v.toString())
    }

    @Test
    fun `shl negative`() {
        // -1 << 8 = -256
        assertEquals(BigInteger(-256L), BigInteger(-1L).shl(8))
        // -1 << 1 = -2
        assertEquals(BigInteger(-2L), BigInteger(-1L).shl(1))
    }

    @Test
    fun `shl zero value`() {
        assertEquals(BigInteger.ZERO, BigInteger.ZERO.shl(100))
    }

    // ---- Arithmetic: add ----

    @Test
    fun `add positive`() {
        assertEquals(BigInteger(3L), BigInteger(1L).add(BigInteger(2L)))
    }

    @Test
    fun `add negative`() {
        assertEquals(BigInteger(-3L), BigInteger(-1L).add(BigInteger(-2L)))
    }

    @Test
    fun `add mixed`() {
        assertEquals(BigInteger.ZERO, BigInteger(1L).add(BigInteger(-1L)))
    }

    @Test
    fun `add large`() {
        // (2^127 - 1) + 1 = 2^127
        val max = BigInteger.ONE.shl(127) - BigInteger.ONE
        val result = max + BigInteger.ONE
        assertEquals(BigInteger.ONE.shl(127), result)
    }

    // ---- Arithmetic: subtract ----

    @Test
    fun `subtract positive`() {
        assertEquals(BigInteger(-1L), BigInteger(1L) - BigInteger(2L))
    }

    @Test
    fun `subtract same`() {
        assertEquals(BigInteger.ZERO, BigInteger(42L) - BigInteger(42L))
    }

    // ---- Arithmetic: negate ----

    @Test
    fun `negate positive`() {
        assertEquals(BigInteger(-42L), -BigInteger(42L))
    }

    @Test
    fun `negate negative`() {
        assertEquals(BigInteger(42L), -BigInteger(-42L))
    }

    @Test
    fun `negate zero`() {
        assertEquals(BigInteger.ZERO, -BigInteger.ZERO)
    }

    @Test
    fun `negate long min`() {
        // -(Long.MIN_VALUE) = Long.MAX_VALUE + 1 = 9223372036854775808
        val v = -BigInteger(Long.MIN_VALUE)
        assertEquals("9223372036854775808", v.toString())
        assertEquals(1, v.signum())
    }

    // ---- signum ----

    @Test
    fun `signum values`() {
        assertEquals(0, BigInteger.ZERO.signum())
        assertEquals(1, BigInteger.ONE.signum())
        assertEquals(-1, BigInteger(-1L).signum())
    }

    // ---- compareTo ----

    @Test
    fun `compare to same value`() {
        assertEquals(0, BigInteger(42L).compareTo(BigInteger(42L)))
    }

    @Test
    fun `compare to positive`() {
        assertTrue(BigInteger(1L) < BigInteger(2L))
        assertTrue(BigInteger(2L) > BigInteger(1L))
    }

    @Test
    fun `compare to negative`() {
        assertTrue(BigInteger(-2L) < BigInteger(-1L))
    }

    @Test
    fun `compare to cross sign`() {
        assertTrue(BigInteger(-1L) < BigInteger(1L))
        assertTrue(BigInteger(1L) > BigInteger(-1L))
        assertTrue(BigInteger(-1L) < BigInteger.ZERO)
        assertTrue(BigInteger.ZERO < BigInteger.ONE)
    }

    @Test
    fun `compare to large values`() {
        val a = BigInteger.ONE.shl(127)
        val b = BigInteger.ONE.shl(127) - BigInteger.ONE
        assertTrue(a > b)
        assertTrue(b < a)
    }

    // ---- equals and hashCode ----

    @Test
    fun `equals identical`() {
        assertEquals(BigInteger(42L), BigInteger(42L))
    }

    @Test
    fun `equals from different paths`() {
        // Same value constructed differently should be equal
        val a = BigInteger.parseString("255")
        val b = BigInteger.parseString("ff", 16)
        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
    }

    @Test
    fun `not equals different values`() {
        assertNotEquals(BigInteger(1L), BigInteger(2L))
    }

    // ---- toByteArray (BE two's complement) ----

    @Test
    fun `to byte array zero`() {
        val bytes = BigInteger.ZERO.toByteArray()
        assertEquals(1, bytes.size)
        assertEquals(0.toByte(), bytes[0])
    }

    @Test
    fun `to byte array positive`() {
        val bytes = BigInteger(1L).toByteArray()
        assertEquals(1, bytes.size)
        assertEquals(1.toByte(), bytes[0])
    }

    @Test
    fun `to byte array negative`() {
        // -1 in BE two's complement = [0xFF]
        val bytes = BigInteger(-1L).toByteArray()
        assertEquals(1, bytes.size)
        assertEquals(0xFF.toByte(), bytes[0])
    }

    @Test
    fun `to byte array128`() {
        // 128 needs 2 bytes in BE: [0x00, 0x80]
        val bytes = BigInteger(128L).toByteArray()
        assertEquals(2, bytes.size)
        assertEquals(0x00.toByte(), bytes[0])
        assertEquals(0x80.toByte(), bytes[1])
    }

    // ---- fromLeBytes / toLeBytesFixedWidth round-trip ----

    @Test
    fun `le bytes round trip16`() {
        val values = listOf(BigInteger.ZERO, BigInteger.ONE, BigInteger(-1L),
            BigInteger.ONE.shl(127) - BigInteger.ONE, // I128 max
            -BigInteger.ONE.shl(127))                  // I128 min
        for (v in values) {
            val le = v.toLeBytesFixedWidth(16)
            assertEquals(16, le.size)
            val restored = BigInteger.fromLeBytes(le, 0, 16)
            assertEquals(v, restored, "LE round-trip failed for $v")
        }
    }

    @Test
    fun `le bytes round trip32`() {
        val values = listOf(BigInteger.ZERO, BigInteger.ONE, BigInteger(-1L),
            BigInteger.ONE.shl(255) - BigInteger.ONE, // I256 max
            -BigInteger.ONE.shl(255))                  // I256 min
        for (v in values) {
            val le = v.toLeBytesFixedWidth(32)
            assertEquals(32, le.size)
            val restored = BigInteger.fromLeBytes(le, 0, 32)
            assertEquals(v, restored, "LE round-trip failed for $v")
        }
    }

    @Test
    fun `from le bytes unsigned max u128`() {
        // All 0xFF bytes = U128 max
        val le = ByteArray(16) { 0xFF.toByte() }
        val v = BigInteger.fromLeBytesUnsigned(le, 0, 16)
        assertEquals(1, v.signum())
        assertEquals("340282366920938463463374607431768211455", v.toString())
    }

    // ---- fromByteArray with Sign ----

    @Test
    fun `from byte array positive`() {
        // BE magnitude [0xFF] with POSITIVE sign = 255
        val v = BigInteger.fromByteArray(byteArrayOf(0xFF.toByte()), Sign.POSITIVE)
        assertEquals(BigInteger(255L), v)
    }

    @Test
    fun `from byte array negative`() {
        val v = BigInteger.fromByteArray(byteArrayOf(0x01), Sign.NEGATIVE)
        assertEquals(BigInteger(-1L), v)
    }

    @Test
    fun `from byte array zero`() {
        assertEquals(BigInteger.ZERO, BigInteger.fromByteArray(byteArrayOf(0), Sign.ZERO))
    }

    // ---- fitsInSignedBytes / fitsInUnsignedBytes ----

    @Test
    fun `fits in signed bytes i128`() {
        val max = BigInteger.ONE.shl(127) - BigInteger.ONE
        val min = -BigInteger.ONE.shl(127)
        assertTrue(max.fitsInSignedBytes(16))
        assertTrue(min.fitsInSignedBytes(16))

        val overflow = BigInteger.ONE.shl(127)
        assertTrue(!overflow.fitsInSignedBytes(16))
    }

    @Test
    fun `fits in unsigned bytes u128`() {
        val max = BigInteger.ONE.shl(128) - BigInteger.ONE
        assertTrue(max.fitsInUnsignedBytes(16))

        val overflow = BigInteger.ONE.shl(128)
        assertTrue(!overflow.fitsInUnsignedBytes(16))

        assertTrue(!BigInteger(-1L).fitsInUnsignedBytes(16))
    }

    // ---- Chunk boundary values (128-bit) ----

    @Test
    fun `chunk boundary128`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63) - ONE,   // 2^63 - 1
            ONE.shl(63),         // 2^63
            ONE.shl(64) - ONE,   // 2^64 - 1
            ONE.shl(64),         // 2^64
            ONE.shl(64) + ONE,   // 2^64 + 1
        )
        for (v in values) {
            val le = v.toLeBytesFixedWidth(16)
            val restored = BigInteger.fromLeBytesUnsigned(le, 0, 16)
            assertEquals(v, restored, "Chunk boundary failed for $v")
        }
    }

    // ---- Chunk boundary values (256-bit) ----

    @Test
    fun `chunk boundary256`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            ONE.shl(63),
            ONE.shl(64),
            ONE.shl(127),
            ONE.shl(128),
            ONE.shl(191),
            ONE.shl(192),
            ONE.shl(255),
        )
        for (v in values) {
            val le = v.toLeBytesFixedWidth(32)
            val restored = BigInteger.fromLeBytesUnsigned(le, 0, 32)
            assertEquals(v, restored, "256-bit chunk boundary failed for $v")
        }
    }

    // ---- Negative LE round-trips (signed) ----

    @Test
    fun `negative le bytes round trip`() {
        val ONE = BigInteger.ONE
        val values = listOf(
            BigInteger(-2),
            -ONE.shl(63),
            -ONE.shl(64),
            -ONE.shl(64) - ONE,
            -ONE.shl(127),
        )
        for (v in values) {
            val le = v.toLeBytesFixedWidth(16)
            val restored = BigInteger.fromLeBytes(le, 0, 16)
            assertEquals(v, restored, "Negative LE round-trip failed for $v")
        }
    }

    // ---- Decimal toString round-trip for large values ----

    @Test
    fun `decimal round trip large values`() {
        val values = listOf(
            "170141183460469231731687303715884105727",   // I128 max
            "-170141183460469231731687303715884105728",  // I128 min
            "340282366920938463463374607431768211455",   // U128 max
            "57896044618658097711785492504343953926634992332820282019728792003956564819967",   // I256 max
            "-57896044618658097711785492504343953926634992332820282019728792003956564819968",  // I256 min
            "115792089237316195423570985008687907853269984665640564039457584007913129639935", // U256 max
        )
        for (s in values) {
            val v = BigInteger.parseString(s)
            assertEquals(s, v.toString(), "Decimal round-trip failed for $s")
        }
    }

    // ---- writeLeBytes ----

    @Test
    fun `write le bytes directly`() {
        val v = BigInteger(0x0102030405060708L)
        val dest = ByteArray(16)
        v.writeLeBytes(dest, 0, 16)
        assertEquals(0x08.toByte(), dest[0])
        assertEquals(0x07.toByte(), dest[1])
        assertEquals(0x06.toByte(), dest[2])
        assertEquals(0x05.toByte(), dest[3])
        assertEquals(0x04.toByte(), dest[4])
        assertEquals(0x03.toByte(), dest[5])
        assertEquals(0x02.toByte(), dest[6])
        assertEquals(0x01.toByte(), dest[7])
        // Rest should be zero-padded
        for (i in 8 until 16) {
            assertEquals(0.toByte(), dest[i], "Byte at $i should be 0")
        }
    }

    @Test
    fun `write le bytes negative`() {
        val v = BigInteger(-1L)
        val dest = ByteArray(16)
        v.writeLeBytes(dest, 0, 16)
        // -1 in 16 bytes LE = all 0xFF
        for (i in 0 until 16) {
            assertEquals(0xFF.toByte(), dest[i], "Byte at $i should be 0xFF")
        }
    }
}
