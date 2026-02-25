package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.toEpochMicroseconds
import java.math.BigInteger
import kotlin.time.Instant
import kotlin.uuid.Uuid

class Counter(var value: Int = 0)

enum class UuidVersion { Nil, V4, V7, Max, Unknown }

data class SpacetimeUuid(val data: Uuid) : Comparable<SpacetimeUuid> {
    override fun compareTo(other: SpacetimeUuid): Int {
        val a = data.toByteArray()
        val b = other.data.toByteArray()
        for (i in a.indices) {
            val cmp = (a[i].toInt() and 0xFF).compareTo(b[i].toInt() and 0xFF)
            if (cmp != 0) return cmp
        }
        return 0
    }
    fun encode(writer: BsatnWriter) {
        val value = BigInteger(1, data.toByteArray())
        writer.writeU128(value)
    }

    override fun toString(): String = data.toString()

    fun toHexString(): String = data.toHexString()

    fun toByteArray(): ByteArray = data.toByteArray()

    fun getCounter(): Int {
        val b = data.toByteArray()
        return ((b[7].toInt() and 0xFF) shl 23) or
            ((b[9].toInt() and 0xFF) shl 15) or
            ((b[10].toInt() and 0xFF) shl 7) or
            ((b[11].toInt() and 0xFF) shr 1)
    }

    fun getVersion(): UuidVersion {
        if (data == Uuid.NIL) return UuidVersion.Nil
        val bytes = data.toByteArray()
        if (bytes.all { it == 0xFF.toByte() }) return UuidVersion.Max
        return when ((bytes[6].toInt() shr 4) and 0x0F) {
            4 -> UuidVersion.V4
            7 -> UuidVersion.V7
            else -> UuidVersion.Unknown
        }
    }

    companion object {
        val NIL = SpacetimeUuid(Uuid.NIL)
        val MAX = SpacetimeUuid(Uuid.fromByteArray(ByteArray(16) { 0xFF.toByte() }))

        fun decode(reader: BsatnReader): SpacetimeUuid {
            val value = reader.readU128()
            val bytes = value.toByteArray()
            val unsigned = if (bytes.size > 1 && bytes[0] == 0.toByte()) bytes.copyOfRange(1, bytes.size) else bytes
            val padded = ByteArray(16 - unsigned.size) + unsigned
            return SpacetimeUuid(Uuid.fromByteArray(padded))
        }

        fun random(): SpacetimeUuid = SpacetimeUuid(Uuid.random())

        fun fromRandomBytesV4(bytes: ByteArray): SpacetimeUuid {
            require(bytes.size == 16) { "UUID v4 requires exactly 16 bytes, got ${bytes.size}" }
            val b = bytes.copyOf()
            b[6] = ((b[6].toInt() and 0x0F) or 0x40).toByte() // version 4
            b[8] = ((b[8].toInt() and 0x3F) or 0x80).toByte() // variant RFC 4122
            return SpacetimeUuid(Uuid.fromByteArray(b))
        }

        fun fromCounterV7(counter: Counter, now: Timestamp, randomBytes: ByteArray): SpacetimeUuid {
            require(randomBytes.size >= 4) { "V7 UUID requires at least 4 random bytes, got ${randomBytes.size}" }
            val counterVal = counter.value
            counter.value = (counterVal + 1) and 0x7FFF_FFFF

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
            b[12] = (b[12].toInt() or (randomBytes[0].toInt() and 0x7F)).toByte()
            b[13] = randomBytes[1]
            b[14] = randomBytes[2]
            b[15] = randomBytes[3]

            return SpacetimeUuid(Uuid.fromByteArray(b))
        }

        fun parse(str: String): SpacetimeUuid = SpacetimeUuid(Uuid.parse(str))
    }
}
