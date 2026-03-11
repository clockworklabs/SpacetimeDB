package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A typed reference to a table column.
 * Supports all comparison operators (eq, neq, lt, lte, gt, gte).
 *
 * @param TRow the row type this column belongs to
 * @param TValue the Kotlin type of this column's value
 */
public class Col<TRow, TValue>(tableName: String, columnName: String) {
    public val refSql: String = "${SqlFormat.quoteIdent(tableName)}.${SqlFormat.quoteIdent(columnName)}"

    public fun eq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${value.sql})")
    public fun eq(other: Col<TRow, TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${other.refSql})")
    public fun neq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${value.sql})")
    public fun neq(other: Col<TRow, TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${other.refSql})")
    public fun lt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql < ${value.sql})")
    public fun lte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <= ${value.sql})")
    public fun gt(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql > ${value.sql})")
    public fun gte(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql >= ${value.sql})")
}

/**
 * A typed reference to an indexed column.
 * Supports eq/neq comparisons and indexed join equality.
 */
public class IxCol<TRow, TValue>(tableName: String, columnName: String) {
    public val refSql: String = "${SqlFormat.quoteIdent(tableName)}.${SqlFormat.quoteIdent(columnName)}"

    public fun eq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql = ${value.sql})")
    public fun <TOtherRow> eq(other: IxCol<TOtherRow, TValue>): IxJoinEq<TRow, TOtherRow> =
        IxJoinEq(refSql, other.refSql)

    public fun neq(value: SqlLiteral<TValue>): BoolExpr<TRow> = BoolExpr("($refSql <> ${value.sql})")
}

/**
 * Represents an indexed equality join condition between two tables.
 * Created by calling [IxCol.eq] with another indexed column.
 * Used as the `on` parameter for semi-join methods.
 */
public class IxJoinEq<@Suppress("unused") TLeftRow, @Suppress("unused") TRightRow>(
    public val leftRefSql: String,
    public val rightRefSql: String,
)
