package com.clockworklabs.spacetimedb.bsatn

sealed class RowSizeHint {
    data class FixedSize(val rowSize: UShort) : RowSizeHint()
    data class RowOffsets(val offsets: List<ULong>) : RowSizeHint()

    companion object {
        fun read(reader: BsatnReader): RowSizeHint {
            return when (reader.readTag().toInt()) {
                0 -> FixedSize(reader.readU16())
                1 -> RowOffsets(reader.readArray { it.readU64() })
                else -> throw IllegalStateException("Invalid RowSizeHint tag")
            }
        }

        fun write(writer: BsatnWriter, value: RowSizeHint) {
            when (value) {
                is FixedSize -> {
                    writer.writeTag(0u)
                    writer.writeU16(value.rowSize)
                }
                is RowOffsets -> {
                    writer.writeTag(1u)
                    writer.writeArray(value.offsets) { w, v -> w.writeU64(v) }
                }
            }
        }
    }
}

class BsatnRowList(
    val sizeHint: RowSizeHint,
    val rowsData: ByteArray,
) {
    fun decodeRows(): List<ByteArray> {
        if (rowsData.isEmpty()) return emptyList()

        return when (val hint = sizeHint) {
            is RowSizeHint.FixedSize -> {
                val rowSize = hint.rowSize.toInt()
                if (rowSize == 0) return emptyList()
                val count = rowsData.size / rowSize
                List(count) { i ->
                    rowsData.copyOfRange(i * rowSize, (i + 1) * rowSize)
                }
            }
            is RowSizeHint.RowOffsets -> {
                val offsets = hint.offsets
                List(offsets.size) { i ->
                    val start = offsets[i].toInt()
                    val end = if (i + 1 < offsets.size) offsets[i + 1].toInt() else rowsData.size
                    rowsData.copyOfRange(start, end)
                }
            }
        }
    }

    companion object {
        fun read(reader: BsatnReader): BsatnRowList {
            val sizeHint = RowSizeHint.read(reader)
            val rowsData = reader.readByteArray()
            return BsatnRowList(sizeHint, rowsData)
        }

        fun write(writer: BsatnWriter, value: BsatnRowList) {
            RowSizeHint.write(writer, value.sizeHint)
            writer.writeByteArray(value.rowsData)
        }
    }
}
