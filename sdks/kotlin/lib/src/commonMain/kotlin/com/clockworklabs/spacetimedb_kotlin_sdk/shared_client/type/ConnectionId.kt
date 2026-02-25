package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.parseHexString
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.randomBigInteger
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toHexString
import java.math.BigInteger

data class ConnectionId(val data: BigInteger) {
    fun encode(writer: BsatnWriter) = writer.writeU128(data)
    fun toHexString(): String = data.toHexString(16) // U128 = 16 bytes = 32 hex chars
    override fun toString(): String = toHexString()
    fun isZero(): Boolean = data == BigInteger.ZERO
    fun toByteArray(): ByteArray {
        val bytes = data.toByteArray()
        val unsigned = if (bytes.size > 1 && bytes[0] == 0.toByte()) bytes.copyOfRange(1, bytes.size) else bytes
        return ByteArray(16 - unsigned.size) + unsigned
    }

    companion object {
        fun decode(reader: BsatnReader): ConnectionId = ConnectionId(reader.readU128())
        fun zero(): ConnectionId = ConnectionId(BigInteger.ZERO)
        fun nullIfZero(addr: ConnectionId): ConnectionId? = if (addr.isZero()) null else addr
        fun random(): ConnectionId = ConnectionId(randomBigInteger(16)) /* 16 bytes = 128 bits */
        fun fromHexString(hex: String): ConnectionId = ConnectionId(parseHexString(hex))
        fun fromHexStringOrNull(hex: String): ConnectionId? {
            val id = fromHexString(hex)
            return nullIfZero(id)
        }
    }
}