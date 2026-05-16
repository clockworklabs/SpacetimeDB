@file:Suppress("TooManyFunctions")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid

/**
 * Type-specialized comparison extensions for [Col] and [IxCol].
 *
 * Each overload accepts a native Kotlin value, converts it to a [SqlLiteral] via [SqlLit],
 * and delegates to the underlying column comparison method. This avoids requiring callers
 * to wrap every value in [SqlLit] manually.
 */

// ---- Col<TRow, String> ----

public fun <TRow> Col<TRow, String>.eq(value: String): BoolExpr<TRow> = eq(SqlLit.string(value))
public fun <TRow> Col<TRow, String>.neq(value: String): BoolExpr<TRow> = neq(SqlLit.string(value))
public fun <TRow> Col<TRow, String>.lt(value: String): BoolExpr<TRow> = lt(SqlLit.string(value))
public fun <TRow> Col<TRow, String>.lte(value: String): BoolExpr<TRow> = lte(SqlLit.string(value))
public fun <TRow> Col<TRow, String>.gt(value: String): BoolExpr<TRow> = gt(SqlLit.string(value))
public fun <TRow> Col<TRow, String>.gte(value: String): BoolExpr<TRow> = gte(SqlLit.string(value))

public fun <TRow> IxCol<TRow, String>.eq(value: String): BoolExpr<TRow> = eq(SqlLit.string(value))
public fun <TRow> IxCol<TRow, String>.neq(value: String): BoolExpr<TRow> = neq(SqlLit.string(value))

// ---- Col<TRow, Boolean> ----

public fun <TRow> Col<TRow, Boolean>.eq(value: Boolean): BoolExpr<TRow> = eq(SqlLit.bool(value))
public fun <TRow> Col<TRow, Boolean>.neq(value: Boolean): BoolExpr<TRow> = neq(SqlLit.bool(value))
public operator fun <TRow> Col<TRow, Boolean>.not(): BoolExpr<TRow> = eq(SqlLit.bool(true)).not()

public fun <TRow> IxCol<TRow, Boolean>.eq(value: Boolean): BoolExpr<TRow> = eq(SqlLit.bool(value))
public fun <TRow> IxCol<TRow, Boolean>.neq(value: Boolean): BoolExpr<TRow> = neq(SqlLit.bool(value))
public operator fun <TRow> IxCol<TRow, Boolean>.not(): BoolExpr<TRow> = eq(SqlLit.bool(true)).not()

// ---- Col<TRow, Int> ----

public fun <TRow> Col<TRow, Int>.eq(value: Int): BoolExpr<TRow> = eq(SqlLit.int(value))
public fun <TRow> Col<TRow, Int>.neq(value: Int): BoolExpr<TRow> = neq(SqlLit.int(value))
public fun <TRow> Col<TRow, Int>.lt(value: Int): BoolExpr<TRow> = lt(SqlLit.int(value))
public fun <TRow> Col<TRow, Int>.lte(value: Int): BoolExpr<TRow> = lte(SqlLit.int(value))
public fun <TRow> Col<TRow, Int>.gt(value: Int): BoolExpr<TRow> = gt(SqlLit.int(value))
public fun <TRow> Col<TRow, Int>.gte(value: Int): BoolExpr<TRow> = gte(SqlLit.int(value))

public fun <TRow> IxCol<TRow, Int>.eq(value: Int): BoolExpr<TRow> = eq(SqlLit.int(value))
public fun <TRow> IxCol<TRow, Int>.neq(value: Int): BoolExpr<TRow> = neq(SqlLit.int(value))

// ---- Col<TRow, Long> ----

public fun <TRow> Col<TRow, Long>.eq(value: Long): BoolExpr<TRow> = eq(SqlLit.long(value))
public fun <TRow> Col<TRow, Long>.neq(value: Long): BoolExpr<TRow> = neq(SqlLit.long(value))
public fun <TRow> Col<TRow, Long>.lt(value: Long): BoolExpr<TRow> = lt(SqlLit.long(value))
public fun <TRow> Col<TRow, Long>.lte(value: Long): BoolExpr<TRow> = lte(SqlLit.long(value))
public fun <TRow> Col<TRow, Long>.gt(value: Long): BoolExpr<TRow> = gt(SqlLit.long(value))
public fun <TRow> Col<TRow, Long>.gte(value: Long): BoolExpr<TRow> = gte(SqlLit.long(value))

public fun <TRow> IxCol<TRow, Long>.eq(value: Long): BoolExpr<TRow> = eq(SqlLit.long(value))
public fun <TRow> IxCol<TRow, Long>.neq(value: Long): BoolExpr<TRow> = neq(SqlLit.long(value))

// ---- Col<TRow, Byte/Short/UByte/UShort/UInt/ULong/Float/Double> ----

public fun <TRow> Col<TRow, Byte>.eq(value: Byte): BoolExpr<TRow> = eq(SqlLit.byte(value))
public fun <TRow> Col<TRow, Byte>.neq(value: Byte): BoolExpr<TRow> = neq(SqlLit.byte(value))
public fun <TRow> Col<TRow, Byte>.lt(value: Byte): BoolExpr<TRow> = lt(SqlLit.byte(value))
public fun <TRow> Col<TRow, Byte>.lte(value: Byte): BoolExpr<TRow> = lte(SqlLit.byte(value))
public fun <TRow> Col<TRow, Byte>.gt(value: Byte): BoolExpr<TRow> = gt(SqlLit.byte(value))
public fun <TRow> Col<TRow, Byte>.gte(value: Byte): BoolExpr<TRow> = gte(SqlLit.byte(value))

public fun <TRow> Col<TRow, Short>.eq(value: Short): BoolExpr<TRow> = eq(SqlLit.short(value))
public fun <TRow> Col<TRow, Short>.neq(value: Short): BoolExpr<TRow> = neq(SqlLit.short(value))
public fun <TRow> Col<TRow, Short>.lt(value: Short): BoolExpr<TRow> = lt(SqlLit.short(value))
public fun <TRow> Col<TRow, Short>.lte(value: Short): BoolExpr<TRow> = lte(SqlLit.short(value))
public fun <TRow> Col<TRow, Short>.gt(value: Short): BoolExpr<TRow> = gt(SqlLit.short(value))
public fun <TRow> Col<TRow, Short>.gte(value: Short): BoolExpr<TRow> = gte(SqlLit.short(value))

public fun <TRow> Col<TRow, UByte>.eq(value: UByte): BoolExpr<TRow> = eq(SqlLit.ubyte(value))
public fun <TRow> Col<TRow, UByte>.neq(value: UByte): BoolExpr<TRow> = neq(SqlLit.ubyte(value))
public fun <TRow> Col<TRow, UByte>.lt(value: UByte): BoolExpr<TRow> = lt(SqlLit.ubyte(value))
public fun <TRow> Col<TRow, UByte>.lte(value: UByte): BoolExpr<TRow> = lte(SqlLit.ubyte(value))
public fun <TRow> Col<TRow, UByte>.gt(value: UByte): BoolExpr<TRow> = gt(SqlLit.ubyte(value))
public fun <TRow> Col<TRow, UByte>.gte(value: UByte): BoolExpr<TRow> = gte(SqlLit.ubyte(value))

public fun <TRow> Col<TRow, UShort>.eq(value: UShort): BoolExpr<TRow> = eq(SqlLit.ushort(value))
public fun <TRow> Col<TRow, UShort>.neq(value: UShort): BoolExpr<TRow> = neq(SqlLit.ushort(value))
public fun <TRow> Col<TRow, UShort>.lt(value: UShort): BoolExpr<TRow> = lt(SqlLit.ushort(value))
public fun <TRow> Col<TRow, UShort>.lte(value: UShort): BoolExpr<TRow> = lte(SqlLit.ushort(value))
public fun <TRow> Col<TRow, UShort>.gt(value: UShort): BoolExpr<TRow> = gt(SqlLit.ushort(value))
public fun <TRow> Col<TRow, UShort>.gte(value: UShort): BoolExpr<TRow> = gte(SqlLit.ushort(value))

public fun <TRow> Col<TRow, UInt>.eq(value: UInt): BoolExpr<TRow> = eq(SqlLit.uint(value))
public fun <TRow> Col<TRow, UInt>.neq(value: UInt): BoolExpr<TRow> = neq(SqlLit.uint(value))
public fun <TRow> Col<TRow, UInt>.lt(value: UInt): BoolExpr<TRow> = lt(SqlLit.uint(value))
public fun <TRow> Col<TRow, UInt>.lte(value: UInt): BoolExpr<TRow> = lte(SqlLit.uint(value))
public fun <TRow> Col<TRow, UInt>.gt(value: UInt): BoolExpr<TRow> = gt(SqlLit.uint(value))
public fun <TRow> Col<TRow, UInt>.gte(value: UInt): BoolExpr<TRow> = gte(SqlLit.uint(value))

public fun <TRow> Col<TRow, ULong>.eq(value: ULong): BoolExpr<TRow> = eq(SqlLit.ulong(value))
public fun <TRow> Col<TRow, ULong>.neq(value: ULong): BoolExpr<TRow> = neq(SqlLit.ulong(value))
public fun <TRow> Col<TRow, ULong>.lt(value: ULong): BoolExpr<TRow> = lt(SqlLit.ulong(value))
public fun <TRow> Col<TRow, ULong>.lte(value: ULong): BoolExpr<TRow> = lte(SqlLit.ulong(value))
public fun <TRow> Col<TRow, ULong>.gt(value: ULong): BoolExpr<TRow> = gt(SqlLit.ulong(value))
public fun <TRow> Col<TRow, ULong>.gte(value: ULong): BoolExpr<TRow> = gte(SqlLit.ulong(value))

public fun <TRow> Col<TRow, Float>.eq(value: Float): BoolExpr<TRow> = eq(SqlLit.float(value))
public fun <TRow> Col<TRow, Float>.neq(value: Float): BoolExpr<TRow> = neq(SqlLit.float(value))
public fun <TRow> Col<TRow, Float>.lt(value: Float): BoolExpr<TRow> = lt(SqlLit.float(value))
public fun <TRow> Col<TRow, Float>.lte(value: Float): BoolExpr<TRow> = lte(SqlLit.float(value))
public fun <TRow> Col<TRow, Float>.gt(value: Float): BoolExpr<TRow> = gt(SqlLit.float(value))
public fun <TRow> Col<TRow, Float>.gte(value: Float): BoolExpr<TRow> = gte(SqlLit.float(value))

public fun <TRow> Col<TRow, Double>.eq(value: Double): BoolExpr<TRow> = eq(SqlLit.double(value))
public fun <TRow> Col<TRow, Double>.neq(value: Double): BoolExpr<TRow> = neq(SqlLit.double(value))
public fun <TRow> Col<TRow, Double>.lt(value: Double): BoolExpr<TRow> = lt(SqlLit.double(value))
public fun <TRow> Col<TRow, Double>.lte(value: Double): BoolExpr<TRow> = lte(SqlLit.double(value))
public fun <TRow> Col<TRow, Double>.gt(value: Double): BoolExpr<TRow> = gt(SqlLit.double(value))
public fun <TRow> Col<TRow, Double>.gte(value: Double): BoolExpr<TRow> = gte(SqlLit.double(value))

public fun <TRow> IxCol<TRow, Byte>.eq(value: Byte): BoolExpr<TRow> = eq(SqlLit.byte(value))
public fun <TRow> IxCol<TRow, Byte>.neq(value: Byte): BoolExpr<TRow> = neq(SqlLit.byte(value))

public fun <TRow> IxCol<TRow, Short>.eq(value: Short): BoolExpr<TRow> = eq(SqlLit.short(value))
public fun <TRow> IxCol<TRow, Short>.neq(value: Short): BoolExpr<TRow> = neq(SqlLit.short(value))

public fun <TRow> IxCol<TRow, UByte>.eq(value: UByte): BoolExpr<TRow> = eq(SqlLit.ubyte(value))
public fun <TRow> IxCol<TRow, UByte>.neq(value: UByte): BoolExpr<TRow> = neq(SqlLit.ubyte(value))

public fun <TRow> IxCol<TRow, UShort>.eq(value: UShort): BoolExpr<TRow> = eq(SqlLit.ushort(value))
public fun <TRow> IxCol<TRow, UShort>.neq(value: UShort): BoolExpr<TRow> = neq(SqlLit.ushort(value))

public fun <TRow> IxCol<TRow, UInt>.eq(value: UInt): BoolExpr<TRow> = eq(SqlLit.uint(value))
public fun <TRow> IxCol<TRow, UInt>.neq(value: UInt): BoolExpr<TRow> = neq(SqlLit.uint(value))

public fun <TRow> IxCol<TRow, ULong>.eq(value: ULong): BoolExpr<TRow> = eq(SqlLit.ulong(value))
public fun <TRow> IxCol<TRow, ULong>.neq(value: ULong): BoolExpr<TRow> = neq(SqlLit.ulong(value))

public fun <TRow> IxCol<TRow, Float>.eq(value: Float): BoolExpr<TRow> = eq(SqlLit.float(value))
public fun <TRow> IxCol<TRow, Float>.neq(value: Float): BoolExpr<TRow> = neq(SqlLit.float(value))

public fun <TRow> IxCol<TRow, Double>.eq(value: Double): BoolExpr<TRow> = eq(SqlLit.double(value))
public fun <TRow> IxCol<TRow, Double>.neq(value: Double): BoolExpr<TRow> = neq(SqlLit.double(value))

// ---- Col<TRow, Int128/UInt128/Int256/UInt256> ----

public fun <TRow> Col<TRow, Int128>.eq(value: Int128): BoolExpr<TRow> = eq(SqlLit.int128(value))
public fun <TRow> Col<TRow, Int128>.neq(value: Int128): BoolExpr<TRow> = neq(SqlLit.int128(value))
public fun <TRow> Col<TRow, Int128>.lt(value: Int128): BoolExpr<TRow> = lt(SqlLit.int128(value))
public fun <TRow> Col<TRow, Int128>.lte(value: Int128): BoolExpr<TRow> = lte(SqlLit.int128(value))
public fun <TRow> Col<TRow, Int128>.gt(value: Int128): BoolExpr<TRow> = gt(SqlLit.int128(value))
public fun <TRow> Col<TRow, Int128>.gte(value: Int128): BoolExpr<TRow> = gte(SqlLit.int128(value))

public fun <TRow> Col<TRow, UInt128>.eq(value: UInt128): BoolExpr<TRow> = eq(SqlLit.uint128(value))
public fun <TRow> Col<TRow, UInt128>.neq(value: UInt128): BoolExpr<TRow> = neq(SqlLit.uint128(value))
public fun <TRow> Col<TRow, UInt128>.lt(value: UInt128): BoolExpr<TRow> = lt(SqlLit.uint128(value))
public fun <TRow> Col<TRow, UInt128>.lte(value: UInt128): BoolExpr<TRow> = lte(SqlLit.uint128(value))
public fun <TRow> Col<TRow, UInt128>.gt(value: UInt128): BoolExpr<TRow> = gt(SqlLit.uint128(value))
public fun <TRow> Col<TRow, UInt128>.gte(value: UInt128): BoolExpr<TRow> = gte(SqlLit.uint128(value))

public fun <TRow> Col<TRow, Int256>.eq(value: Int256): BoolExpr<TRow> = eq(SqlLit.int256(value))
public fun <TRow> Col<TRow, Int256>.neq(value: Int256): BoolExpr<TRow> = neq(SqlLit.int256(value))
public fun <TRow> Col<TRow, Int256>.lt(value: Int256): BoolExpr<TRow> = lt(SqlLit.int256(value))
public fun <TRow> Col<TRow, Int256>.lte(value: Int256): BoolExpr<TRow> = lte(SqlLit.int256(value))
public fun <TRow> Col<TRow, Int256>.gt(value: Int256): BoolExpr<TRow> = gt(SqlLit.int256(value))
public fun <TRow> Col<TRow, Int256>.gte(value: Int256): BoolExpr<TRow> = gte(SqlLit.int256(value))

public fun <TRow> Col<TRow, UInt256>.eq(value: UInt256): BoolExpr<TRow> = eq(SqlLit.uint256(value))
public fun <TRow> Col<TRow, UInt256>.neq(value: UInt256): BoolExpr<TRow> = neq(SqlLit.uint256(value))
public fun <TRow> Col<TRow, UInt256>.lt(value: UInt256): BoolExpr<TRow> = lt(SqlLit.uint256(value))
public fun <TRow> Col<TRow, UInt256>.lte(value: UInt256): BoolExpr<TRow> = lte(SqlLit.uint256(value))
public fun <TRow> Col<TRow, UInt256>.gt(value: UInt256): BoolExpr<TRow> = gt(SqlLit.uint256(value))
public fun <TRow> Col<TRow, UInt256>.gte(value: UInt256): BoolExpr<TRow> = gte(SqlLit.uint256(value))

public fun <TRow> IxCol<TRow, Int128>.eq(value: Int128): BoolExpr<TRow> = eq(SqlLit.int128(value))
public fun <TRow> IxCol<TRow, Int128>.neq(value: Int128): BoolExpr<TRow> = neq(SqlLit.int128(value))

public fun <TRow> IxCol<TRow, UInt128>.eq(value: UInt128): BoolExpr<TRow> = eq(SqlLit.uint128(value))
public fun <TRow> IxCol<TRow, UInt128>.neq(value: UInt128): BoolExpr<TRow> = neq(SqlLit.uint128(value))

public fun <TRow> IxCol<TRow, Int256>.eq(value: Int256): BoolExpr<TRow> = eq(SqlLit.int256(value))
public fun <TRow> IxCol<TRow, Int256>.neq(value: Int256): BoolExpr<TRow> = neq(SqlLit.int256(value))

public fun <TRow> IxCol<TRow, UInt256>.eq(value: UInt256): BoolExpr<TRow> = eq(SqlLit.uint256(value))
public fun <TRow> IxCol<TRow, UInt256>.neq(value: UInt256): BoolExpr<TRow> = neq(SqlLit.uint256(value))

// ---- Col<TRow, Identity/ConnectionId/SpacetimeUuid> ----

public fun <TRow> Col<TRow, Identity>.eq(value: Identity): BoolExpr<TRow> = eq(SqlLit.identity(value))
public fun <TRow> Col<TRow, Identity>.neq(value: Identity): BoolExpr<TRow> = neq(SqlLit.identity(value))

public fun <TRow> IxCol<TRow, Identity>.eq(value: Identity): BoolExpr<TRow> = eq(SqlLit.identity(value))
public fun <TRow> IxCol<TRow, Identity>.neq(value: Identity): BoolExpr<TRow> = neq(SqlLit.identity(value))

public fun <TRow> Col<TRow, ConnectionId>.eq(value: ConnectionId): BoolExpr<TRow> = eq(SqlLit.connectionId(value))
public fun <TRow> Col<TRow, ConnectionId>.neq(value: ConnectionId): BoolExpr<TRow> = neq(SqlLit.connectionId(value))

public fun <TRow> IxCol<TRow, ConnectionId>.eq(value: ConnectionId): BoolExpr<TRow> = eq(SqlLit.connectionId(value))
public fun <TRow> IxCol<TRow, ConnectionId>.neq(value: ConnectionId): BoolExpr<TRow> = neq(SqlLit.connectionId(value))

public fun <TRow> Col<TRow, SpacetimeUuid>.eq(value: SpacetimeUuid): BoolExpr<TRow> = eq(SqlLit.uuid(value))
public fun <TRow> Col<TRow, SpacetimeUuid>.neq(value: SpacetimeUuid): BoolExpr<TRow> = neq(SqlLit.uuid(value))

public fun <TRow> IxCol<TRow, SpacetimeUuid>.eq(value: SpacetimeUuid): BoolExpr<TRow> = eq(SqlLit.uuid(value))
public fun <TRow> IxCol<TRow, SpacetimeUuid>.neq(value: SpacetimeUuid): BoolExpr<TRow> = neq(SqlLit.uuid(value))
