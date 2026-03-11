package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.time.Duration
import kotlin.time.Instant

public sealed interface ScheduleAt {
    public data class Interval(val duration: TimeDuration) : ScheduleAt
    public data class Time(val timestamp: Timestamp) : ScheduleAt

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

        public fun interval(interval: Duration): ScheduleAt = Interval(TimeDuration(interval))
        public fun time(time: Instant): ScheduleAt = Time(Timestamp(time))

        public fun decode(reader: BsatnReader): ScheduleAt {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> Interval(TimeDuration.decode(reader))
                1 -> Time(Timestamp.decode(reader))
                else -> error("Unknown ScheduleAt tag: $tag")
            }
        }
    }
}
