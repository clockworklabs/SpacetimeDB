package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlin.time.Duration

/**
 * Reducer call status.
 */
public sealed interface Status {
    /** The reducer committed its transaction successfully. */
    public data object Committed : Status
    /** The reducer failed with the given error [message]. */
    public data class Failed(val message: String) : Status
}

/**
 * Procedure event data for procedure-specific callbacks.
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
 * Restricts access to the subset of operations appropriate inside event handlers.
 *
 * Generated code adds extension properties (`db`, `reducers`, `procedures`)
 * on this interface for typed access to module bindings.
 */
public interface DbConnectionView {
    /** The identity assigned by the server, or `null` before connection. */
    public val identity: Identity?
    /** The connection ID assigned by the server, or `null` before connection. */
    public val connectionId: ConnectionId?
    /** Whether the connection is currently active. */
    public val isActive: Boolean
    /** Generated table accessors, or `null` if no module bindings were registered. */
    public val moduleTables: ModuleTables?
    /** Generated reducer accessors, or `null` if no module bindings were registered. */
    public val moduleReducers: ModuleReducers?
    /** Generated procedure accessors, or `null` if no module bindings were registered. */
    public val moduleProcedures: ModuleProcedures?

    /** Creates a new [SubscriptionBuilder] for configuring and subscribing to queries. */
    public fun subscriptionBuilder(): SubscriptionBuilder
    /** Subscribes to the given SQL [queries] with optional callbacks. */
    public fun subscribe(
        queries: List<String>,
        onApplied: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
        onError: List<(EventContext.Error, SubscriptionError) -> Unit> = emptyList(),
    ): SubscriptionHandle
    /** Subscribes to the given SQL [queries]. */
    public fun subscribe(vararg queries: String): SubscriptionHandle

    /** Executes a one-off SQL query with a callback for the result. */
    public fun oneOffQuery(
        queryString: String,
        callback: (SdkResult<OneOffQueryData, QueryError>) -> Unit,
    ): UInt
    /** Executes a one-off SQL query, suspending until the result is available. */
    public suspend fun oneOffQuery(
        queryString: String,
        timeout: Duration = Duration.INFINITE,
    ): SdkResult<OneOffQueryData, QueryError>

    /** Disconnects from SpacetimeDB, optionally providing a [reason]. */
    public suspend fun disconnect(reason: Throwable? = null)

    /** Registers a callback invoked when the connection is closed. */
    public fun onDisconnect(cb: (DbConnectionView, Throwable?) -> Unit)
    /** Removes a previously registered disconnect callback. */
    public fun removeOnDisconnect(cb: (DbConnectionView, Throwable?) -> Unit)
    /** Registers a callback invoked when a connection attempt fails. */
    public fun onConnectError(cb: (DbConnectionView, Throwable) -> Unit)
    /** Removes a previously registered connect-error callback. */
    public fun removeOnConnectError(cb: (DbConnectionView, Throwable) -> Unit)
}

/**
 * Context passed to callbacks. Sealed interface with specialized subtypes
 * so callbacks receive only the fields relevant to their event type.
 *
 * Subtypes are plain classes (not data classes) because [connection] is a
 * mutable handle, not value data — it should not participate in equals/hashCode.
 */
public sealed interface EventContext {
    /** Unique identifier for this event. */
    public val id: String
    /** The connection that produced this event. */
    public val connection: DbConnectionView

    /** Fired when a subscription's initial rows have been applied to the client cache. */
    public class SubscribeApplied(
        override val id: String,
        override val connection: DbConnectionView,
    ) : EventContext

    /** Fired when an unsubscription has been confirmed by the server. */
    public class UnsubscribeApplied(
        override val id: String,
        override val connection: DbConnectionView,
    ) : EventContext

    /** Fired when a server-side transaction update has been applied. */
    public class Transaction(
        override val id: String,
        override val connection: DbConnectionView,
    ) : EventContext

    /** Fired when a reducer result is received, carrying the typed arguments and status. */
    public class Reducer<A : Any>(
        override val id: String,
        override val connection: DbConnection,
        public val timestamp: Timestamp,
        public val reducerName: String,
        public val args: A,
        public val status: Status,
        public val callerIdentity: Identity,
        public val callerConnectionId: ConnectionId?,
    ) : EventContext

    /** Fired when a procedure result is received. */
    public class Procedure(
        override val id: String,
        override val connection: DbConnection,
        public val event: ProcedureEvent,
    ) : EventContext

    /** Fired when an error occurs, such as a subscription error. */
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
        override val connection: DbConnectionView,
    ) : EventContext
}

/** Test-only [EventContext] stub. Not part of the public API. */
internal class StubEventContext(override val id: String = "test") : EventContext {
    override val connection: DbConnectionView
        get() = error("StubEventContext.connection should not be accessed in unit tests")
}
