package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.math.abs
import kotlin.time.Duration
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds

data class TimeDuration(val duration: Duration) {
    fun encode(writer: BsatnWriter) = writer.writeI64(duration.inWholeMicroseconds)
    val micros: Long get() = duration.inWholeMicroseconds
    val millis: Long get() = duration.inWholeMilliseconds

    operator fun plus(other: TimeDuration): TimeDuration =
        TimeDuration(duration + other.duration)

    operator fun minus(other: TimeDuration): TimeDuration =
        TimeDuration(duration - other.duration)

    operator fun compareTo(other: TimeDuration): Int =
        duration.compareTo(other.duration)

    override fun toString(): String {
        val sign = if (duration.inWholeMicroseconds >= 0) "+" else "-"
        val abs = abs(duration.inWholeMicroseconds)
        val secs = abs / 1_000_000
        val frac = abs % 1_000_000
        return "$sign$secs.${frac.toString().padStart(6, '0')}"
    }

    companion object {
        fun decode(reader: BsatnReader): TimeDuration = TimeDuration(reader.readI64().microseconds)
        fun fromMillis(millis: Long): TimeDuration = TimeDuration(millis.milliseconds)
    }
}