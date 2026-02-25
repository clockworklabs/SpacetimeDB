@file:Suppress("MemberVisibilityCanBePrivate", "unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn

import java.math.BigInteger

/**
 * Binary reader for BSATN decoding. All multi-byte values are little-endian.
 */
class BsatnReader(private var data: ByteArray, offset: Int = 0, private var limit: Int = data.size) {
    companion object {
        private val UNSIGNED_LONG_MASK = BigInteger.ONE.shiftLeft(64).subtract(BigInteger.ONE)
    }

    var offset: Int = offset
        private set

    val remaining: Int get() = limit - offset

    fun reset(newData: ByteArray) {
        data = newData
        offset = 0
        limit = newData.size
    }

    private fun ensure(n: Int) {
        check(remaining >= n) { "BsatnReader: need $n bytes but only $remaining remain" }
    }

    fun readBool(): Boolean {
        ensure(1)
        val b = data[offset].toInt() and 0xFF
        offset += 1
        return b != 0
    }

    fun readByte(): Byte {
        ensure(1)
        val b = data[offset]
        offset += 1
        return b
    }

    fun readI8(): Byte = readByte()

    fun readU8(): UByte {
        ensure(1)
        val b = data[offset].toUByte()
        offset += 1
        return b
    }

    fun readI16(): Short {
        ensure(2)
        val b0 = data[offset].toInt() and 0xFF
        val b1 = data[offset + 1].toInt() and 0xFF
        offset += 2
        return (b0 or (b1 shl 8)).toShort()
    }

    fun readU16(): UShort = readI16().toUShort()

    fun readI32(): Int {
        ensure(4)
        val b0 = data[offset].toLong() and 0xFF
        val b1 = data[offset + 1].toLong() and 0xFF
        val b2 = data[offset + 2].toLong() and 0xFF
        val b3 = data[offset + 3].toLong() and 0xFF
        offset += 4
        return (b0 or (b1 shl 8) or (b2 shl 16) or (b3 shl 24)).toInt()
    }

    fun readU32(): UInt = readI32().toUInt()

    fun readI64(): Long {
        ensure(8)
        var result = 0L
        for (i in 0 until 8) {
            result = result or ((data[offset + i].toLong() and 0xFF) shl (i * 8))
        }
        offset += 8
        return result
    }

    fun readU64(): ULong = readI64().toULong()

    fun readF32(): Float = Float.fromBits(readI32())

    fun readF64(): Double = Double.fromBits(readI64())

    fun readI128(): BigInteger {
        val p0 = readI64()
        val p1 = readI64() // signed top chunk

        return BigInteger.valueOf(p1).shiftLeft(64 * 1)
            .add(BigInteger.valueOf(p0).and(UNSIGNED_LONG_MASK))
    }

    fun readU128(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()

        return BigInteger.valueOf(p1).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 1)
            .add(BigInteger.valueOf(p0).and(UNSIGNED_LONG_MASK))
    }

    fun readI256(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()
        val p2 = readI64()
        val p3 = readI64() // signed top chunk

        return BigInteger.valueOf(p3).shiftLeft(64 * 3)
            .add(BigInteger.valueOf(p2).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 2))
            .add(BigInteger.valueOf(p1).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 1))
            .add(BigInteger.valueOf(p0).and(UNSIGNED_LONG_MASK))
    }

    fun readU256(): BigInteger {
        val p0 = readI64()
        val p1 = readI64()
        val p2 = readI64()
        val p3 = readI64()

        return BigInteger.valueOf(p3).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 3)
            .add(BigInteger.valueOf(p2).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 2))
            .add(BigInteger.valueOf(p1).and(UNSIGNED_LONG_MASK).shiftLeft(64 * 1))
            .add(BigInteger.valueOf(p0).and(UNSIGNED_LONG_MASK))
    }

    fun readString(): String {
        val len = readU32().toInt()
        check(len >= 0) { "Negative string length: $len" }
        val bytes = readRawBytes(len)
        return bytes.decodeToString()
    }

    fun readByteArray(): ByteArray {
        val len = readU32().toInt()
        check(len >= 0) { "Negative byte array length: $len" }
        return readRawBytes(len)
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
    fun readRawBytesView(length: Int): BsatnReader {
        ensure(length)
        val view = BsatnReader(data, offset, offset + length)
        offset += length
        return view
    }

    /**
     * Returns a copy of the underlying buffer between [from] and [to].
     * Used when a materialized ByteArray is needed (e.g. for content-based keying).
     */
    fun sliceArray(from: Int, to: Int): ByteArray = data.copyOfRange(from, to)

    // Sum type tag byte
    fun readSumTag(): UByte = readU8()

    // Array length prefix (U32, returned as Int for indexing)
    fun readArrayLen(): Int {
        val len = readI32()
        check(len >= 0) { "Negative array length: $len" }
        return len
    }
}