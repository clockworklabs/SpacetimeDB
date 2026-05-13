package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.protocol.TimeDuration

sealed class ScheduleAt {
    data class Interval(val every: TimeDuration) : ScheduleAt()
    data class Time(val time: Timestamp) : ScheduleAt()

    companion object {
        fun read(reader: BsatnReader): ScheduleAt {
            return when (reader.readTag().toInt()) {
                0 -> Interval(TimeDuration.read(reader))
                1 -> Time(Timestamp.read(reader))
                else -> throw IllegalStateException("Unknown ScheduleAt tag")
            }
        }

        fun write(writer: BsatnWriter, value: ScheduleAt) {
            when (value) {
                is Interval -> {
                    writer.writeTag(0u)
                    TimeDuration.write(writer, value.every)
                }
                is Time -> {
                    writer.writeTag(1u)
                    Timestamp.write(writer, value.time)
                }
            }
        }
    }
}
