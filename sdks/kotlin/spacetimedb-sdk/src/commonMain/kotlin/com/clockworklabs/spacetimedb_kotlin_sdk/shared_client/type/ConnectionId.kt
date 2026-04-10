package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.randomBigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString

/** A 128-bit connection identifier in SpacetimeDB. */
public data class ConnectionId(val data: BigInteger) {
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeU128(data)
    /** Returns this connection ID as a 32-character lowercase hex string. */
    public fun toHexString(): String = data.toHexString(16) // U128 = 16 bytes = 32 hex chars
    override fun toString(): String = toHexString()
    /** Whether this connection ID is all zeros. */
    public fun isZero(): Boolean = data == BigInteger.ZERO
    /**
     * Returns the 16-byte little-endian representation, matching BSATN wire format.
     */
    public fun toByteArray(): ByteArray = data.toLeBytesFixedWidth(16)

    public companion object {
        /** Decodes a [ConnectionId] from BSATN. */
        public fun decode(reader: BsatnReader): ConnectionId = ConnectionId(reader.readU128())
        /** Returns a zero [ConnectionId]. */
        public fun zero(): ConnectionId = ConnectionId(BigInteger.ZERO)
        /** Returns `null` if the given [ConnectionId] is zero, otherwise returns it unchanged. */
        public fun nullIfZero(addr: ConnectionId): ConnectionId? = if (addr.isZero()) null else addr
        /** Returns a random [ConnectionId]. */
        public fun random(): ConnectionId = ConnectionId(randomBigInteger(16)) /* 16 bytes = 128 bits */
        /** Parses a [ConnectionId] from a hex string. */
        public fun fromHexString(hex: String): ConnectionId = ConnectionId(parseHexString(hex))
        /** Parses a [ConnectionId] from a hex string, returning `null` if parsing fails or the result is zero. */
        public fun fromHexStringOrNull(hex: String): ConnectionId? {
            val id = try { fromHexString(hex) } catch (_: Exception) { return null }
            return nullIfZero(id)
        }
    }
}
