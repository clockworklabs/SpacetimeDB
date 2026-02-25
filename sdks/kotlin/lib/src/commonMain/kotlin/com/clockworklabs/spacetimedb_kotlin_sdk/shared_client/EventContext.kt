@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp

/**
 * Reducer call status.
 */
sealed interface Status {
    data object Committed : Status
    data class Failed(val message: String) : Status
    data object OutOfEnergy : Status
}

/**
 * Procedure event data for procedure-specific callbacks.
 * Matches C#'s ProcedureEvent record.
 */
data class ProcedureEvent(
    val timestamp: Timestamp,
    val status: ProcedureStatus,
    val callerIdentity: Identity,
    val callerConnectionId: ConnectionId?,
    val totalHostExecutionDuration: TimeDuration,
    val requestId: UInt,
)

/**
 * Context passed to callbacks. Sealed interface with specialized subtypes
 * so callbacks receive only the fields relevant to their event type.
 *
 * Mirrors TS SDK's EventContextInterface / ReducerEventContextInterface /
 * SubscriptionEventContextInterface / ErrorContextInterface.
 */
sealed interface EventContext {
    val id: String
    val connection: DbConnection

    data class SubscribeApplied(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    data class UnsubscribeApplied(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    data class Transaction(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    data class Reducer<A>(
        override val id: String,
        override val connection: DbConnection,
        val timestamp: Timestamp,
        val reducerName: String,
        val args: A,
        val status: Status,
        val callerIdentity: Identity,
        val callerConnectionId: ConnectionId?,
    ) : EventContext

    data class Procedure(
        override val id: String,
        override val connection: DbConnection,
        val event: ProcedureEvent,
    ) : EventContext

    data class Error(
        override val id: String,
        override val connection: DbConnection,
        val error: Throwable,
    ) : EventContext

    /**
     * A reducer result was received but no matching [ReducerCallInfo] was found.
     * This is defensive — it can happen if the reducer was called from another client
     * or if the call info was lost (e.g. reconnect).
     */
    data class UnknownTransaction(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext
}
