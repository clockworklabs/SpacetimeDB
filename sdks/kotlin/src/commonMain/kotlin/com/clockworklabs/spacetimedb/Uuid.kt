package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import kotlin.uuid.Uuid

fun Uuid.Companion.read(reader: BsatnReader): Uuid {
    val msb = reader.readI64()
    val lsb = reader.readI64()
    val bytes = ByteArray(16)
    for (i in 0 until 8) bytes[7 - i] = (msb shr (i * 8)).toByte()
    for (i in 0 until 8) bytes[15 - i] = (lsb shr (i * 8)).toByte()
    return Uuid.fromByteArray(bytes)
}

fun Uuid.Companion.write(writer: BsatnWriter, value: Uuid) {
    val bytes = value.toByteArray()
    var msb = 0L
    var lsb = 0L
    for (i in 0 until 8) msb = msb.shl(8) or (bytes[i].toLong() and 0xFF)
    for (i in 8 until 16) lsb = lsb.shl(8) or (bytes[i].toLong() and 0xFF)
    writer.writeI64(msb)
    writer.writeI64(lsb)
}
