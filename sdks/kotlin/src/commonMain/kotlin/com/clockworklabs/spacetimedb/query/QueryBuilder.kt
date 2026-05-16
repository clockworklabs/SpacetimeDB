package com.clockworklabs.spacetimedb.query

/** Marker interface for types that can produce SQL queries. */
fun interface QueryProvider {
    fun toSql(): String
}

/** A typed column reference within a table [T]. */
class Col<V>(val columnName: String) {
    fun eq(value: V): BoolExpr = BoolExpr.Eq(this, value)
    fun ne(value: V): BoolExpr = BoolExpr.Ne(this, value)
    fun gt(value: V): BoolExpr = BoolExpr.Gt(this, value)
    fun lt(value: V): BoolExpr = BoolExpr.Lt(this, value)
    fun gte(value: V): BoolExpr = BoolExpr.Gte(this, value)
    fun lte(value: V): BoolExpr = BoolExpr.Lte(this, value)
}

/** A boolean expression for WHERE clauses. */
sealed class BoolExpr {
    data class Eq(val col: Col<*>, val value: Any?) : BoolExpr()
    data class Ne(val col: Col<*>, val value: Any?) : BoolExpr()
    data class Gt(val col: Col<*>, val value: Any?) : BoolExpr()
    data class Lt(val col: Col<*>, val value: Any?) : BoolExpr()
    data class Gte(val col: Col<*>, val value: Any?) : BoolExpr()
    data class Lte(val col: Col<*>, val value: Any?) : BoolExpr()
    data class And(val left: BoolExpr, val right: BoolExpr) : BoolExpr()
    data class Or(val left: BoolExpr, val right: BoolExpr) : BoolExpr()

    infix fun and(other: BoolExpr): BoolExpr = And(this, other)
    infix fun or(other: BoolExpr): BoolExpr = Or(this, other)

    internal fun toSql(tableName: String): String = when (this) {
        is Eq -> "\"${tableName}\".\"${col.columnName}\" = ${formatValue(value)}"
        is Ne -> "\"${tableName}\".\"${col.columnName}\" != ${formatValue(value)}"
        is Gt -> "\"${tableName}\".\"${col.columnName}\" > ${formatValue(value)}"
        is Lt -> "\"${tableName}\".\"${col.columnName}\" < ${formatValue(value)}"
        is Gte -> "\"${tableName}\".\"${col.columnName}\" >= ${formatValue(value)}"
        is Lte -> "\"${tableName}\".\"${col.columnName}\" <= ${formatValue(value)}"
        is And -> "(${left.toSql(tableName)} AND ${right.toSql(tableName)})"
        is Or -> "(${left.toSql(tableName)} OR ${right.toSql(tableName)})"
    }

    private fun formatValue(value: Any?): String = when (value) {
        null -> "NULL"
        is String -> "'${value.replace("'", "''")}'"
        is Boolean -> if (value) "true" else "false"
        is Number -> value.toString()
        else -> "'$value'"
    }
}

/** Base class for generated column accessor structs. */
abstract class Cols<T>(val tableName: String)

/**
 * A reference to a table, providing typed query building.
 *
 * Usage:
 * ```
 * QueryTable("users") { UsersCols(it) }.where { cols -> cols.age.gt(18) }
 * ```
 */
class QueryTable<T>(val tableName: String, private val colsFactory: (String) -> Cols<T>) : QueryProvider {
    override fun toSql(): String = """SELECT * FROM "$tableName""""

    /** Add a WHERE clause using the table's generated column accessors. */
    fun where(block: Cols<T>.() -> BoolExpr): FromQuery<T> =
        FromQuery(tableName, block(colsFactory(tableName)))
}

/** A table reference with a WHERE clause attached. */
class FromQuery<T>(val tableName: String, val expr: BoolExpr) : QueryProvider {
    override fun toSql(): String = """SELECT * FROM "$tableName" WHERE ${expr.toSql(tableName)}"""
}

