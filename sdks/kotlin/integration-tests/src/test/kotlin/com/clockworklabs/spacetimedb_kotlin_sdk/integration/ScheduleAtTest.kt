package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ScheduleAt
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class ScheduleAtTest {

    // --- interval factory ---

    @Test
    fun `interval creates Interval variant`() {
        val schedule = ScheduleAt.interval(5.seconds)
        assertTrue(schedule is ScheduleAt.Interval, "Should be Interval, got: ${schedule::class.simpleName}")
    }

    @Test
    fun `interval preserves duration`() {
        val schedule = ScheduleAt.interval(5.seconds) as ScheduleAt.Interval
        assertEquals(5000L, schedule.duration.millis)
    }

    @Test
    fun `interval with minutes`() {
        val schedule = ScheduleAt.interval(2.minutes) as ScheduleAt.Interval
        assertEquals(120_000L, schedule.duration.millis)
    }

    // --- time factory ---

    @Test
    fun `time creates Time variant`() {
        val instant = Instant.fromEpochMilliseconds(System.currentTimeMillis())
        val schedule = ScheduleAt.time(instant)
        assertTrue(schedule is ScheduleAt.Time, "Should be Time, got: ${schedule::class.simpleName}")
    }

    @Test
    fun `time preserves instant`() {
        val millis = System.currentTimeMillis()
        val instant = Instant.fromEpochMilliseconds(millis)
        val schedule = ScheduleAt.time(instant) as ScheduleAt.Time
        assertEquals(millis, schedule.timestamp.millisSinceUnixEpoch)
    }

    // --- Direct constructors ---

    @Test
    fun `Interval constructor with TimeDuration`() {
        val dur = TimeDuration.fromMillis(3000L)
        val schedule = ScheduleAt.Interval(dur)
        assertEquals(3000L, schedule.duration.millis)
    }

    @Test
    fun `Time constructor with Timestamp`() {
        val ts = Timestamp.fromMillis(42000L)
        val schedule = ScheduleAt.Time(ts)
        assertEquals(42000L, schedule.timestamp.millisSinceUnixEpoch)
    }

    // --- Equality ---

    @Test
    fun `Interval equality`() {
        val a = ScheduleAt.interval(5.seconds)
        val b = ScheduleAt.interval(5.seconds)
        assertEquals(a, b)
    }

    @Test
    fun `Time equality`() {
        val instant = Instant.fromEpochMilliseconds(1000L)
        val a = ScheduleAt.time(instant)
        val b = ScheduleAt.time(instant)
        assertEquals(a, b)
    }

    @Test
    fun `Interval and Time are not equal`() {
        val interval = ScheduleAt.interval(1.seconds)
        val time = ScheduleAt.time(Instant.fromEpochMilliseconds(1000L))
        assertTrue(interval != time, "Interval and Time should not be equal")
    }
}
