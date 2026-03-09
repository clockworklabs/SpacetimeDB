package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A type-safe boolean SQL expression.
 * The type parameter [TRow] tracks which table row type this expression applies to.
 * Constructed via column comparison methods on [Col] and [IxCol].
 */
@JvmInline
public value class BoolExpr<@Suppress("unused") TRow>(public val sql: String) {
    public fun and(other: BoolExpr<TRow>): BoolExpr<TRow> = BoolExpr("($sql AND ${other.sql})")
    public fun or(other: BoolExpr<TRow>): BoolExpr<TRow> = BoolExpr("($sql OR ${other.sql})")
    public fun not(): BoolExpr<TRow> = BoolExpr("(NOT $sql)")
}
