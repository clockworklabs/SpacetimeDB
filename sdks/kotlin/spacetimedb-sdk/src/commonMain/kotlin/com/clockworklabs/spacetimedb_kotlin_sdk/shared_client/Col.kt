package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A typed reference to a table column.
 * Supports all comparison operators (eq, neq, lt, lte, gt, gte).
 *
 * @param TRow the row type this column belongs to
 * @param TValue the Kotlin type of this column's value
 */
public class Col<TRow, TValue> @InternalSpacetimeApi constructor(tableName: String, columnName: String) {
    internal val refSql: String = "${SqlFormat.quoteIdent(tableName)}.${SqlFormat.quoteIdent(columnName)}"

    /** Tests equality against a literal value. */
    public fun eq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${value.sql})")

    /** Tests equality against another column. */
    public fun eq(other: Col<TRow, TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${other.refSql})")

    /** Tests inequality against a literal value. */
    public fun neq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${value.sql})")

    /** Tests inequality against another column. */
    public fun neq(other: Col<TRow, TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${other.refSql})")

    /** Tests whether this column is strictly less than [value]. */
    public fun lt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql < ${value.sql})")

    /** Tests whether this column is less than or equal to [value]. */
    public fun lte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <= ${value.sql})")

    /** Tests whether this column is strictly greater than [value]. */
    public fun gt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql > ${value.sql})")

    /** Tests whether this column is greater than or equal to [value]. */
    public fun gte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql >= ${value.sql})")
}

/**
 * A typed reference to an indexed column.
 * Supports eq/neq comparisons and indexed join equality.
 */
public class IxCol<TRow, TValue> @InternalSpacetimeApi constructor(tableName: String, columnName: String) {
    internal val refSql: String = "${SqlFormat.quoteIdent(tableName)}.${SqlFormat.quoteIdent(columnName)}"

    /** Tests equality against a literal value. */
    public fun eq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${value.sql})")

    /** Creates an indexed join equality condition against another indexed column. */
    @OptIn(InternalSpacetimeApi::class)
    public fun <TOtherRow> eq(other: IxCol<TOtherRow, TValue>): IxJoinEq<TRow, TOtherRow> =
        IxJoinEq(refSql, other.refSql)

    /** Tests inequality against a literal value. */
    public fun neq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${value.sql})")

    /** Tests whether this column is strictly less than [value]. */
    public fun lt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql < ${value.sql})")

    /** Tests whether this column is less than or equal to [value]. */
    public fun lte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <= ${value.sql})")

    /** Tests whether this column is strictly greater than [value]. */
    public fun gt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql > ${value.sql})")

    /** Tests whether this column is greater than or equal to [value]. */
    public fun gte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql >= ${value.sql})")
}

/**
 * Represents an indexed equality join condition between two tables.
 * Created by calling [IxCol.eq] with another indexed column.
 * Used as the `on` parameter for semi-join methods.
 */
public class IxJoinEq<@Suppress("unused") TLeftRow, @Suppress("unused") TRightRow> @InternalSpacetimeApi constructor(
    internal val leftRefSql: String,
    internal val rightRefSql: String,
)
