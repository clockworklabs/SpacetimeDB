package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString
import java.math.BigInteger

data class Identity(val data: BigInteger) : Comparable<Identity> {
    override fun compareTo(other: Identity): Int = data.compareTo(other.data)
    fun encode(writer: BsatnWriter) = writer.writeU256(data)
    fun toHexString(): String = data.toHexString(32) // U256 = 32 bytes = 64 hex chars
    fun toByteArray(): ByteArray {
        val bytes = data.toByteArray()
        val unsigned = if (bytes.size > 1 && bytes[0] == 0.toByte()) bytes.copyOfRange(1, bytes.size) else bytes
        return ByteArray(32 - unsigned.size) + unsigned
    }
    override fun toString(): String = toHexString()

    companion object {
        fun decode(reader: BsatnReader): Identity = Identity(reader.readU256())
        fun fromHexString(hex: String): Identity = Identity(parseHexString(hex))
        fun zero(): Identity = Identity(BigInteger.ZERO)
    }
}
