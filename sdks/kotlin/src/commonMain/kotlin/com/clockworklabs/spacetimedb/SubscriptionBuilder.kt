package com.clockworklabs.spacetimedb

/**
 * Builder for subscribing to SQL queries on a [DbConnection].
 *
 * ```kotlin
 * conn.subscriptionBuilder()
 *     .onApplied { println("Subscription active") }
 *     .onError { err -> println("Subscription failed: $err") }
 *     .subscribe("SELECT * FROM users WHERE online = true)
 * ```
 */
class SubscriptionBuilder(private val connection: DbConnection) {
    private var onAppliedCallback: (() -> Unit)? = null
    private var onErrorCallback: ((String) -> Unit)? = null
    private var onEndedCallback: (() -> Unit)? = null
    private val pendingQueries = mutableListOf<String>()

    fun onApplied(callback: () -> Unit) = apply { this.onAppliedCallback = callback }

    fun onError(callback: (String) -> Unit) = apply { this.onErrorCallback = callback }

    fun onEnded(callback: () -> Unit) = apply { this.onEndedCallback = callback }

    /** Add a raw SQL query string to the pending list. */
    fun addQuery(query: String): SubscriptionBuilder = apply { pendingQueries.add(query) }

    /** Add a query from a [QueryProvider]. Used by generated typed extensions. */
    fun addQueryFrom(provider: com.clockworklabs.spacetimedb.query.QueryProvider): SubscriptionBuilder =
        apply { pendingQueries.add(provider.toSql()) }

    fun subscribe(vararg queries: String): SubscriptionHandle {
        val allQueries = (pendingQueries + queries).toList()
        pendingQueries.clear()
        val handle = SubscriptionHandle(
            connection = connection,
            onAppliedCallback = onAppliedCallback,
            onErrorCallback = onErrorCallback,
            onEndedCallback = onEndedCallback,
        )
        connection.subscribe(allQueries, handle)
        return handle
    }

    fun subscribeToAllTables(): SubscriptionHandle {
        return subscribe("SELECT * FROM *")
    }
}
