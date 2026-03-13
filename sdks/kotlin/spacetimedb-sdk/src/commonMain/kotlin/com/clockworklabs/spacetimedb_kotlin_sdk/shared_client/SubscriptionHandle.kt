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
 * Handle to a subscription. Mirrors TS SDK's SubscriptionHandleImpl.
 *
 * Tracks the lifecycle: Pending -> Active -> Ended.
 * - Active after SubscribeApplied received
 * - Ended after UnsubscribeApplied or SubscriptionError received
 */
public class SubscriptionHandle internal constructor(
    public val querySetId: QuerySetId,
    public val queries: List<String>,
    private val connection: DbConnection,
    private val onAppliedCallbacks: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
    private val onErrorCallbacks: List<(EventContext.Error, Throwable) -> Unit> = emptyList(),
) {
    private val _state = atomic(SubscriptionState.PENDING)
    public val state: SubscriptionState get() = _state.value
    public val isPending: Boolean get() = _state.value == SubscriptionState.PENDING
    public val isActive: Boolean get() = _state.value == SubscriptionState.ACTIVE
    public val isUnsubscribing: Boolean get() = _state.value == SubscriptionState.UNSUBSCRIBING
    public val isEnded: Boolean get() = _state.value == SubscriptionState.ENDED

    private val _onEndCallback = atomic<((EventContext.UnsubscribeApplied) -> Unit)?>(null)

    /**
     * Unsubscribe from this subscription.
     * The onEnd callback will fire when the server confirms.
     */
    public fun unsubscribe(flags: UnsubscribeFlags = UnsubscribeFlags.Default) {
        doUnsubscribe(flags)
    }

    /**
     * Unsubscribe and register a callback for when it completes.
     */
    public fun unsubscribeThen(
        flags: UnsubscribeFlags = UnsubscribeFlags.Default,
        onEnd: (EventContext.UnsubscribeApplied) -> Unit,
    ) {
        doUnsubscribe(flags, onEnd)
    }

    private fun doUnsubscribe(
        flags: UnsubscribeFlags,
        onEnd: ((EventContext.UnsubscribeApplied) -> Unit)? = null,
    ) {
        // Set callback BEFORE the CAS so handleEnd() can't race between
        // the state transition and the callback assignment.
        if (onEnd != null) _onEndCallback.value = onEnd
        if (!_state.compareAndSet(SubscriptionState.ACTIVE, SubscriptionState.UNSUBSCRIBING)) {
            _onEndCallback.value = null
            error("Cannot unsubscribe: subscription is ${_state.value}")
        }
        connection.unsubscribe(this, flags)
    }

    internal suspend fun handleApplied(ctx: EventContext.SubscribeApplied) {
        _state.value = SubscriptionState.ACTIVE
        for (cb in onAppliedCallbacks) connection.runUserCallback { cb(ctx) }
    }

    internal suspend fun handleError(ctx: EventContext.Error, error: Throwable) {
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
