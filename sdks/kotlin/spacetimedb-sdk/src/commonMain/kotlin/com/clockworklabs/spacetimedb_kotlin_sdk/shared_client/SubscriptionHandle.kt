package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.UnsubscribeFlags
import kotlinx.atomicfu.atomic

/**
 * Subscription lifecycle state.
 */
public enum class SubscriptionState {
    PENDING,
    ACTIVE,
    UNSUBSCRIBING,
    ENDED,
}

/**
 * Handle to a subscription.
 *
 * Tracks the lifecycle: Pending -> Active -> Ended.
 * - Active after SubscribeApplied received
 * - Ended after UnsubscribeApplied or SubscriptionError received
 */
public class SubscriptionHandle internal constructor(
    /** The server-assigned query set identifier for this subscription. */
    @InternalSpacetimeApi
    public val querySetId: QuerySetId,
    /** The SQL queries this subscription is tracking. */
    public val queries: List<String>,
    private val connection: DbConnection,
    private val onAppliedCallbacks: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
    private val onErrorCallbacks: List<(EventContext.Error, SubscriptionError) -> Unit> = emptyList(),
) {
    private val _state = atomic(SubscriptionState.PENDING)
    /** The current lifecycle state of this subscription. */
    public val state: SubscriptionState get() = _state.value
    /** Whether the subscription is pending (sent but not yet confirmed by the server). */
    public val isPending: Boolean get() = _state.value == SubscriptionState.PENDING
    /** Whether the subscription is active (confirmed and receiving updates). */
    public val isActive: Boolean get() = _state.value == SubscriptionState.ACTIVE
    /** Whether an unsubscribe request has been sent but not yet confirmed. */
    public val isUnsubscribing: Boolean get() = _state.value == SubscriptionState.UNSUBSCRIBING
    /** Whether the subscription has ended (unsubscribed or errored). */
    public val isEnded: Boolean get() = _state.value == SubscriptionState.ENDED

    private val _onEndCallback = atomic<((EventContext.UnsubscribeApplied) -> Unit)?>(null)

    /**
     * Unsubscribe from this subscription.
     * The onEnd callback will fire when the server confirms.
     */
    public fun unsubscribe() {
        doUnsubscribe()
    }

    /**
     * Unsubscribe and register a callback for when it completes.
     */
    public fun unsubscribeThen(
        onEnd: (EventContext.UnsubscribeApplied) -> Unit,
    ) {
        doUnsubscribe(onEnd)
    }

    private fun doUnsubscribe(
        onEnd: ((EventContext.UnsubscribeApplied) -> Unit)? = null,
    ) {
        if (!_state.compareAndSet(SubscriptionState.ACTIVE, SubscriptionState.UNSUBSCRIBING)) {
            error("Cannot unsubscribe: subscription is ${_state.value}")
        }
        // Set callback AFTER the CAS succeeds. This is safe because handleEnd()
        // only fires after the server receives our Unsubscribe message (sent below).
        if (onEnd != null) _onEndCallback.value = onEnd
        connection.unsubscribe(this, UnsubscribeFlags.SendDroppedRows)
    }

    internal suspend fun handleApplied(ctx: EventContext.SubscribeApplied) {
        _state.value = SubscriptionState.ACTIVE
        for (cb in onAppliedCallbacks) connection.runUserCallback { cb(ctx) }
    }

    internal suspend fun handleError(ctx: EventContext.Error, error: SubscriptionError) {
        _state.value = SubscriptionState.ENDED
        for (cb in onErrorCallbacks) connection.runUserCallback { cb(ctx, error) }
    }

    internal suspend fun handleEnd(ctx: EventContext.UnsubscribeApplied) {
        _state.value = SubscriptionState.ENDED
        _onEndCallback.value?.let { connection.runUserCallback { it.invoke(ctx) } }
    }

    internal fun markEnded() {
        _state.value = SubscriptionState.ENDED
    }
}
