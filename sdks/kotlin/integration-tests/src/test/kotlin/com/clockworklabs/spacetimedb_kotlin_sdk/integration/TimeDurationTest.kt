package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class TimeDurationTest {

    // --- Factory ---

    @Test
    fun `fromMillis creates correct duration`() {
        val d = TimeDuration.fromMillis(1500L)
        assertEquals(1500L, d.millis)
        assertEquals(1_500_000L, d.micros)
    }

    @Test
    fun `fromMillis zero`() {
        val d = TimeDuration.fromMillis(0L)
        assertEquals(0L, d.millis)
        assertEquals(0L, d.micros)
    }

    @Test
    fun `constructor from Duration`() {
        val d = TimeDuration(3.seconds)
        assertEquals(3000L, d.millis)
        assertEquals(3_000_000L, d.micros)
    }

    @Test
    fun `constructor from microseconds Duration`() {
        val d = TimeDuration(500.microseconds)
        assertEquals(500L, d.micros)
        assertEquals(0L, d.millis) // 500us < 1ms
    }

    // --- Accessors ---

    @Test
    fun `micros and millis are consistent`() {
        val d = TimeDuration.fromMillis(2345L)
        assertEquals(d.micros, d.millis * 1000)
    }

    // --- Arithmetic ---

    @Test
    fun `plus adds durations`() {
        val a = TimeDuration.fromMillis(100L)
        val b = TimeDuration.fromMillis(200L)
        val result = a + b
        assertEquals(300L, result.millis)
    }

    @Test
    fun `minus subtracts durations`() {
        val a = TimeDuration.fromMillis(500L)
        val b = TimeDuration.fromMillis(200L)
        val result = a - b
        assertEquals(300L, result.millis)
    }

    @Test
    fun `minus can produce negative duration`() {
        val a = TimeDuration.fromMillis(100L)
        val b = TimeDuration.fromMillis(500L)
        val result = a - b
        assertTrue(result.micros < 0, "100 - 500 should be negative")
        assertEquals(-400L, result.millis)
    }

    @Test
    fun `plus and minus are inverse`() {
        val a = TimeDuration.fromMillis(1000L)
        val b = TimeDuration.fromMillis(300L)
        assertEquals(a, (a + b) - b)
    }

    @Test
    fun `plus zero is identity`() {
        val a = TimeDuration.fromMillis(42L)
        assertEquals(a, a + TimeDuration.fromMillis(0L))
    }

    // --- Comparison ---

    @Test
    fun `compareTo orders by duration`() {
        val short = TimeDuration.fromMillis(100L)
        val long = TimeDuration.fromMillis(200L)
        assertTrue(short < long)
        assertTrue(long > short)
    }

    @Test
    fun `compareTo equal durations`() {
        val a = TimeDuration.fromMillis(500L)
        val b = TimeDuration.fromMillis(500L)
        assertEquals(0, a.compareTo(b))
    }

    @Test
    fun `compareTo negative vs positive`() {
        val neg = TimeDuration((-100).milliseconds)
        val pos = TimeDuration(100.milliseconds)
        assertTrue(neg < pos)
    }

    // --- Formatting ---

    @Test
    fun `toString positive duration`() {
        val d = TimeDuration.fromMillis(1500L)
        val str = d.toString()
        assertTrue(str.startsWith("+"), "Positive duration should start with +: $str")
        assertTrue(str.contains("1."), "Should show 1 second: $str")
    }

    @Test
    fun `toString negative duration`() {
        val d = TimeDuration((-1500).milliseconds)
        val str = d.toString()
        assertTrue(str.startsWith("-"), "Negative duration should start with -: $str")
    }

    @Test
    fun `toString zero`() {
        val d = TimeDuration.fromMillis(0L)
        val str = d.toString()
        assertTrue(str.contains("0.000000"), "Zero should be +0.000000: $str")
    }

    @Test
    fun `toString has 6 digit microsecond precision`() {
        val d = TimeDuration.fromMillis(1234L)
        val str = d.toString()
        // format: +1.234000
        val frac = str.substringAfter(".")
        assertEquals(6, frac.length, "Fraction should be 6 digits: $str")
    }

    // --- equals / hashCode ---

    @Test
    fun `equal durations from different constructors`() {
        val a = TimeDuration.fromMillis(1000L)
        val b = TimeDuration(1.seconds)
        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
    }
}
