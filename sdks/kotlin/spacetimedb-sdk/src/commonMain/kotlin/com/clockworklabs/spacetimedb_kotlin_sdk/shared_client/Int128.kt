package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import kotlin.jvm.JvmInline

/** A signed 128-bit integer, backed by [BigInteger]. */
@JvmInline
public value class Int128(public val value: BigInteger) : Comparable<Int128> {
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeI128(value)
    override fun compareTo(other: Int128): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        /** Decodes an [Int128] from BSATN. */
        public fun decode(reader: BsatnReader): Int128 = Int128(reader.readI128())
        /** A zero-valued [Int128]. */
        public val ZERO: Int128 = Int128(BigInteger.ZERO)
    }
}
