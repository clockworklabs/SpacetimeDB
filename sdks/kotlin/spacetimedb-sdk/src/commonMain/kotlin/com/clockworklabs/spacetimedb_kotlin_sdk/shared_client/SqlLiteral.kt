package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid
import kotlin.jvm.JvmInline

/**
 * A type-safe wrapper around a SQL literal string.
 * The type parameter [T] tracks the Kotlin type at compile time
 * to ensure column comparisons are type-safe.
 */
@JvmInline
public value class SqlLiteral<@Suppress("unused") T> @InternalSpacetimeApi constructor(@property:InternalSpacetimeApi public val sql: String)

/**
 * Factory for creating [SqlLiteral] values from Kotlin types.
 *
 * Each method converts a native Kotlin value into its SQL literal representation.
 */
public object SqlLit {
    public fun string(value: String): SqlLiteral<String> =
        SqlLiteral(SqlFormat.formatStringLiteral(value))

    public fun bool(value: Boolean): SqlLiteral<Boolean> =
        SqlLiteral(if (value) "TRUE" else "FALSE")

    public fun byte(value: Byte): SqlLiteral<Byte> = SqlLiteral(value.toString())
    public fun ubyte(value: UByte): SqlLiteral<UByte> = SqlLiteral(value.toString())
    public fun short(value: Short): SqlLiteral<Short> = SqlLiteral(value.toString())
    public fun ushort(value: UShort): SqlLiteral<UShort> = SqlLiteral(value.toString())
    public fun int(value: Int): SqlLiteral<Int> = SqlLiteral(value.toString())
    public fun uint(value: UInt): SqlLiteral<UInt> = SqlLiteral(value.toString())
    public fun long(value: Long): SqlLiteral<Long> = SqlLiteral(value.toString())
    public fun ulong(value: ULong): SqlLiteral<ULong> = SqlLiteral(value.toString())
    public fun float(value: Float): SqlLiteral<Float> {
        require(value.isFinite()) { "SQL literals do not support NaN or Infinity" }
        return SqlLiteral(value.toPlainDecimalString())
    }

    public fun double(value: Double): SqlLiteral<Double> {
        require(value.isFinite()) { "SQL literals do not support NaN or Infinity" }
        return SqlLiteral(value.toPlainDecimalString())
    }

    public fun int128(value: Int128): SqlLiteral<Int128> = SqlLiteral(value.value.toString())
    public fun uint128(value: UInt128): SqlLiteral<UInt128> = SqlLiteral(value.value.toString())
    public fun int256(value: Int256): SqlLiteral<Int256> = SqlLiteral(value.value.toString())
    public fun uint256(value: UInt256): SqlLiteral<UInt256> = SqlLiteral(value.value.toString())

    public fun identity(value: Identity): SqlLiteral<Identity> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))

    public fun connectionId(value: ConnectionId): SqlLiteral<ConnectionId> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))

    public fun uuid(value: SpacetimeUuid): SqlLiteral<SpacetimeUuid> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))
}

/**
 * Formats a Float as a plain decimal string without scientific notation.
 * Uses Float.toString() to preserve original float precision (avoids float→double expansion).
 */
private fun Float.toPlainDecimalString(): String {
    val s = this.toString()
    if ('E' !in s && 'e' !in s) return s
    return expandScientificNotation(s)
}

/**
 * Formats a Double as a plain decimal string without scientific notation.
 * Handles the E/e notation that Double.toString() may produce for very large or small values.
 */
private fun Double.toPlainDecimalString(): String {
    val s = this.toString()
    if ('E' !in s && 'e' !in s) return s
    return expandScientificNotation(s)
}

/** Expands a scientific notation string (e.g. "1.5E-7") to plain decimal (e.g. "0.00000015"). */
private fun expandScientificNotation(s: String): String {
    val eIdx = s.indexOfFirst { it == 'E' || it == 'e' }
    val mantissa = s.substring(0, eIdx)
    val exponent = s.substring(eIdx + 1).toInt()

    val negative = mantissa.startsWith('-')
    val absMantissa = if (negative) mantissa.substring(1) else mantissa
    val dotIdx = absMantissa.indexOf('.')
    val intPart = if (dotIdx >= 0) absMantissa.substring(0, dotIdx) else absMantissa
    val fracPart = if (dotIdx >= 0) absMantissa.substring(dotIdx + 1) else ""
    val allDigits = intPart + fracPart
    val newDecimalPos = intPart.length + exponent

    val sb = StringBuilder()
    if (negative) sb.append('-')

    when {
        newDecimalPos <= 0 -> {
            sb.append("0.")
            repeat(-newDecimalPos) { sb.append('0') }
            sb.append(allDigits)
        }
        newDecimalPos >= allDigits.length -> {
            sb.append(allDigits)
            repeat(newDecimalPos - allDigits.length) { sb.append('0') }
            sb.append(".0")
        }
        else -> {
            sb.append(allDigits, 0, newDecimalPos)
            sb.append('.')
            sb.append(allDigits, newDecimalPos, allDigits.length)
        }
    }

    return sb.toString()
}
