package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter

private val HEX_CHARS = "0123456789abcdef".toCharArray()

internal fun ByteArray.toHexString(): String {
    val result = CharArray(size * 2)
    for (i in indices) {
        val v = this[i].toInt() and 0xFF
        result[i * 2] = HEX_CHARS[v ushr 4]
        result[i * 2 + 1] = HEX_CHARS[v and 0x0F]
    }
    return result.concatToString()
}

internal fun String.hexToByteArray(): ByteArray {
    require(length % 2 == 0) { "Hex string must have even length" }
    return ByteArray(length / 2) { i ->
        val hi = this[i * 2].digitToInt(16)
        val lo = this[i * 2 + 1].digitToInt(16)
        ((hi shl 4) or lo).toByte()
    }
}

/** A 256-bit identifier that uniquely represents a user across all SpacetimeDB modules. */
class Identity(val bytes: ByteArray) {
    init {
        require(bytes.size == 32) { "Identity must be 32 bytes" }
    }

    fun toHex(): String = bytes.toHexString()

    override fun equals(other: Any?): Boolean =
        other is Identity && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = bytes.contentHashCode()

    override fun toString(): String = "Identity(${toHex()})"

    companion object {
        val ZERO = Identity(ByteArray(32))

        fun fromHex(hex: String): Identity {
            require(hex.length == 64) { "Identity hex must be 64 characters" }
            val bytes = hex.hexToByteArray()
            return Identity(bytes)
        }

        fun read(reader: BsatnReader): Identity = Identity(reader.readBytes(32))

        fun write(writer: BsatnWriter, value: Identity) { writer.writeBytes(value.bytes) }
    }
}

/** A 128-bit identifier unique to each client connection session. */
class ConnectionId(val bytes: ByteArray) {
    init {
        require(bytes.size == 16) { "ConnectionId must be 16 bytes" }
    }

    fun toHex(): String = bytes.toHexString()

    override fun equals(other: Any?): Boolean =
        other is ConnectionId && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = bytes.contentHashCode()

    override fun toString(): String = "ConnectionId(${toHex()})"

    companion object {
        val ZERO = ConnectionId(ByteArray(16))

        fun read(reader: BsatnReader): ConnectionId = ConnectionId(reader.readBytes(16))

        fun write(writer: BsatnWriter, value: ConnectionId) { writer.writeBytes(value.bytes) }
    }
}

/** A 128-bit address identifying a client in the SpacetimeDB network. */
class Address(val bytes: ByteArray) {
    init {
        require(bytes.size == 16) { "Address must be 16 bytes" }
    }

    fun toHex(): String = bytes.toHexString()

    override fun equals(other: Any?): Boolean =
        other is Address && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = bytes.contentHashCode()

    override fun toString(): String = "Address(${toHex()})"

    companion object {
        val ZERO = Address(ByteArray(16))

        fun read(reader: BsatnReader): Address = Address(reader.readBytes(16))

        fun write(writer: BsatnWriter, value: Address) { writer.writeBytes(value.bytes) }
    }
}

/** Server-side timestamp in microseconds since the Unix epoch. */
@kotlin.jvm.JvmInline
value class Timestamp(val microseconds: Long) {
    companion object {
        fun read(reader: BsatnReader): Timestamp = Timestamp(reader.readI64())

        fun write(writer: BsatnWriter, value: Timestamp) { writer.writeI64(value.microseconds) }
    }
}
