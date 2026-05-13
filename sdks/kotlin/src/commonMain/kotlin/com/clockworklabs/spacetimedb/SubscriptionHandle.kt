package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.protocol.QuerySetId

/** Lifecycle states of a subscription. */
enum class SubscriptionState {
    PENDING,
    ACTIVE,
    ENDED,
    CANCELLED,
}

/**
 * Represents an active subscription to one or more SQL queries.
 *
 * Created by [SubscriptionBuilder.subscribe]. Call [unsubscribe] to end it.
 */
class SubscriptionHandle internal constructor(
    private val connection: DbConnection,
    internal val onAppliedCallback: (() -> Unit)?,
    internal val onErrorCallback: ((String) -> Unit)?,
    internal var onEndedCallback: (() -> Unit)? = null,
) {
    internal var querySetId: QuerySetId? = null
    internal var requestId: UInt = 0u
    var state: SubscriptionState = SubscriptionState.PENDING
        internal set

    val isActive: Boolean get() = state == SubscriptionState.ACTIVE
    val isEnded: Boolean get() = state == SubscriptionState.ENDED

    fun unsubscribe() {
        when (state) {
            SubscriptionState.PENDING -> {
                state = SubscriptionState.CANCELLED
                connection.pendingCancel(this)
            }
            SubscriptionState.ACTIVE -> {
                state = SubscriptionState.ENDED
                connection.unsubscribe(this)
            }
            else -> {}
        }
    }

    fun unsubscribeThen(onEnded: () -> Unit) {
        when (state) {
            SubscriptionState.PENDING -> {
                state = SubscriptionState.CANCELLED
                connection.pendingCancel(this)
                onEnded()
            }
            SubscriptionState.ACTIVE -> {
                state = SubscriptionState.ENDED
                connection.unsubscribeThen(this, onEnded)
            }
            else -> {}
        }
    }
}
