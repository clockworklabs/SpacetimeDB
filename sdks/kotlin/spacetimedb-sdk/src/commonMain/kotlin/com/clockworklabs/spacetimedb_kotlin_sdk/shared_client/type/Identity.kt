package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString
import com.ionspin.kotlin.bignum.integer.BigInteger

public data class Identity(val data: BigInteger) : Comparable<Identity> {
    override fun compareTo(other: Identity): Int = data.compareTo(other.data)
    public fun encode(writer: BsatnWriter): Unit = writer.writeU256(data)
    public fun toHexString(): String = data.toHexString(32) // U256 = 32 bytes = 64 hex chars
    /**
     * Returns the 32-byte little-endian representation, matching BSATN wire format.
     */
    public fun toByteArray(): ByteArray {
        val beBytes = data.toByteArray()
        require(beBytes.size <= 32) {
            "Identity value too large: ${beBytes.size} bytes exceeds U256 (32 bytes)"
        }
        val padded = ByteArray(32)
        val dstStart = 32 - beBytes.size
        beBytes.copyInto(padded, dstStart)
        padded.reverse()
        return padded
    }
    override fun toString(): String = toHexString()

    public companion object {
        public fun decode(reader: BsatnReader): Identity = Identity(reader.readU256())
        public fun fromHexString(hex: String): Identity = Identity(parseHexString(hex))
        public fun zero(): Identity = Identity(BigInteger.ZERO)
    }
}
