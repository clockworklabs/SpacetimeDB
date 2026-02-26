package com.clockworklabs.spacetimedb.bsatn

class BsatnReader(private val data: ByteArray, private var offset: Int = 0) {

    val remaining: Int get() = data.size - offset

    val isExhausted: Boolean get() = offset >= data.size

    private fun require(count: Int) {
        if (offset + count > data.size) {
            throw IllegalStateException(
                "BSATN: unexpected end of data at offset $offset, " +
                    "need $count bytes but only ${data.size - offset} remain"
            )
        }
    }

    fun readU8(): UByte {
        require(1)
        return data[offset++].toUByte()
    }

    fun readI8(): Byte {
        require(1)
        return data[offset++]
    }

    fun readBool(): Boolean = readU8().toInt() != 0

    fun readU16(): UShort {
        require(2)
        val v = (data[offset].toUByte().toInt() or (data[offset + 1].toUByte().toInt() shl 8)).toUShort()
        offset += 2
        return v
    }

    fun readI16(): Short {
        require(2)
        val v = (data[offset].toUByte().toInt() or (data[offset + 1].toUByte().toInt() shl 8)).toShort()
        offset += 2
        return v
    }

    fun readU32(): UInt {
        require(4)
        val v = (data[offset].toUByte().toUInt()) or
            (data[offset + 1].toUByte().toUInt() shl 8) or
            (data[offset + 2].toUByte().toUInt() shl 16) or
            (data[offset + 3].toUByte().toUInt() shl 24)
        offset += 4
        return v
    }

    fun readI32(): Int {
        require(4)
        val v = (data[offset].toUByte().toInt()) or
            (data[offset + 1].toUByte().toInt() shl 8) or
            (data[offset + 2].toUByte().toInt() shl 16) or
            (data[offset + 3].toUByte().toInt() shl 24)
        offset += 4
        return v
    }

    fun readU64(): ULong {
        require(8)
        var v = 0UL
        for (i in 0 until 8) {
            v = v or (data[offset + i].toUByte().toULong() shl (i * 8))
        }
        offset += 8
        return v
    }

    fun readI64(): Long {
        require(8)
        var v = 0L
        for (i in 0 until 8) {
            v = v or ((data[offset + i].toUByte().toLong()) shl (i * 8))
        }
        offset += 8
        return v
    }

    fun readF32(): Float = Float.fromBits(readI32())

    fun readF64(): Double = Double.fromBits(readI64())

    fun readBytes(count: Int): ByteArray {
        require(count)
        val result = data.copyOfRange(offset, offset + count)
        offset += count
        return result
    }

    fun readByteArray(): ByteArray {
        val len = readU32().toInt()
        return readBytes(len)
    }

    fun readString(): String {
        val bytes = readByteArray()
        return bytes.decodeToString()
    }

    fun readTag(): UByte = readU8()

    fun <T> readArray(readElement: (BsatnReader) -> T): List<T> {
        val count = readU32().toInt()
        return List(count) { readElement(this) }
    }

    fun <T> readOption(readElement: (BsatnReader) -> T): T? {
        return when (readTag().toInt()) {
            0 -> null
            1 -> readElement(this)
            else -> throw IllegalStateException("Invalid Option tag")
        }
    }
}
