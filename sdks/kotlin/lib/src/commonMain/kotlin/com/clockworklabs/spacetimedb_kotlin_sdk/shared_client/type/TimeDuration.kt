package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.math.abs
import kotlin.time.Duration
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds

public data class TimeDuration(val duration: Duration) : Comparable<TimeDuration> {
    public fun encode(writer: BsatnWriter): Unit = writer.writeI64(duration.inWholeMicroseconds)
    public val micros: Long get() = duration.inWholeMicroseconds
    public val millis: Long get() = duration.inWholeMilliseconds

    public operator fun plus(other: TimeDuration): TimeDuration =
        TimeDuration(duration + other.duration)

    public operator fun minus(other: TimeDuration): TimeDuration =
        TimeDuration(duration - other.duration)

    override operator fun compareTo(other: TimeDuration): Int =
        duration.compareTo(other.duration)

    override fun toString(): String {
        val sign = if (duration.inWholeMicroseconds >= 0) "+" else "-"
        val abs = abs(duration.inWholeMicroseconds)
        val secs = abs / 1_000_000
        val frac = abs % 1_000_000
        return "$sign$secs.${frac.toString().padStart(6, '0')}"
    }

    public companion object {
        public fun decode(reader: BsatnReader): TimeDuration = TimeDuration(reader.readI64().microseconds)
        public fun fromMillis(millis: Long): TimeDuration = TimeDuration(millis.milliseconds)
    }
}
