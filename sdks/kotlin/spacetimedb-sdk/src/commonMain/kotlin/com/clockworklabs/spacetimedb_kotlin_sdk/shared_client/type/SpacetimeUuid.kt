package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toEpochMicroseconds
import com.ionspin.kotlin.bignum.integer.BigInteger
import com.ionspin.kotlin.bignum.integer.Sign
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.getAndUpdate
import kotlin.time.Instant
import kotlin.uuid.Uuid

public class Counter(value: Int = 0) {
    private val _value = atomic(value)
    internal fun getAndIncrement(): Int =
        _value.getAndUpdate { (it + 1) and 0x7FFF_FFFF }
}

public enum class UuidVersion { Nil, V4, V7, Max, Unknown }

public data class SpacetimeUuid(val data: Uuid) : Comparable<SpacetimeUuid> {
    override fun compareTo(other: SpacetimeUuid): Int {
        val a = data.toByteArray()
        val b = other.data.toByteArray()
        for (i in a.indices) {
            val cmp = (a[i].toInt() and 0xFF).compareTo(b[i].toInt() and 0xFF)
            if (cmp != 0) return cmp
        }
        return 0
    }
    public fun encode(writer: BsatnWriter) {
        val value = BigInteger.fromByteArray(data.toByteArray(), Sign.POSITIVE)
        writer.writeU128(value)
    }

    override fun toString(): String = data.toString()

    public fun toHexString(): String = data.toHexString()

    public fun toByteArray(): ByteArray = data.toByteArray()

    public fun getCounter(): Int {
        val b = data.toByteArray()
        return ((b[7].toInt() and 0xFF) shl 23) or
            ((b[9].toInt() and 0xFF) shl 15) or
            ((b[10].toInt() and 0xFF) shl 7) or
            ((b[11].toInt() and 0xFF) shr 1)
    }

    public fun getVersion(): UuidVersion {
        if (data == Uuid.NIL) return UuidVersion.Nil
        val bytes = data.toByteArray()
        if (bytes.all { it == 0xFF.toByte() }) return UuidVersion.Max
        return when ((bytes[6].toInt() shr 4) and 0x0F) {
            4 -> UuidVersion.V4
            7 -> UuidVersion.V7
            else -> UuidVersion.Unknown
        }
    }

    public companion object {
        public val NIL: SpacetimeUuid = SpacetimeUuid(Uuid.NIL)
        public val MAX: SpacetimeUuid = SpacetimeUuid(Uuid.fromByteArray(ByteArray(16) { 0xFF.toByte() }))

        public fun decode(reader: BsatnReader): SpacetimeUuid {
            val value = reader.readU128()
            val bytes = value.toByteArray()
            val padded = if (bytes.size >= 16) bytes.copyOfRange(bytes.size - 16, bytes.size)
            else ByteArray(16 - bytes.size) + bytes
            return SpacetimeUuid(Uuid.fromByteArray(padded))
        }

        public fun random(): SpacetimeUuid = SpacetimeUuid(Uuid.random())

        public fun fromRandomBytesV4(bytes: ByteArray): SpacetimeUuid {
            require(bytes.size == 16) { "UUID v4 requires exactly 16 bytes, got ${bytes.size}" }
            val b = bytes.copyOf()
            b[6] = ((b[6].toInt() and 0x0F) or 0x40).toByte() // version 4
            b[8] = ((b[8].toInt() and 0x3F) or 0x80).toByte() // variant RFC 4122
            return SpacetimeUuid(Uuid.fromByteArray(b))
        }

        public fun fromCounterV7(counter: Counter, now: Timestamp, randomBytes: ByteArray): SpacetimeUuid {
            require(randomBytes.size >= 4) { "V7 UUID requires at least 4 random bytes, got ${randomBytes.size}" }
            val counterVal = counter.getAndIncrement()

            val tsMs = now.instant.toEpochMicroseconds() / 1_000

            val b = ByteArray(16)
            // Bytes 0-5: 48-bit unix timestamp (ms), big-endian
            b[0] = (tsMs shr 40).toByte()
            b[1] = (tsMs shr 32).toByte()
            b[2] = (tsMs shr 24).toByte()
            b[3] = (tsMs shr 16).toByte()
            b[4] = (tsMs shr 8).toByte()
            b[5] = tsMs.toByte()
            // Byte 6: version 7
            b[6] = 0x70.toByte()
            // Byte 7: counter bits 30-23
            b[7] = ((counterVal shr 23) and 0xFF).toByte()
            // Byte 8: variant RFC 4122
            b[8] = 0x80.toByte()
            // Bytes 9-11: counter bits 22-0
            b[9] = ((counterVal shr 15) and 0xFF).toByte()
            b[10] = ((counterVal shr 7) and 0xFF).toByte()
            b[11] = ((counterVal and 0x7F) shl 1).toByte()
            // Bytes 12-15: random bytes
            b[12] = (randomBytes[0].toInt() and 0x7F).toByte()
            b[13] = randomBytes[1]
            b[14] = randomBytes[2]
            b[15] = randomBytes[3]

            return SpacetimeUuid(Uuid.fromByteArray(b))
        }

        public fun parse(str: String): SpacetimeUuid = SpacetimeUuid(Uuid.parse(str))
    }
}
