package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString

/** A 256-bit identity that uniquely identifies a user in SpacetimeDB. */
public data class Identity(val data: BigInteger) : Comparable<Identity> {
    override fun compareTo(other: Identity): Int = data.compareTo(other.data)
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeU256(data)
    /** Returns this identity as a 64-character lowercase hex string. */
    public fun toHexString(): String = data.toHexString(32) // U256 = 32 bytes = 64 hex chars
    /**
     * Returns the 32-byte little-endian representation, matching BSATN wire format.
     */
    public fun toByteArray(): ByteArray = data.toLeBytesFixedWidth(32)
    override fun toString(): String = toHexString()

    public companion object {
        /** Decodes an [Identity] from BSATN. */
        public fun decode(reader: BsatnReader): Identity = Identity(reader.readU256())
        /** Parses an [Identity] from a hex string. */
        public fun fromHexString(hex: String): Identity = Identity(parseHexString(hex))
        /** Returns a zero [Identity]. */
        public fun zero(): Identity = Identity(BigInteger.ZERO)
    }
}
