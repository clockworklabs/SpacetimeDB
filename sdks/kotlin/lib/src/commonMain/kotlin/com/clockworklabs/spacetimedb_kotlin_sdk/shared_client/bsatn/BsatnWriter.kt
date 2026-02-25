@file:Suppress("MemberVisibilityCanBePrivate", "unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import java.math.BigInteger
import java.util.Base64

/**
 * Resizable buffer for BSATN writing. Doubles capacity on overflow.
 */
class ResizableBuffer(initialCapacity: Int) {
    var buffer: ByteArray = ByteArray(initialCapacity)
        private set

    val capacity: Int get() = buffer.size

    fun grow(newSize: Int) {
        if (newSize <= buffer.size) return
        val newCapacity = maxOf(buffer.size * 2, newSize)
        buffer = buffer.copyOf(newCapacity)
    }
}

/**
 * Binary writer for BSATN encoding. Mirrors TypeScript BinaryWriter.
 * Little-endian, length-prefixed strings/byte arrays, auto-growing buffer.
 */
class BsatnWriter(initialCapacity: Int = 256) {
    private var buffer = ResizableBuffer(initialCapacity)
    var offset: Int = 0
        private set

    private fun expandBuffer(additionalCapacity: Int) {
        val minCapacity = offset + additionalCapacity
        if (minCapacity > buffer.capacity) buffer.grow(minCapacity)
    }

    // ---------- Primitive Writes ----------

    fun writeBool(value: Boolean) {
        expandBuffer(1)
        buffer.buffer[offset] = if (value) 1 else 0
        offset += 1
    }

    fun writeByte(value: Byte) {
        expandBuffer(1)
        buffer.buffer[offset] = value
        offset += 1
    }

    fun writeUByte(value: UByte) {
        writeByte(value.toByte())
    }

    fun writeI8(value: Byte) = writeByte(value)
    fun writeU8(value: UByte) = writeUByte(value)

    fun writeI16(value: Short) {
        expandBuffer(2)
        val v = value.toInt()
        buffer.buffer[offset] = (v and 0xFF).toByte()
        buffer.buffer[offset + 1] = ((v shr 8) and 0xFF).toByte()
        offset += 2
    }

    fun writeU16(value: UShort) = writeI16(value.toShort())

    fun writeI32(value: Int) {
        expandBuffer(4)
        buffer.buffer[offset] = (value and 0xFF).toByte()
        buffer.buffer[offset + 1] = ((value shr 8) and 0xFF).toByte()
        buffer.buffer[offset + 2] = ((value shr 16) and 0xFF).toByte()
        buffer.buffer[offset + 3] = ((value shr 24) and 0xFF).toByte()
        offset += 4
    }

    fun writeU32(value: UInt) = writeI32(value.toInt())

    fun writeI64(value: Long) {
        expandBuffer(8)
        for (i in 0 until 8) {
            buffer.buffer[offset + i] = ((value shr (i * 8)) and 0xFF).toByte()
        }
        offset += 8
    }

    fun writeU64(value: ULong) = writeI64(value.toLong())

    fun writeF32(value: Float) = writeI32(value.toRawBits())

    fun writeF64(value: Double) = writeI64(value.toRawBits())

    // ---------- Big Integer Writes ----------

    fun writeI128(value: BigInteger) = writeBigIntLE(value, 16, signed = true)

    fun writeU128(value: BigInteger) = writeBigIntLE(value, 16, signed = false)

    fun writeI256(value: BigInteger) = writeBigIntLE(value, 32, signed = true)

    fun writeU256(value: BigInteger) = writeBigIntLE(value, 32, signed = false)

    private fun writeBigIntLE(value: BigInteger, byteSize: Int, signed: Boolean) {
        expandBuffer(byteSize)
        val beBytes = value.toByteArray() // big-endian, sign-magnitude
        val padByte: Byte = if (value.signum() < 0) 0xFF.toByte() else 0
        val padded = ByteArray(byteSize) { padByte }
        // Copy big-endian bytes right-aligned into padded, then reverse for LE
        val srcStart = maxOf(0, beBytes.size - byteSize)
        val dstStart = maxOf(0, byteSize - beBytes.size)
        beBytes.copyInto(padded, dstStart, srcStart, beBytes.size)
        padded.reverse()
        writeRawBytes(padded)
    }

    // ---------- Strings / Byte Arrays ----------

    /** Length-prefixed string (U32 length + UTF-8 bytes) */
    fun writeString(value: String) {
        val bytes = value.encodeToByteArray()
        writeU32(bytes.size.toUInt())
        writeRawBytes(bytes)
    }

    /** Length-prefixed byte array (U32 length + raw bytes) */
    fun writeByteArray(value: ByteArray) {
        writeU32(value.size.toUInt())
        writeRawBytes(value)
    }

    /** Raw bytes, no length prefix */
    fun writeRawBytes(bytes: ByteArray) {
        expandBuffer(bytes.size)
        bytes.copyInto(buffer.buffer, offset)
        offset += bytes.size
    }

    // ---------- Utilities ----------

    fun writeSumTag(tag: UByte) = writeU8(tag)

    fun writeArrayLen(length: Int) = writeU32(length.toUInt())

    /** Return the written buffer up to current offset */
    fun toByteArray(): ByteArray = buffer.buffer.copyOf(offset)

    fun toBase64(): String = Base64.getEncoder().encodeToString(toByteArray())

    fun reset(initialCapacity: Int = 256) {
        buffer = ResizableBuffer(initialCapacity)
        offset = 0
    }
}
