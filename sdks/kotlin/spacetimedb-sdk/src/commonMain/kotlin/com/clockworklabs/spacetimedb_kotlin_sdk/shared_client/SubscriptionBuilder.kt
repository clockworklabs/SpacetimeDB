package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Builder for configuring subscription callbacks before subscribing.
 */
public class SubscriptionBuilder internal constructor(
    private val connection: DbConnection,
) {
    private val onAppliedCallbacks = mutableListOf<(EventContext.SubscribeApplied) -> Unit>()
    private val onErrorCallbacks = mutableListOf<(EventContext.Error, SubscriptionError) -> Unit>()
    private val querySqls = mutableListOf<String>()

    /** Registers a callback invoked when the subscription's initial rows are applied. */
    public fun onApplied(cb: (EventContext.SubscribeApplied) -> Unit): SubscriptionBuilder = apply {
        onAppliedCallbacks.add(cb)
    }

    /** Registers a callback invoked when the subscription encounters an error. */
    public fun onError(cb: (EventContext.Error, SubscriptionError) -> Unit): SubscriptionBuilder = apply {
        onErrorCallbacks.add(cb)
    }

    /**
     * Add a raw SQL query to the subscription.
     */
    public fun addQuery(sql: String): SubscriptionBuilder = apply {
        querySqls.add(sql)
    }

    /**
     * Subscribe with the accumulated queries.
     * Requires at least one query added via [addQuery].
     */
    public fun subscribe(): SubscriptionHandle {
        check(querySqls.isNotEmpty()) { "No queries added. Use addQuery() before subscribe()." }
        return connection.subscribe(querySqls, onApplied = onAppliedCallbacks, onError = onErrorCallbacks)
    }

    /**
     * Subscribe to a single raw SQL query.
     */
    public fun subscribe(query: String): SubscriptionHandle =
        subscribe(listOf(query))

    /**
     * Subscribe to the given raw SQL queries.
     */
    public fun subscribe(queries: List<String>): SubscriptionHandle {
        return connection.subscribe(queries, onApplied = onAppliedCallbacks, onError = onErrorCallbacks)
    }

}
