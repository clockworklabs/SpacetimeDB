package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.ionspin.kotlin.bignum.integer.BigInteger

@JvmInline
public value class UInt256(public val value: BigInteger) : Comparable<UInt256> {
    public fun encode(writer: BsatnWriter): Unit = writer.writeU256(value)
    override fun compareTo(other: UInt256): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        public fun decode(reader: BsatnReader): UInt256 = UInt256(reader.readU256())
        public val ZERO: UInt256 = UInt256(BigInteger.ZERO)
    }
}
