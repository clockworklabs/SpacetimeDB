@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetId

/**
 * Subscription lifecycle state.
 */
enum class SubscriptionState {
    PENDING,
    ACTIVE,
    ENDED,
}

/**
 * Handle to a subscription. Mirrors TS SDK's SubscriptionHandleImpl.
 *
 * Tracks the lifecycle: Pending -> Active -> Ended.
 * - Active after SubscribeApplied received
 * - Ended after UnsubscribeApplied or SubscriptionError received
 */
class SubscriptionHandle internal constructor(
    val querySetId: QuerySetId,
    val queries: List<String>,
    private val connection: DbConnection,
    private val onAppliedCallbacks: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
    private val onErrorCallbacks: List<(EventContext.Error, Throwable) -> Unit> = emptyList(),
) {
    var state: SubscriptionState = SubscriptionState.PENDING
        private set

    private var onEndCallback: ((EventContext.UnsubscribeApplied) -> Unit)? = null
    private var unsubscribeCalled = false

    val isActive: Boolean get() = state == SubscriptionState.ACTIVE
    val isEnded: Boolean get() = state == SubscriptionState.ENDED

    /**
     * Unsubscribe from this subscription.
     * The onEnd callback will fire when the server confirms.
     */
    fun unsubscribe() {
        doUnsubscribe()
    }

    /**
     * Unsubscribe and register a callback for when it completes.
     */
    fun unsubscribeThen(onEnd: (EventContext.UnsubscribeApplied) -> Unit) {
        onEndCallback = onEnd
        doUnsubscribe()
    }

    private fun doUnsubscribe() {
        check(state == SubscriptionState.ACTIVE) { "Cannot unsubscribe: subscription is $state" }
        check(!unsubscribeCalled) { "Cannot unsubscribe: already unsubscribed" }
        unsubscribeCalled = true
        connection.unsubscribe(this)
    }

    internal fun handleApplied(ctx: EventContext.SubscribeApplied) {
        state = SubscriptionState.ACTIVE
        for (cb in onAppliedCallbacks) cb(ctx)
    }

    internal fun handleError(ctx: EventContext.Error, error: Throwable) {
        state = SubscriptionState.ENDED
        for (cb in onErrorCallbacks) cb(ctx, error)
    }

    internal fun handleEnd(ctx: EventContext.UnsubscribeApplied) {
        state = SubscriptionState.ENDED
        onEndCallback?.invoke(ctx)
    }
}
