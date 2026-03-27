package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/** Success payload for a one-off SQL query result. */
public data class OneOffQueryData(
    /** Number of tables that returned rows. */
    val tableCount: Int,
)

/** Result type for one-off SQL queries. */
public typealias OneOffQueryResult = SdkResult<OneOffQueryData, QueryError>
