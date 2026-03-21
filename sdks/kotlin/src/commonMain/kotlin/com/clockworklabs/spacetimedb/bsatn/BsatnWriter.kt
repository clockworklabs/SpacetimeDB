package com.clockworklabs.spacetimedb.bsatn

class BsatnWriter(initialCapacity: Int = 256) {

    private var buffer = ByteArray(initialCapacity)
    private var position = 0

    private fun ensureCapacity(needed: Int) {
        val required = position + needed
        if (required > buffer.size) {
            val newSize = maxOf(buffer.size * 2, required)
            buffer = buffer.copyOf(newSize)
        }
    }

    fun writeBool(value: Boolean) {
        writeU8(if (value) 1u else 0u)
    }

    fun writeU8(value: UByte) {
        ensureCapacity(1)
        buffer[position++] = value.toByte()
    }

    fun writeI8(value: Byte) {
        ensureCapacity(1)
        buffer[position++] = value
    }

    fun writeU16(value: UShort) {
        ensureCapacity(2)
        val v = value.toInt()
        buffer[position++] = v.toByte()
        buffer[position++] = (v shr 8).toByte()
    }

    fun writeI16(value: Short) {
        ensureCapacity(2)
        val v = value.toInt()
        buffer[position++] = v.toByte()
        buffer[position++] = (v shr 8).toByte()
    }

    fun writeU32(value: UInt) {
        ensureCapacity(4)
        val v = value.toInt()
        buffer[position++] = v.toByte()
        buffer[position++] = (v shr 8).toByte()
        buffer[position++] = (v shr 16).toByte()
        buffer[position++] = (v shr 24).toByte()
    }

    fun writeI32(value: Int) {
        ensureCapacity(4)
        buffer[position++] = value.toByte()
        buffer[position++] = (value shr 8).toByte()
        buffer[position++] = (value shr 16).toByte()
        buffer[position++] = (value shr 24).toByte()
    }

    fun writeU64(value: ULong) {
        ensureCapacity(8)
        val v = value.toLong()
        for (i in 0 until 8) {
            buffer[position++] = (v shr (i * 8)).toByte()
        }
    }

    fun writeI64(value: Long) {
        ensureCapacity(8)
        for (i in 0 until 8) {
            buffer[position++] = (value shr (i * 8)).toByte()
        }
    }

    fun writeF32(value: Float) { writeI32(value.toRawBits()) }

    fun writeF64(value: Double) { writeI64(value.toRawBits()) }

    fun writeBytes(bytes: ByteArray) {
        ensureCapacity(bytes.size)
        bytes.copyInto(buffer, position)
        position += bytes.size
    }

    fun writeByteArray(bytes: ByteArray) {
        writeU32(bytes.size.toUInt())
        writeBytes(bytes)
    }

    fun writeString(value: String) {
        val bytes = value.encodeToByteArray()
        writeByteArray(bytes)
    }

    fun writeTag(tag: UByte) { writeU8(tag) }

    fun <T> writeArray(items: List<T>, writeElement: (BsatnWriter, T) -> Unit) {
        writeU32(items.size.toUInt())
        items.forEach { writeElement(this, it) }
    }

    fun <T> writeOption(value: T?, writeElement: (BsatnWriter, T) -> Unit) {
        if (value == null) {
            writeTag(0u)
        } else {
            writeTag(1u)
            writeElement(this, value)
        }
    }

    fun toByteArray(): ByteArray = buffer.copyOf(position)
}
