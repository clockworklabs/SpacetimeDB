package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class TimestampTest {

    // --- Factories ---

    @Test
    fun `UNIX_EPOCH is at epoch zero`() {
        assertEquals(0L, Timestamp.UNIX_EPOCH.microsSinceUnixEpoch)
        assertEquals(0L, Timestamp.UNIX_EPOCH.millisSinceUnixEpoch)
    }

    @Test
    fun `now returns a timestamp after epoch`() {
        val now = Timestamp.now()
        assertTrue(now.microsSinceUnixEpoch > 0, "now() should be after epoch")
    }

    @Test
    fun `now returns increasing timestamps`() {
        val a = Timestamp.now()
        val b = Timestamp.now()
        assertTrue(b >= a, "Second now() should be >= first")
    }

    @Test
    fun `fromMillis creates correct timestamp`() {
        val ts = Timestamp.fromMillis(1000L)
        assertEquals(1000L, ts.millisSinceUnixEpoch)
        assertEquals(1_000_000L, ts.microsSinceUnixEpoch)
    }

    @Test
    fun `fromMillis zero is epoch`() {
        assertEquals(Timestamp.UNIX_EPOCH, Timestamp.fromMillis(0L))
    }

    @Test
    fun `fromEpochMicroseconds creates correct timestamp`() {
        val ts = Timestamp.fromEpochMicroseconds(1_500_000L)
        assertEquals(1_500_000L, ts.microsSinceUnixEpoch)
        assertEquals(1500L, ts.millisSinceUnixEpoch)
    }

    @Test
    fun `fromEpochMicroseconds zero is epoch`() {
        assertEquals(Timestamp.UNIX_EPOCH, Timestamp.fromEpochMicroseconds(0L))
    }

    // --- Accessors ---

    @Test
    fun `microsSinceUnixEpoch and millisSinceUnixEpoch are consistent`() {
        val ts = Timestamp.fromMillis(12345L)
        assertEquals(ts.microsSinceUnixEpoch, ts.millisSinceUnixEpoch * 1000)
    }

    // --- Arithmetic ---

    @Test
    fun `plus TimeDuration adds time`() {
        val ts = Timestamp.fromMillis(1000L)
        val dur = TimeDuration.fromMillis(500L)
        val result = ts + dur
        assertEquals(1500L, result.millisSinceUnixEpoch)
    }

    @Test
    fun `minus TimeDuration subtracts time`() {
        val ts = Timestamp.fromMillis(1000L)
        val dur = TimeDuration.fromMillis(300L)
        val result = ts - dur
        assertEquals(700L, result.millisSinceUnixEpoch)
    }

    @Test
    fun `minus Timestamp returns TimeDuration`() {
        val a = Timestamp.fromMillis(1000L)
        val b = Timestamp.fromMillis(400L)
        val diff = a - b
        assertEquals(600L, diff.millis)
    }

    @Test
    fun `minus Timestamp can be negative`() {
        val a = Timestamp.fromMillis(100L)
        val b = Timestamp.fromMillis(500L)
        val diff = a - b
        assertTrue(diff.micros < 0, "Earlier - later should be negative: ${diff.micros}")
    }

    @Test
    fun `since returns duration between timestamps`() {
        val a = Timestamp.fromMillis(1000L)
        val b = Timestamp.fromMillis(300L)
        val dur = a.since(b)
        assertEquals(700L, dur.millis)
    }

    @Test
    fun `plus and minus are inverse operations`() {
        val ts = Timestamp.fromMillis(5000L)
        val dur = TimeDuration.fromMillis(1234L)
        assertEquals(ts, (ts + dur) - dur)
    }

    // --- Comparison ---

    @Test
    fun `compareTo orders by time`() {
        val early = Timestamp.fromMillis(100L)
        val late = Timestamp.fromMillis(200L)
        assertTrue(early < late)
        assertTrue(late > early)
    }

    @Test
    fun `compareTo equal timestamps`() {
        val a = Timestamp.fromMillis(100L)
        val b = Timestamp.fromMillis(100L)
        assertEquals(0, a.compareTo(b))
    }

    @Test
    fun `UNIX_EPOCH is less than now`() {
        assertTrue(Timestamp.UNIX_EPOCH < Timestamp.now())
    }

    // --- Formatting ---

    @Test
    fun `toISOString contains Z suffix`() {
        val ts = Timestamp.fromMillis(1000L)
        val iso = ts.toISOString()
        assertTrue(iso.endsWith("Z"), "ISO string should end with Z: $iso")
    }

    @Test
    fun `toISOString contains T separator`() {
        val ts = Timestamp.now()
        val iso = ts.toISOString()
        assertTrue(iso.contains("T"), "ISO string should contain T: $iso")
    }

    @Test
    fun `toISOString preserves microsecond precision`() {
        val ts = Timestamp.fromEpochMicroseconds(1_000_123_456L)
        val iso = ts.toISOString()
        // Should have 6-digit microsecond fraction
        assertTrue(iso.contains("."), "ISO string should have fractional part: $iso")
        val frac = iso.substringAfter(".").removeSuffix("Z")
        assertEquals(6, frac.length, "Fraction should be 6 digits: $frac")
    }

    @Test
    fun `toString equals toISOString`() {
        val ts = Timestamp.fromMillis(42000L)
        assertEquals(ts.toISOString(), ts.toString())
    }

    @Test
    fun `UNIX_EPOCH toISOString is 1970-01-01`() {
        val iso = Timestamp.UNIX_EPOCH.toISOString()
        assertTrue(iso.startsWith("1970-01-01"), "Epoch should be 1970-01-01: $iso")
    }

    // --- equals / hashCode ---

    @Test
    fun `equal timestamps from different factories are equal`() {
        val a = Timestamp.fromMillis(5000L)
        val b = Timestamp.fromEpochMicroseconds(5_000_000L)
        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
    }
}
