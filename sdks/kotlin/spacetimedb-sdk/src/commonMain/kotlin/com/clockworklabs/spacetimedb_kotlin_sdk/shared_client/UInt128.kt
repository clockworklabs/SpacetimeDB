package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.ionspin.kotlin.bignum.integer.BigInteger

@JvmInline
public value class UInt128(public val value: BigInteger) : Comparable<UInt128> {
    public fun encode(writer: BsatnWriter): Unit = writer.writeU128(value)
    override fun compareTo(other: UInt128): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        public fun decode(reader: BsatnReader): UInt128 = UInt128(reader.readU128())
        public val ZERO: UInt128 = UInt128(BigInteger.ZERO)
    }
}
