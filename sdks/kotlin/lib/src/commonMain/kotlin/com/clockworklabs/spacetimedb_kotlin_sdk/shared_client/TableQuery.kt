package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * A type-safe query reference for a specific table row type.
 * Generated code creates these via [QueryBuilder] per-table methods.
 *
 * The type parameter [T] tracks the row type at compile time,
 * ensuring type-safe subscription queries.
 */
class TableQuery<@Suppress("unused") T>(private val tableName: String) {
    fun toSql(): String = "SELECT * FROM $tableName"
}
