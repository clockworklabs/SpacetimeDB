package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import kotlin.jvm.JvmInline

/** Server-side timestamp in microseconds since the Unix epoch. */
@JvmInline
value class Timestamp(val microseconds: Long) {
    companion object {
        fun read(reader: BsatnReader): Timestamp = Timestamp(reader.readI64())

        fun write(writer: BsatnWriter, value: Timestamp) { writer.writeI64(value.microseconds) }
    }
}