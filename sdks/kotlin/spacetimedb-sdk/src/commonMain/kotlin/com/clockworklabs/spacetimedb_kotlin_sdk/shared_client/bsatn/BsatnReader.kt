package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import com.ionspin.kotlin.bignum.integer.BigInteger

/**
 * Binary reader for BSATN decoding. All multi-byte values are little-endian.
 */
public class BsatnReader(internal var data: ByteArray, offset: Int = 0, private var limit: Int = data.size) {
    public companion object {
        /** Convert a signed Long to an unsigned BigInteger (0..2^64-1). */
        private fun unsignedBigInt(v: Long): BigInteger = BigInteger.fromULong(v.toULong())
    }

    public var offset: Int = offset
        private set

    public val remaining: Int get() = limit - offset

    public fun reset(newData: ByteArray) {
        data = newData
        offset = 0
        limit = newData.size
    }

    public fun skip(n: Int) {
        ensure(n)
        offset += n
    }

    private fun ensure(n: Int) {
        check(n >= 0 && remaining >= n) { "BsatnReader: need $n bytes but only $remaining remain" }
    }

    public fun readBool(): Boolean {
        ensure(1)
        val b = data[offset].toInt() and 0xFF
        offset += 1
        return b != 0
    }

    public fun readByte(): Byte {
        ensure(1)
        val b = data[offset]
        offset += 1
        return b
    }

    public fun readI8(): Byte = readByte()

    public fun readU8(): UByte {
        ensure(1)
        val b = data[offset].toUByte()
        offset += 1
        return b
    }

    public fun readI16(): Short {
        ensure(2)
        val b0 = data[offset].toInt() and 0xFF
        val b1 = data[offset + 1].toInt() and 0xFF
        offset += 2
        return (b0 or (b1 shl 8)).toShort()
    }

    public fun readU16(): UShort = readI16().toUShort()

    public fun readI32(): Int {
        ensure(4)
        val b0 = data[offset].toLong() and 0xFF
        val b1 = data[offset + 1].toLong() and 0xFF
        val b2 = data[offset + 2].toLong() and 0xFF
        val b3 = data[offset + 3].toLong() and 0xFF
        offset += 4
        return (b0 or (b1 shl 8) or (b2 shl 16) or (b3 shl 24)).toInt()
    }

    public fun readU32(): UInt = readI32().toUInt()

    public fun readI64(): Long {
        ensure(8)
        var result = 0L
        for (i in 0 until 8) {
            result = result or ((data[offset + i].toLong() and 0xFF) shl (i * 8))
        }
        offset += 8
        return result
    }

    public fun readU64(): ULong = readI64().toULong()

    public fun readF32(): Float = Float.fromBits(readI32())

    public fun readF64(): Double = Double.fromBits(readI64())

    public fun readI128(): BigInteger {
        val p0 = readI64()
        val p1 = readI64() // signed top chunk

        return BigInteger(p1).shl(64)
            .add(unsignedBigInt(p0))
    }

    public fun readU128(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()

        return unsignedBigInt(p1).shl(64)
            .add(unsignedBigInt(p0))
    }

    public fun readI256(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()
        val p2 = readI64()
        val p3 = readI64() // signed top chunk

        return BigInteger(p3).shl(64 * 3)
            .add(unsignedBigInt(p2).shl(64 * 2))
            .add(unsignedBigInt(p1).shl(64))
            .add(unsignedBigInt(p0))
    }

    public fun readU256(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()
        val p2 = readI64()
        val p3 = readI64()

        return unsignedBigInt(p3).shl(64 * 3)
            .add(unsignedBigInt(p2).shl(64 * 2))
            .add(unsignedBigInt(p1).shl(64))
            .add(unsignedBigInt(p0))
    }

    public fun readString(): String {
        val len = readU32()
        check(len <= Int.MAX_VALUE.toUInt()) { "String length $len exceeds maximum supported size" }
        val bytes = readRawBytes(len.toInt())
        return bytes.decodeToString()
    }

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
    public fun readRawBytesView(length: Int): BsatnReader {
        ensure(length)
        val view = BsatnReader(data, offset, offset + length)
        offset += length
        return view
    }

    /**
     * Returns a copy of the underlying buffer between [from] and [to].
     * Used when a materialized ByteArray is needed (e.g. for content-based keying).
     */
    public fun sliceArray(from: Int, to: Int): ByteArray {
        check(from <= to && to <= limit) {
            "sliceArray($from, $to) out of view bounds (limit=$limit)"
        }
        return data.copyOfRange(from, to)
    }

    // Sum type tag byte
    public fun readSumTag(): UByte = readU8()

    // Array length prefix (U32, returned as Int for indexing)
    public fun readArrayLen(): Int {
        val len = readU32()
        check(len <= Int.MAX_VALUE.toUInt()) { "Array length $len exceeds maximum supported size" }
        return len.toInt()
    }
}
