package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.jvm.JvmInline

/**
 * A type-safe boolean SQL expression.
 * The type parameter [TRow] tracks which table row type this expression applies to.
 * Constructed via column comparison methods on [Col] and [IxCol].
 */
@JvmInline
public value class BoolExpr<@Suppress("unused") TRow>(public val sql: String) {
    /** Returns a new expression that is the logical AND of this and [other]. */
    public fun and(other: BoolExpr<TRow>): BoolExpr<TRow> = BoolExpr("($sql AND ${other.sql})")

    /** Returns a new expression that is the logical OR of this and [other]. */
    public fun or(other: BoolExpr<TRow>): BoolExpr<TRow> = BoolExpr("($sql OR ${other.sql})")

    /** Returns the logical negation of this expression. */
    public fun not(): BoolExpr<TRow> = BoolExpr("(NOT $sql)")
}
