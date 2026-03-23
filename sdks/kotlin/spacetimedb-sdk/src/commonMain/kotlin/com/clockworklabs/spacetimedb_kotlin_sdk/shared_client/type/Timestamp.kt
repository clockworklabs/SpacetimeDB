package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.fromEpochMicroseconds
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toEpochMicroseconds
import kotlin.time.Clock
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Instant

/** A microsecond-precision timestamp backed by [Instant]. */
public data class Timestamp(val instant: Instant) : Comparable<Timestamp> {
    public companion object {
        /** The Unix epoch (1970-01-01T00:00:00Z). */
        public val UNIX_EPOCH: Timestamp = Timestamp(Instant.fromEpochMilliseconds(0))

        /** Returns the current system time as a [Timestamp]. */
        public fun now(): Timestamp = Timestamp(Clock.System.now())

        /** Decodes a [Timestamp] from BSATN. */
        public fun decode(reader: BsatnReader): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(reader.readI64()))

        /** Creates a [Timestamp] from microseconds since the Unix epoch. */
        public fun fromEpochMicroseconds(micros: Long): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(micros))

        /** Creates a [Timestamp] from milliseconds since the Unix epoch. */
        public fun fromMillis(millis: Long): Timestamp =
            Timestamp(Instant.fromEpochMilliseconds(millis))
    }

    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter) {
        writer.writeI64(instant.toEpochMicroseconds())
    }

    /** Microseconds since Unix epoch */
    public val microsSinceUnixEpoch: Long
        get() = instant.toEpochMicroseconds()

    /** Milliseconds since Unix epoch */
    public val millisSinceUnixEpoch: Long
        get() = instant.toEpochMilliseconds()

    /** Duration since another Timestamp */
    public fun since(other: Timestamp): TimeDuration =
        TimeDuration((microsSinceUnixEpoch - other.microsSinceUnixEpoch).microseconds)

    /** Returns a new [Timestamp] offset forward by [duration]. */
    public operator fun plus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch + duration.micros)

    /** Returns a new [Timestamp] offset backward by [duration]. */
    public operator fun minus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch - duration.micros)

    /** Returns the duration between this timestamp and [other]. */
    public operator fun minus(other: Timestamp): TimeDuration =
        TimeDuration((microsSinceUnixEpoch - other.microsSinceUnixEpoch).microseconds)

    override operator fun compareTo(other: Timestamp): Int =
        microsSinceUnixEpoch.compareTo(other.microsSinceUnixEpoch)

    /** Returns this timestamp as an ISO 8601 string with microsecond precision. */
    public fun toISOString(): String {
        val micros = microsSinceUnixEpoch
        val seconds = micros.floorDiv(1_000_000L)
        val microFraction = micros.mod(1_000_000L).toInt()
        val base = Instant.fromEpochSeconds(seconds).toString().removeSuffix("Z")
        return "$base.${microFraction.toString().padStart(6, '0')}Z"
    }

    override fun toString(): String = toISOString()
}
