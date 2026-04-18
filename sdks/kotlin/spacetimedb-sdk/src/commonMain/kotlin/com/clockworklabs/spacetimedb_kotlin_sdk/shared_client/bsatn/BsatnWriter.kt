package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi
import kotlin.io.encoding.Base64
import kotlin.io.encoding.ExperimentalEncodingApi

/**
 * Resizable buffer for BSATN writing. Doubles capacity on overflow.
 */
internal class ResizableBuffer(initialCapacity: Int) {
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
 * Binary writer for BSATN encoding.
 * Little-endian, length-prefixed strings/byte arrays, auto-growing buffer.
 */
public class BsatnWriter(initialCapacity: Int = 256) {
    private var buffer = ResizableBuffer(initialCapacity)
    /** Number of bytes written so far. */
    @InternalSpacetimeApi
    public var offset: Int = 0
        private set

    private fun expandBuffer(additionalCapacity: Int) {
        val minCapacity = offset + additionalCapacity
        if (minCapacity > buffer.capacity) buffer.grow(minCapacity)
    }

    // ---------- Primitive Writes ----------

    /** Writes a boolean as a single byte (1 = true, 0 = false). */
    public fun writeBool(value: Boolean) {
        expandBuffer(1)
        buffer.buffer[offset] = if (value) 1 else 0
        offset += 1
    }

    /** Writes a single signed byte. */
    public fun writeByte(value: Byte) {
        expandBuffer(1)
        buffer.buffer[offset] = value
        offset += 1
    }

    /** Writes a single unsigned byte. */
    public fun writeUByte(value: UByte) {
        writeByte(value.toByte())
    }

    /** Writes a signed 8-bit integer. */
    public fun writeI8(value: Byte): Unit = writeByte(value)
    /** Writes an unsigned 8-bit integer. */
    public fun writeU8(value: UByte): Unit = writeUByte(value)

    /** Writes a signed 16-bit integer (little-endian). */
    public fun writeI16(value: Short) {
        expandBuffer(2)
        val v = value.toInt()
        buffer.buffer[offset] = (v and 0xFF).toByte()
        buffer.buffer[offset + 1] = ((v shr 8) and 0xFF).toByte()
        offset += 2
    }

    /** Writes an unsigned 16-bit integer (little-endian). */
    public fun writeU16(value: UShort): Unit = writeI16(value.toShort())

    /** Writes a signed 32-bit integer (little-endian). */
    public fun writeI32(value: Int) {
        expandBuffer(4)
        buffer.buffer[offset] = (value and 0xFF).toByte()
        buffer.buffer[offset + 1] = ((value shr 8) and 0xFF).toByte()
        buffer.buffer[offset + 2] = ((value shr 16) and 0xFF).toByte()
        buffer.buffer[offset + 3] = ((value shr 24) and 0xFF).toByte()
        offset += 4
    }

    /** Writes an unsigned 32-bit integer (little-endian). */
    public fun writeU32(value: UInt): Unit = writeI32(value.toInt())

    /** Writes a signed 64-bit integer (little-endian). */
    public fun writeI64(value: Long) {
        expandBuffer(8)
        for (i in 0 until 8) {
            buffer.buffer[offset + i] = ((value shr (i * 8)) and 0xFF).toByte()
        }
        offset += 8
    }

    /** Writes an unsigned 64-bit integer (little-endian). */
    public fun writeU64(value: ULong): Unit = writeI64(value.toLong())

    /** Writes a 32-bit IEEE 754 float (little-endian). */
    public fun writeF32(value: Float): Unit = writeI32(value.toRawBits())

    /** Writes a 64-bit IEEE 754 double (little-endian). */
    public fun writeF64(value: Double): Unit = writeI64(value.toRawBits())

    // ---------- Big Integer Writes ----------

    /** Writes a signed 128-bit integer (little-endian). */
    public fun writeI128(value: BigInteger): Unit = writeSignedBigIntLE(value, 16)

    /** Writes an unsigned 128-bit integer (little-endian). */
    public fun writeU128(value: BigInteger): Unit = writeUnsignedBigIntLE(value, 16)

    /** Writes a signed 256-bit integer (little-endian). */
    public fun writeI256(value: BigInteger): Unit = writeSignedBigIntLE(value, 32)

    /** Writes an unsigned 256-bit integer (little-endian). */
    public fun writeU256(value: BigInteger): Unit = writeUnsignedBigIntLE(value, 32)

    private fun writeSignedBigIntLE(value: BigInteger, byteSize: Int) {
        require(value.fitsInSignedBytes(byteSize)) {
            "Signed value does not fit in $byteSize bytes: $value"
        }
        expandBuffer(byteSize)
        value.writeLeBytes(buffer.buffer, offset, byteSize)
        offset += byteSize
    }

    private fun writeUnsignedBigIntLE(value: BigInteger, byteSize: Int) {
        require(value.signum() >= 0) {
            "Unsigned value must be non-negative: $value"
        }
        require(value.fitsInUnsignedBytes(byteSize)) {
            "Unsigned value does not fit in $byteSize bytes: $value"
        }
        expandBuffer(byteSize)
        value.writeLeBytes(buffer.buffer, offset, byteSize)
        offset += byteSize
    }

    // ---------- Strings / Byte Arrays ----------

    /** Length-prefixed string (U32 length + UTF-8 bytes) */
    public fun writeString(value: String) {
        val bytes = value.encodeToByteArray()
        writeU32(bytes.size.toUInt())
        writeRawBytes(bytes)
    }

    /** Length-prefixed byte array (U32 length + raw bytes) */
    public fun writeByteArray(value: ByteArray) {
        writeU32(value.size.toUInt())
        writeRawBytes(value)
    }

    /** Raw bytes, no length prefix */
    internal fun writeRawBytes(bytes: ByteArray) {
        expandBuffer(bytes.size)
        bytes.copyInto(buffer.buffer, offset)
        offset += bytes.size
    }

    // ---------- Utilities ----------

    /** Writes a sum-type tag byte. */
    public fun writeSumTag(tag: UByte): Unit = writeU8(tag)

    /** Writes a BSATN array length prefix (U32). */
    public fun writeArrayLen(length: Int) {
        require(length >= 0) { "Array length must be non-negative, got $length" }
        writeU32(length.toUInt())
    }

    /** Return the written buffer up to current offset */
    public fun toByteArray(): ByteArray = buffer.buffer.copyOf(offset)

    /** Returns the written bytes as a Base64-encoded string. */
    @OptIn(ExperimentalEncodingApi::class)
    @InternalSpacetimeApi
    public fun toBase64(): String = Base64.encode(toByteArray())

    /** Resets this writer, discarding all written data and re-allocating the buffer. */
    @InternalSpacetimeApi
    public fun reset(initialCapacity: Int = 256) {
        buffer = ResizableBuffer(initialCapacity)
        offset = 0
    }
}
