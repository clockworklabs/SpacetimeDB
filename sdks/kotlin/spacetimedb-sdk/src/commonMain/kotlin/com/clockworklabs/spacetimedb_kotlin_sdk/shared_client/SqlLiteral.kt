package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid
import com.ionspin.kotlin.bignum.decimal.BigDecimal

/**
 * A type-safe wrapper around a SQL literal string.
 * The type parameter [T] tracks the Kotlin type at compile time
 * to ensure column comparisons are type-safe.
 */
@JvmInline
public value class SqlLiteral<@Suppress("unused") T>(public val sql: String)

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
        return SqlLiteral(BigDecimal.fromFloat(value).toPlainString())
    }

    public fun double(value: Double): SqlLiteral<Double> {
        require(value.isFinite()) { "SQL literals do not support NaN or Infinity" }
        return SqlLiteral(BigDecimal.fromDouble(value).toPlainString())
    }

    public fun identity(value: Identity): SqlLiteral<Identity> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))

    public fun connectionId(value: ConnectionId): SqlLiteral<ConnectionId> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))

    public fun uuid(value: SpacetimeUuid): SqlLiteral<SpacetimeUuid> =
        SqlLiteral(SqlFormat.formatHexLiteral(value.toHexString()))
}
