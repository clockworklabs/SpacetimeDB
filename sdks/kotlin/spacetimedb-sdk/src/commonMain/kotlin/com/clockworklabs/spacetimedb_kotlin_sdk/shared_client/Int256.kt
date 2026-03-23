package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.ionspin.kotlin.bignum.integer.BigInteger

@JvmInline
public value class Int256(public val value: BigInteger) : Comparable<Int256> {
    public fun encode(writer: BsatnWriter): Unit = writer.writeI256(value)
    override fun compareTo(other: Int256): Int = value.compareTo(other.value)
    override fun toString(): String = value.toString()

    public companion object {
        public fun decode(reader: BsatnReader): Int256 = Int256(reader.readI256())
        public val ZERO: Int256 = Int256(BigInteger.ZERO)
    }
}
