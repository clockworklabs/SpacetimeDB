package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentListOf

/**
 * Builder for configuring subscription callbacks before subscribing.
 * Matches TS SDK's SubscriptionBuilderImpl pattern.
 */
public class SubscriptionBuilder internal constructor(
    private val connection: DbConnection,
) {
    private val onAppliedCallbacks = atomic(persistentListOf<(EventContext.SubscribeApplied) -> Unit>())
    private val onErrorCallbacks = atomic(persistentListOf<(EventContext.Error, Throwable) -> Unit>())
    private val querySqls = atomic(persistentListOf<String>())

    public fun onApplied(cb: (EventContext.SubscribeApplied) -> Unit): SubscriptionBuilder = apply {
        onAppliedCallbacks.update { it.add(cb) }
    }

    public fun onError(cb: (EventContext.Error, Throwable) -> Unit): SubscriptionBuilder = apply {
        onErrorCallbacks.update { it.add(cb) }
    }

    /**
     * Add a raw SQL query to the subscription.
     */
    public fun addQuery(sql: String): SubscriptionBuilder = apply {
        querySqls.update { it.add(sql) }
    }

    /**
     * Subscribe with the accumulated queries.
     * Requires at least one query added via [addQuery].
     */
    public fun subscribe(): SubscriptionHandle {
        val queries = querySqls.value
        check(queries.isNotEmpty()) { "No queries added. Use addQuery() before subscribe()." }
        return connection.subscribe(queries, onApplied = onAppliedCallbacks.value, onError = onErrorCallbacks.value)
    }

    /**
     * Subscribe to a single raw SQL query.
     */
    public fun subscribe(query: String): SubscriptionHandle =
        subscribe(listOf(query))

    /**
     * Subscribe to multiple raw SQL queries.
     */
    public fun subscribe(queries: List<String>): SubscriptionHandle {
        return connection.subscribe(queries, onApplied = onAppliedCallbacks.value, onError = onErrorCallbacks.value)
    }

    /**
     * Subscribe to all registered tables by generating
     * `SELECT * FROM <table>` for each table in the client cache.
     */
    public fun subscribeToAllTables(): SubscriptionHandle {
        val queries = connection.clientCache.tableNames().map { "SELECT * FROM $it" }
        return subscribe(queries)
    }
}
