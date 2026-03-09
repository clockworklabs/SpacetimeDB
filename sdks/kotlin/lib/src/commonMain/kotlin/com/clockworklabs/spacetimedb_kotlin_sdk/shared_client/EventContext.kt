package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlin.time.Duration

/**
 * Reducer call status.
 */
public sealed interface Status {
    public data object Committed : Status
    public data class Failed(val message: String) : Status
}

/**
 * Procedure event data for procedure-specific callbacks.
 * Matches C#'s ProcedureEvent record.
 */
public data class ProcedureEvent(
    val timestamp: Timestamp,
    val status: ProcedureStatus,
    val callerIdentity: Identity,
    val callerConnectionId: ConnectionId?,
    val totalHostExecutionDuration: TimeDuration,
    val requestId: UInt,
)

/**
 * Scoped view of [DbConnection] exposed to callback code via [EventContext].
 * Restricts access to the subset of operations that are appropriate for use
 * inside event handlers, matching the C#/TS SDKs' context interface pattern.
 *
 * Generated code adds extension properties (`db`, `reducers`, `procedures`)
 * on this interface for typed access to module bindings.
 */
public interface DbConnectionView {
    public val identity: Identity?
    public val connectionId: ConnectionId?
    public val isActive: Boolean

    public fun subscriptionBuilder(): SubscriptionBuilder
    public fun subscribeToAllTables(
        onApplied: ((EventContext.SubscribeApplied) -> Unit)? = null,
        onError: ((EventContext.Error, Throwable) -> Unit)? = null,
    ): SubscriptionHandle
    public fun subscribe(
        queries: List<String>,
        onApplied: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
        onError: List<(EventContext.Error, Throwable) -> Unit> = emptyList(),
    ): SubscriptionHandle
    public fun subscribe(vararg queries: String): SubscriptionHandle

    public fun oneOffQuery(
        queryString: String,
        callback: (ServerMessage.OneOffQueryResult) -> Unit,
    ): UInt
    public suspend fun oneOffQuery(
        queryString: String,
        timeout: Duration = Duration.INFINITE,
    ): ServerMessage.OneOffQueryResult

    public suspend fun disconnect(reason: Throwable? = null)

    public fun onConnect(cb: (DbConnection, Identity, String) -> Unit)
    public fun removeOnConnect(cb: (DbConnection, Identity, String) -> Unit)
    public fun onDisconnect(cb: (DbConnection, Throwable?) -> Unit)
    public fun removeOnDisconnect(cb: (DbConnection, Throwable?) -> Unit)
    public fun onConnectError(cb: (DbConnection, Throwable) -> Unit)
    public fun removeOnConnectError(cb: (DbConnection, Throwable) -> Unit)
}

/**
 * Context passed to callbacks. Sealed interface with specialized subtypes
 * so callbacks receive only the fields relevant to their event type.
 *
 * Mirrors TS SDK's EventContextInterface / ReducerEventContextInterface /
 * SubscriptionEventContextInterface / ErrorContextInterface.
 *
 * Subtypes are plain classes (not data classes) because [connection] is a
 * mutable handle, not value data — it should not participate in equals/hashCode.
 */
public sealed interface EventContext {
    public val id: String
    public val connection: DbConnectionView

    public class SubscribeApplied(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    public class UnsubscribeApplied(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    public class Transaction(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext

    public class Reducer<A>(
        override val id: String,
        override val connection: DbConnection,
        public val timestamp: Timestamp,
        public val reducerName: String,
        public val args: A,
        public val status: Status,
        public val callerIdentity: Identity,
        public val callerConnectionId: ConnectionId?,
    ) : EventContext

    public class Procedure(
        override val id: String,
        override val connection: DbConnection,
        public val event: ProcedureEvent,
    ) : EventContext

    public class Error(
        override val id: String,
        override val connection: DbConnection,
        public val error: Throwable,
    ) : EventContext

    /**
     * A reducer result was received but no matching [ReducerCallInfo] was found.
     * This is defensive — it can happen if the reducer was called from another client
     * or if the call info was lost (e.g. reconnect).
     */
    public class UnknownTransaction(
        override val id: String,
        override val connection: DbConnection,
    ) : EventContext
}

/** Test-only [EventContext] stub. Not part of the public API. */
internal class StubEventContext(override val id: String = "test") : EventContext {
    override val connection: DbConnectionView
        get() = error("StubEventContext.connection should not be accessed in unit tests")
}
