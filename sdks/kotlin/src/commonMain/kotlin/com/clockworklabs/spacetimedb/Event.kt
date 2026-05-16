package com.clockworklabs.spacetimedb

sealed class Status {
    data object Committed : Status()
    data class Failed(val message: String) : Status()
    data class OutOfEnergy(val message: String) : Status()
}

data class ReducerEvent(
    val timestamp: Timestamp,
    val status: Status,
    val callerIdentity: Identity,
    val callerConnectionId: ConnectionId,
    val reducerName: String,
    val energyConsumed: Long,
)

sealed class Event<out R> {
    data class Reducer<R>(val event: ReducerEvent) : Event<R>()
    data object SubscribeApplied : Event<Nothing>()
    data object UnsubscribeApplied : Event<Nothing>()
    data object Disconnected : Event<Nothing>()
    data class SubscribeError(val message: String) : Event<Nothing>()
    data object Transaction : Event<Nothing>()
}

data class Credentials(
    val identity: Identity,
    val token: String,
)
