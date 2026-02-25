package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.fromEpochMicroseconds
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toEpochMicroseconds
import kotlin.time.Clock
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Instant

data class Timestamp(val instant: Instant) {
    companion object {
        private const val MICROS_PER_MILLIS = 1_000L

        val UNIX_EPOCH: Timestamp = Timestamp(Instant.fromEpochMilliseconds(0))

        fun now(): Timestamp = Timestamp(Clock.System.now())

        fun decode(reader: BsatnReader): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(reader.readI64()))

        fun fromEpochMicroseconds(micros: Long): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(micros))

        fun fromMillis(millis: Long): Timestamp =
            Timestamp(Instant.fromEpochMilliseconds(millis))
    }

    fun encode(writer: BsatnWriter) {
        writer.writeI64(instant.toEpochMicroseconds())
    }

    /** Microseconds since Unix epoch */
    val microsSinceUnixEpoch: Long
        get() = instant.toEpochMicroseconds()

    /** Milliseconds since Unix epoch */
    val millisSinceUnixEpoch: Long
        get() = instant.toEpochMilliseconds()

    /** Duration since another Timestamp */
    fun since(other: Timestamp): TimeDuration =
        TimeDuration((microsSinceUnixEpoch - other.microsSinceUnixEpoch).microseconds)

    operator fun plus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch + duration.micros)

    operator fun minus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch - duration.micros)

    operator fun minus(other: Timestamp): TimeDuration =
        TimeDuration((microsSinceUnixEpoch - other.microsSinceUnixEpoch).microseconds)

    operator fun compareTo(other: Timestamp): Int =
        microsSinceUnixEpoch.compareTo(other.microsSinceUnixEpoch)

    fun toISOString(): String {
        val micros = microsSinceUnixEpoch
        val seconds = micros / 1_000_000
        val microFraction = (micros % 1_000_000).toInt()
        val base = Instant.fromEpochSeconds(seconds).toString().removeSuffix("Z")
        return "$base.${microFraction.toString().padStart(6, '0')}Z"
    }

    override fun toString(): String = toISOString()
}
