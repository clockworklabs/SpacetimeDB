package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.fromEpochMicroseconds
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toEpochMicroseconds
import kotlin.time.Clock
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Instant

public data class Timestamp(val instant: Instant) : Comparable<Timestamp> {
    public companion object {
        public val UNIX_EPOCH: Timestamp = Timestamp(Instant.fromEpochMilliseconds(0))

        public fun now(): Timestamp = Timestamp(Clock.System.now())

        public fun decode(reader: BsatnReader): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(reader.readI64()))

        public fun fromEpochMicroseconds(micros: Long): Timestamp =
            Timestamp(Instant.fromEpochMicroseconds(micros))

        public fun fromMillis(millis: Long): Timestamp =
            Timestamp(Instant.fromEpochMilliseconds(millis))
    }

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

    public operator fun plus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch + duration.micros)

    public operator fun minus(duration: TimeDuration): Timestamp =
        fromEpochMicroseconds(microsSinceUnixEpoch - duration.micros)

    public operator fun minus(other: Timestamp): TimeDuration =
        TimeDuration((microsSinceUnixEpoch - other.microsSinceUnixEpoch).microseconds)

    override operator fun compareTo(other: Timestamp): Int =
        microsSinceUnixEpoch.compareTo(other.microsSinceUnixEpoch)

    public fun toISOString(): String {
        val micros = microsSinceUnixEpoch
        val seconds = micros.floorDiv(1_000_000L)
        val microFraction = micros.mod(1_000_000L).toInt()
        val base = Instant.fromEpochSeconds(seconds).toString().removeSuffix("Z")
        return "$base.${microFraction.toString().padStart(6, '0')}Z"
    }

    override fun toString(): String = toISOString()
}
