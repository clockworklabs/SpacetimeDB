package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.BsatnRowList
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.RowSizeHint

data class SampleRow(val id: Int, val name: String)

fun SampleRow.encode(): ByteArray {
    val writer = BsatnWriter()
    writer.writeI32(id)
    writer.writeString(name)
    return writer.toByteArray()
}

fun decodeSampleRow(reader: BsatnReader): SampleRow {
    val id = reader.readI32()
    val name = reader.readString()
    return SampleRow(id, name)
}

fun buildRowList(vararg rows: ByteArray): BsatnRowList {
    val writer = BsatnWriter()
    val offsets = mutableListOf<ULong>()
    var offset = 0uL
    for (row in rows) {
        offsets.add(offset)
        writer.writeRawBytes(row)
        offset += row.size.toULong()
    }
    return BsatnRowList(
        sizeHint = RowSizeHint.RowOffsets(offsets),
        rowsData = writer.toByteArray(),
    )
}

val STUB_CTX: EventContext = StubEventContext()

fun createSampleCache(): TableCache<SampleRow, Int> =
    TableCache.withPrimaryKey(::decodeSampleRow) { it.id }
