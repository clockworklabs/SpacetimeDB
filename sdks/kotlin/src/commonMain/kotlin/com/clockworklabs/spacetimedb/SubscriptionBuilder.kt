package com.clockworklabs.spacetimedb

/**
 * Builder for subscribing to SQL queries on a [DbConnection].
 *
 * ```kotlin
 * conn.subscriptionBuilder()
 *     .onApplied { println("Subscription active") }
 *     .onError { err -> println("Subscription failed: $err") }
 *     .subscribe("SELECT * FROM users WHERE online = true")
 * ```
 */
class SubscriptionBuilder(private val connection: DbConnection) {
    private var onAppliedCallback: (() -> Unit)? = null
    private var onErrorCallback: ((String) -> Unit)? = null

    fun onApplied(callback: () -> Unit) = apply { this.onAppliedCallback = callback }

    fun onError(callback: (String) -> Unit) = apply { this.onErrorCallback = callback }

    fun subscribe(vararg queries: String): SubscriptionHandle {
        val handle = SubscriptionHandle(
            connection = connection,
            onAppliedCallback = onAppliedCallback,
            onErrorCallback = onErrorCallback,
        )
        connection.subscribe(queries.toList(), handle)
        return handle
    }

    fun subscribeToAllTables(): SubscriptionHandle {
        return subscribe("SELECT * FROM *")
    }
}
