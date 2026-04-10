package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.jvm.JvmInline

/** An unsigned 256-bit integer, backed by [BigInteger]. */
@JvmInline
public value class UInt256(public val value: BigInteger) : Comparable<UInt256> {
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeU256(value)
    override fun compareTo(other: UInt256): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        /** Decodes a [UInt256] from BSATN. */
        public fun decode(reader: BsatnReader): UInt256 = UInt256(reader.readU256())
        /** A zero-valued [UInt256]. */
        public val ZERO: UInt256 = UInt256(BigInteger.ZERO)
    }
}
