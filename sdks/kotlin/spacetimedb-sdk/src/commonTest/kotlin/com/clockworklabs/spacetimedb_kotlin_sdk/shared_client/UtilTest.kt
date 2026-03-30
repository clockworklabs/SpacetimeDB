package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.time.Instant

class UtilTest {
    // ---- BigInteger hex round-trip ----

    @Test
    fun `hex round trip 16 bytes`() {
        val value = BigInteger.parseString("12345678901234567890abcdef", 16)
        val hex = value.toHexString(16) // 16 bytes = 32 hex chars
        assertEquals(32, hex.length)
        val restored = parseHexString(hex)
        assertEquals(value, restored)
    }

    @Test
    fun `hex round trip 32 bytes`() {
        val value = BigInteger.parseString("abcdef0123456789abcdef0123456789", 16)
        val hex = value.toHexString(32) // 32 bytes = 64 hex chars
        assertEquals(64, hex.length)
        val restored = parseHexString(hex)
        assertEquals(value, restored)
    }

    @Test
    fun `hex zero value`() {
        val zero = BigInteger.ZERO
        val hex16 = zero.toHexString(16)
        assertEquals("00000000000000000000000000000000", hex16)
        assertEquals(BigInteger.ZERO, parseHexString(hex16))

        val hex32 = zero.toHexString(32)
        assertEquals("0000000000000000000000000000000000000000000000000000000000000000", hex32)
        assertEquals(BigInteger.ZERO, parseHexString(hex32))
    }

    // ---- Instant microsecond round-trip ----

    @Test
    fun `instant microsecond round trip`() {
        val micros = 1_700_000_000_123_456L
        val instant = Instant.fromEpochMicroseconds(micros)
        val roundTripped = instant.toEpochMicroseconds()
        assertEquals(micros, roundTripped)
    }

    @Test
    fun `instant microsecond zero`() {
        val instant = Instant.fromEpochMicroseconds(0L)
        assertEquals(0L, instant.toEpochMicroseconds())
    }

    @Test
    fun `instant microsecond negative`() {
        val micros = -1_000_000L // 1 second before epoch
        val instant = Instant.fromEpochMicroseconds(micros)
        assertEquals(micros, instant.toEpochMicroseconds())
    }

    @Test
    fun `instant microsecond max round trips`() {
        val micros = Long.MAX_VALUE
        val instant = Instant.fromEpochMicroseconds(micros)
        assertEquals(micros, instant.toEpochMicroseconds())
    }

    @Test
    fun `instant microsecond min round trips`() {
        // Long.MIN_VALUE doesn't land on an exact second boundary, so
        // floorDiv pushes it one second beyond the representable range.
        // Use the actual minimum that round-trips cleanly.
        val minSeconds = Long.MIN_VALUE / 1_000_000L
        val micros = minSeconds * 1_000_000L
        val instant = Instant.fromEpochMicroseconds(micros)
        assertEquals(micros, instant.toEpochMicroseconds())
    }

    @Test
    fun `instant beyond microsecond range throws`() {
        // An Instant far beyond the I64 microsecond wire format range
        val farFuture = Instant.fromEpochSeconds(Long.MAX_VALUE / 1_000_000L + 1)
        assertFailsWith<IllegalArgumentException> {
            farFuture.toEpochMicroseconds()
        }
    }
}
