package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.randomBigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString
import com.ionspin.kotlin.bignum.integer.BigInteger

public data class ConnectionId(val data: BigInteger) {
    public fun encode(writer: BsatnWriter): Unit = writer.writeU128(data)
    public fun toHexString(): String = data.toHexString(16) // U128 = 16 bytes = 32 hex chars
    override fun toString(): String = toHexString()
    public fun isZero(): Boolean = data == BigInteger.ZERO
    /**
     * Returns the 16-byte little-endian representation, matching BSATN wire format.
     */
    public fun toByteArray(): ByteArray {
        val beBytes = data.toByteArray()
        val padded = ByteArray(16)
        val srcStart = maxOf(0, beBytes.size - 16)
        val dstStart = maxOf(0, 16 - beBytes.size)
        beBytes.copyInto(padded, dstStart, srcStart, beBytes.size)
        padded.reverse()
        return padded
    }

    public companion object {
        public fun decode(reader: BsatnReader): ConnectionId = ConnectionId(reader.readU128())
        public fun zero(): ConnectionId = ConnectionId(BigInteger.ZERO)
        public fun nullIfZero(addr: ConnectionId): ConnectionId? = if (addr.isZero()) null else addr
        public fun random(): ConnectionId = ConnectionId(randomBigInteger(16)) /* 16 bytes = 128 bits */
        public fun fromHexString(hex: String): ConnectionId = ConnectionId(parseHexString(hex))
        public fun fromHexStringOrNull(hex: String): ConnectionId? {
            val id = try { fromHexString(hex) } catch (_: Exception) { return null }
            return nullIfZero(id)
        }
    }
}
