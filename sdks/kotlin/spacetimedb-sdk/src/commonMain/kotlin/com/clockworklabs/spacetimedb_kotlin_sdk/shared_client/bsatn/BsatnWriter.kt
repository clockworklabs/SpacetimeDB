package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import com.ionspin.kotlin.bignum.integer.BigInteger
import com.ionspin.kotlin.bignum.integer.util.toTwosComplementByteArray
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
        val bitSize = byteSize * 8
        val min = -BigInteger.ONE.shl(bitSize - 1)       // -2^(n-1)
        val max = BigInteger.ONE.shl(bitSize - 1) - BigInteger.ONE  // 2^(n-1) - 1
        require(value in min..max) {
            "Signed value does not fit in $byteSize bytes (range $min..$max): $value"
        }
        writeBigIntLE(value, byteSize)
    }

    private fun writeUnsignedBigIntLE(value: BigInteger, byteSize: Int) {
        require(value.signum() >= 0) {
            "Unsigned value must be non-negative: $value"
        }
        val max = BigInteger.ONE.shl(byteSize * 8) - BigInteger.ONE  // 2^n - 1
        require(value <= max) {
            "Unsigned value does not fit in $byteSize bytes (max $max): $value"
        }
        writeBigIntLE(value, byteSize)
    }

    private fun writeBigIntLE(value: BigInteger, byteSize: Int) {
        expandBuffer(byteSize)
        // Two's complement big-endian bytes (sign-aware, like java.math.BigInteger)
        val beBytes = value.toTwosComplementByteArray()
        val padByte: Byte = if (value.signum() < 0) 0xFF.toByte() else 0
        if (beBytes.size > byteSize) {
            val srcStart = beBytes.size - byteSize
            val isSignExtensionOnly = (0 until srcStart).all { beBytes[it] == padByte }
            require(isSignExtensionOnly) {
                "BigInteger value does not fit in $byteSize bytes: $value"
            }
        }
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
    public fun writeRawBytes(bytes: ByteArray) {
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
    public fun toBase64(): String = Base64.Default.encode(toByteArray())

    /** Resets this writer, discarding all written data and re-allocating the buffer. */
    public fun reset(initialCapacity: Int = 256) {
        buffer = ResizableBuffer(initialCapacity)
        offset = 0
    }
}
