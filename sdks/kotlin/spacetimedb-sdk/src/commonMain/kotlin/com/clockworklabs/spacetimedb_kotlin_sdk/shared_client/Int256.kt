package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.jvm.JvmInline

/** A signed 256-bit integer, backed by [BigInteger]. */
@JvmInline
public value class Int256(public val value: BigInteger) : Comparable<Int256> {
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeI256(value)
    override fun compareTo(other: Int256): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        /** Decodes an [Int256] from BSATN. */
        public fun decode(reader: BsatnReader): Int256 = Int256(reader.readI256())
        /** A zero-valued [Int256]. */
        public val ZERO: Int256 = Int256(BigInteger.ZERO)
    }
}
