package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi

/**
 * Binary reader for BSATN decoding. All multi-byte values are little-endian.
 */
public class BsatnReader(internal var data: ByteArray, offset: Int = 0, private var limit: Int = data.size) {

    /** Current read position within the buffer. */
    @InternalSpacetimeApi
    public var offset: Int = offset
        private set

    /** Number of bytes remaining to be read. */
    @InternalSpacetimeApi
    public val remaining: Int get() = limit - offset

    /** Resets this reader to decode from a new byte array from the beginning. */
    internal fun reset(newData: ByteArray) {
        data = newData
        offset = 0
        limit = newData.size
    }

    /** Advances the read position by [n] bytes without returning data. */
    internal fun skip(n: Int) {
        ensure(n)
        offset += n
    }

    private fun ensure(n: Int) {
        check(n in 0..remaining) { "BsatnReader: need $n bytes but only $remaining remain" }
    }

    /** Reads a BSATN boolean (1 byte, nonzero = true). */
    public fun readBool(): Boolean {
        ensure(1)
        val b = data[offset].toInt() and 0xFF
        offset += 1
        return b != 0
    }

    /** Reads a single signed byte. */
    public fun readByte(): Byte {
        ensure(1)
        val b = data[offset]
        offset += 1
        return b
    }

    /** Reads a signed 8-bit integer. */
    public fun readI8(): Byte = readByte()

    /** Reads an unsigned 8-bit integer. */
    public fun readU8(): UByte {
        ensure(1)
        val b = data[offset].toUByte()
        offset += 1
        return b
    }

    /** Reads a signed 16-bit integer (little-endian). */
    public fun readI16(): Short {
        ensure(2)
        val b0 = data[offset].toInt() and 0xFF
        val b1 = data[offset + 1].toInt() and 0xFF
        offset += 2
        return (b0 or (b1 shl 8)).toShort()
    }

    /** Reads an unsigned 16-bit integer (little-endian). */
    public fun readU16(): UShort = readI16().toUShort()

    /** Reads a signed 32-bit integer (little-endian). */
    public fun readI32(): Int {
        ensure(4)
        val b0 = data[offset].toLong() and 0xFF
        val b1 = data[offset + 1].toLong() and 0xFF
        val b2 = data[offset + 2].toLong() and 0xFF
        val b3 = data[offset + 3].toLong() and 0xFF
        offset += 4
        return (b0 or (b1 shl 8) or (b2 shl 16) or (b3 shl 24)).toInt()
    }

    /** Reads an unsigned 32-bit integer (little-endian). */
    public fun readU32(): UInt = readI32().toUInt()

    /** Reads a signed 64-bit integer (little-endian). */
    public fun readI64(): Long {
        ensure(8)
        var result = 0L
        for (i in 0 until 8) {
            result = result or ((data[offset + i].toLong() and 0xFF) shl (i * 8))
        }
        offset += 8
        return result
    }

    /** Reads an unsigned 64-bit integer (little-endian). */
    public fun readU64(): ULong = readI64().toULong()

    /** Reads a 32-bit IEEE 754 float (little-endian). */
    public fun readF32(): Float = Float.fromBits(readI32())

    /** Reads a 64-bit IEEE 754 double (little-endian). */
    public fun readF64(): Double = Double.fromBits(readI64())

    /** Reads a signed 128-bit integer (little-endian) as a [BigInteger]. */
    public fun readI128(): BigInteger {
        ensure(16)
        val result = BigInteger.fromLeBytes(data, offset, 16)
        offset += 16
        return result
    }

    /** Reads an unsigned 128-bit integer (little-endian) as a [BigInteger]. */
    public fun readU128(): BigInteger {
        ensure(16)
        val result = BigInteger.fromLeBytesUnsigned(data, offset, 16)
        offset += 16
        return result
    }

    /** Reads a signed 256-bit integer (little-endian) as a [BigInteger]. */
    public fun readI256(): BigInteger {
        ensure(32)
        val result = BigInteger.fromLeBytes(data, offset, 32)
        offset += 32
        return result
    }

    /** Reads an unsigned 256-bit integer (little-endian) as a [BigInteger]. */
    public fun readU256(): BigInteger {
        ensure(32)
        val result = BigInteger.fromLeBytesUnsigned(data, offset, 32)
        offset += 32
        return result
    }

    /** Reads a BSATN length-prefixed UTF-8 string. */
    public fun readString(): String {
        val len = readU32()
        check(len <= Int.MAX_VALUE.toUInt()) { "String length $len exceeds maximum supported size" }
        val bytes = readRawBytes(len.toInt())
        return bytes.decodeToString()
    }

    /** Reads a BSATN length-prefixed byte array. */
    public fun readByteArray(): ByteArray {
        val len = readU32()
        check(len <= Int.MAX_VALUE.toUInt()) { "Byte array length $len exceeds maximum supported size" }
        return readRawBytes(len.toInt())
    }

    private fun readRawBytes(length: Int): ByteArray {
        ensure(length)
        val result = data.copyOfRange(offset, offset + length)
        offset += length
        return result
    }

    /**
     * Returns a zero-copy view of the underlying buffer.
     * The returned BsatnReader shares the same backing array — no allocation.
     */
    internal fun readRawBytesView(length: Int): BsatnReader {
        ensure(length)
        val view = BsatnReader(data, offset, offset + length)
        offset += length
        return view
    }

    /**
     * Returns a copy of the underlying buffer between [from] and [to].
     * Used when a materialized ByteArray is needed (e.g. for content-based keying).
     */
    internal fun sliceArray(from: Int, to: Int): ByteArray {
        check(to in from..limit) {
            "sliceArray($from, $to) out of view bounds (limit=$limit)"
        }
        return data.copyOfRange(from, to)
    }

    /** Reads a sum-type tag byte. */
    public fun readSumTag(): UByte = readU8()

    /** Reads a BSATN array length prefix (U32), returned as Int for indexing. */
    public fun readArrayLen(): Int {
        val len = readU32()
        check(len <= Int.MAX_VALUE.toUInt()) { "Array length $len exceeds maximum supported size" }
        return len.toInt()
    }
}
