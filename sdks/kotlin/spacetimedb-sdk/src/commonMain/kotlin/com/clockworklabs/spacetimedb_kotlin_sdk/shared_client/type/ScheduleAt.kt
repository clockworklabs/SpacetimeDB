package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.time.Duration
import kotlin.time.Instant

/** Specifies when a scheduled reducer should fire: at a fixed time or after an interval. */
public sealed interface ScheduleAt {
    /** Schedule by repeating interval. */
    public data class Interval(val duration: TimeDuration) : ScheduleAt
    /** Schedule at a specific point in time. */
    public data class Time(val timestamp: Timestamp) : ScheduleAt

    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter) {
        when (this) {
            is Interval -> {
                writer.writeSumTag(INTERVAL_TAG)
                duration.encode(writer)
            }

            is Time -> {
                writer.writeSumTag(TIME_TAG)
                timestamp.encode(writer)
            }
        }
    }

    public companion object {
        private const val INTERVAL_TAG: UByte = 0u
        private const val TIME_TAG: UByte = 1u

        /** Creates a [ScheduleAt] from a repeating [interval]. */
        public fun interval(interval: Duration): ScheduleAt = Interval(TimeDuration(interval))
        /** Creates a [ScheduleAt] for a specific point in [time]. */
        public fun time(time: Instant): ScheduleAt = Time(Timestamp(time))

        /** Decodes a [ScheduleAt] from BSATN. */
        public fun decode(reader: BsatnReader): ScheduleAt {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> Interval(TimeDuration.decode(reader))
                1 -> Time(Timestamp.decode(reader))
                else -> error("Unknown ScheduleAt tag: $tag")
            }
        }
    }
}
