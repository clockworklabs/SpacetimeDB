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

sealed class Event {
    data class Reducer(val event: ReducerEvent) : Event()
    data object SubscribeApplied : Event()
    data object UnsubscribeApplied : Event()
    data object Disconnected : Event()
    data class SubscribeError(val message: String) : Event()
    data object Transaction : Event()
}

data class Credentials(
    val identity: Identity,
    val token: String,
)
