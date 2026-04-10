@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class)

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.jvm.JvmName

/**
 * A query that can be converted to a SQL string.
 * Implemented by [Table], [FromWhere], [LeftSemiJoin], and [RightSemiJoin].
 */
public interface Query<@Suppress("unused") TRow> {
    /** Converts this query to its SQL string representation. */
    public fun toSql(): String
}

/**
 * A type-safe query reference for a specific table.
 * Generated code creates these via per-table methods on `QueryBuilder`.
 *
 * @param TRow the row type of this table
 * @param TCols the column accessor class (generated per-table)
 * @param TIxCols the indexed column accessor class (generated per-table)
 */
public class Table<TRow, TCols, TIxCols>(
    private val tableName: String,
    internal val cols: TCols,
    internal val ixCols: TIxCols,
) : Query<TRow> {
    internal val tableRefSql: String get() = SqlFormat.quoteIdent(tableName)

    override fun toSql(): String = "SELECT * FROM ${SqlFormat.quoteIdent(tableName)}"

    /** Adds a WHERE clause to this table query. */
    public fun where(predicate: (TCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols))

    public fun where(predicate: (TCols, TIxCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereCol")
    public fun where(predicate: (TCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols).eq(SqlLit.bool(true)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereColIx")
    public fun where(predicate: (TCols, TIxCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols).eq(SqlLit.bool(true)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereIxColIx")
    public fun where(predicate: (TCols, TIxCols) -> IxCol<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols).eq(SqlLit.bool(true)))

    /** Alias for [where]; adds a WHERE clause to this table query. */
    public fun filter(predicate: (TCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols))

    public fun filter(predicate: (TCols, TIxCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterCol")
    public fun filter(predicate: (TCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols).eq(SqlLit.bool(true)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterColIx")
    public fun filter(predicate: (TCols, TIxCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols).eq(SqlLit.bool(true)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterIxColIx")
    public fun filter(predicate: (TCols, TIxCols) -> IxCol<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(this, predicate(cols, ixCols).eq(SqlLit.bool(true)))

    /** Creates a left semi-join with [right], returning rows from this table where a match exists. */
    public fun <TRRow, TRCols, TRIxCols> leftSemijoin(
        right: Table<TRRow, TRCols, TRIxCols>,
        on: (TIxCols, TRIxCols) -> IxJoinEq<TRow, TRRow>,
    ): LeftSemiJoin<TRow, TCols, TIxCols, TRRow, TRCols, TRIxCols> =
        LeftSemiJoin(this, right, on(ixCols, right.ixCols))

    /** Creates a right semi-join with [right], returning rows from the right table where a match exists. */
    public fun <TRRow, TRCols, TRIxCols> rightSemijoin(
        right: Table<TRRow, TRCols, TRIxCols>,
        on: (TIxCols, TRIxCols) -> IxJoinEq<TRow, TRRow>,
    ): RightSemiJoin<TRow, TCols, TIxCols, TRRow, TRCols, TRIxCols> =
        RightSemiJoin(this, right, on(ixCols, right.ixCols))
}

/**
 * A table query with a WHERE clause.
 * Created by calling [Table.where] or [Table.filter].
 * Additional [where] calls chain predicates with AND.
 */
public class FromWhere<TRow, TCols, TIxCols>(
    private val table: Table<TRow, TCols, TIxCols>,
    private val expr: BoolExpr<TRow>,
) : Query<TRow> {
    override fun toSql(): String = "${table.toSql()} WHERE ${expr.sql}"

    /** Chains an additional AND predicate onto this query's WHERE clause. */
    public fun where(predicate: (TCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols)))

    public fun where(predicate: (TCols, TIxCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereCol")
    public fun where(predicate: (TCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols).eq(SqlLit.bool(true))))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereColIx")
    public fun where(predicate: (TCols, TIxCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols).eq(SqlLit.bool(true))))

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereIxColIx")
    public fun where(predicate: (TCols, TIxCols) -> IxCol<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols).eq(SqlLit.bool(true))))

    /** Alias for [where]; chains an additional AND predicate onto this query's WHERE clause. */
    public fun filter(predicate: (TCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols)))

    public fun filter(predicate: (TCols, TIxCols) -> BoolExpr<TRow>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols)))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterCol")
    public fun filter(predicate: (TCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols).eq(SqlLit.bool(true))))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterColIx")
    public fun filter(predicate: (TCols, TIxCols) -> Col<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols).eq(SqlLit.bool(true))))

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterIxColIx")
    public fun filter(predicate: (TCols, TIxCols) -> IxCol<TRow, Boolean>): FromWhere<TRow, TCols, TIxCols> =
        FromWhere(table, expr.and(predicate(table.cols, table.ixCols).eq(SqlLit.bool(true))))

    /** Creates a left semi-join with [right], preserving this query's WHERE clause. */
    public fun <TRRow, TRCols, TRIxCols> leftSemijoin(
        right: Table<TRRow, TRCols, TRIxCols>,
        on: (TIxCols, TRIxCols) -> IxJoinEq<TRow, TRRow>,
    ): LeftSemiJoin<TRow, TCols, TIxCols, TRRow, TRCols, TRIxCols> =
        LeftSemiJoin(this.table, right, on(table.ixCols, right.ixCols), expr)

    /** Creates a right semi-join with [right], preserving this query's WHERE clause. */
    public fun <TRRow, TRCols, TRIxCols> rightSemijoin(
        right: Table<TRRow, TRCols, TRIxCols>,
        on: (TIxCols, TRIxCols) -> IxJoinEq<TRow, TRRow>,
    ): RightSemiJoin<TRow, TCols, TIxCols, TRRow, TRCols, TRIxCols> =
        RightSemiJoin(this.table, right, on(table.ixCols, right.ixCols), expr)
}

/**
 * A left semi-join query. Returns rows from the left table.
 * Created by calling [Table.leftSemijoin] or [FromWhere.leftSemijoin].
 */
public class LeftSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols>(
    private val left: Table<TLRow, TLCols, TLIxCols>,
    private val right: Table<TRRow, TRCols, TRIxCols>,
    private val join: IxJoinEq<TLRow, TRRow>,
    private val whereExpr: BoolExpr<TLRow>? = null,
) : Query<TLRow> {
    override fun toSql(): String {
        val base = "SELECT ${left.tableRefSql}.* FROM ${left.tableRefSql} JOIN ${right.tableRefSql} ON ${join.leftRefSql} = ${join.rightRefSql}"
        return if (whereExpr != null) "$base WHERE ${whereExpr.sql}" else base
    }

    /** Adds a WHERE predicate on the left table's columns. */
    public fun where(predicate: (TLCols) -> BoolExpr<TLRow>): LeftSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(left.cols)
        return LeftSemiJoin(left, right, join, whereExpr?.and(newExpr) ?: newExpr)
    }

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereCol")
    public fun where(predicate: (TLCols) -> Col<TLRow, Boolean>): LeftSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(left.cols).eq(SqlLit.bool(true))
        return LeftSemiJoin(left, right, join, whereExpr?.and(newExpr) ?: newExpr)
    }

    /** Alias for [where]; adds a WHERE predicate on the left table's columns. */
    public fun filter(predicate: (TLCols) -> BoolExpr<TLRow>): LeftSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(left.cols)
        return LeftSemiJoin(left, right, join, whereExpr?.and(newExpr) ?: newExpr)
    }

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterCol")
    public fun filter(predicate: (TLCols) -> Col<TLRow, Boolean>): LeftSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(left.cols).eq(SqlLit.bool(true))
        return LeftSemiJoin(left, right, join, whereExpr?.and(newExpr) ?: newExpr)
    }
}

/**
 * A right semi-join query. Returns rows from the right table.
 * Created by calling [Table.rightSemijoin] or [FromWhere.rightSemijoin].
 */
public class RightSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols>(
    private val left: Table<TLRow, TLCols, TLIxCols>,
    private val right: Table<TRRow, TRCols, TRIxCols>,
    private val join: IxJoinEq<TLRow, TRRow>,
    private val leftWhereExpr: BoolExpr<TLRow>? = null,
    private val rightWhereExpr: BoolExpr<TRRow>? = null,
) : Query<TRRow> {
    override fun toSql(): String {
        val base = "SELECT ${right.tableRefSql}.* FROM ${left.tableRefSql} JOIN ${right.tableRefSql} ON ${join.leftRefSql} = ${join.rightRefSql}"
        val conditions = mutableListOf<String>()
        if (leftWhereExpr != null) conditions.add(leftWhereExpr.sql)
        if (rightWhereExpr != null) conditions.add(rightWhereExpr.sql)
        return if (conditions.isEmpty()) base else "$base WHERE ${conditions.joinToString(" AND ")}"
    }

    /** Adds a WHERE predicate on the right table's columns. */
    public fun where(predicate: (TRCols) -> BoolExpr<TRRow>): RightSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(right.cols)
        return RightSemiJoin(left, right, join, leftWhereExpr, rightWhereExpr?.and(newExpr) ?: newExpr)
    }

    @OverloadResolutionByLambdaReturnType
    @JvmName("whereCol")
    public fun where(predicate: (TRCols) -> Col<TRRow, Boolean>): RightSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(right.cols).eq(SqlLit.bool(true))
        return RightSemiJoin(left, right, join, leftWhereExpr, rightWhereExpr?.and(newExpr) ?: newExpr)
    }

    /** Alias for [where]; adds a WHERE predicate on the right table's columns. */
    public fun filter(predicate: (TRCols) -> BoolExpr<TRRow>): RightSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(right.cols)
        return RightSemiJoin(left, right, join, leftWhereExpr, rightWhereExpr?.and(newExpr) ?: newExpr)
    }

    @OverloadResolutionByLambdaReturnType
    @JvmName("filterCol")
    public fun filter(predicate: (TRCols) -> Col<TRRow, Boolean>): RightSemiJoin<TLRow, TLCols, TLIxCols, TRRow, TRCols, TRIxCols> {
        val newExpr = predicate(right.cols).eq(SqlLit.bool(true))
        return RightSemiJoin(left, right, join, leftWhereExpr, rightWhereExpr?.and(newExpr) ?: newExpr)
    }
}
