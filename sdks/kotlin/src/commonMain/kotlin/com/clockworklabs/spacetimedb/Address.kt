package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter

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