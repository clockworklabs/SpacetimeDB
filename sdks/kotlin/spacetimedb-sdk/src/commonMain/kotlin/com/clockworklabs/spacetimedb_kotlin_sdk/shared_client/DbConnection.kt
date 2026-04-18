package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QueryResult
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ReducerOutcome
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TransactionUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.UnsubscribeFlags
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.availableCompressionModes
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.defaultCompressionMode
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.SpacetimeTransport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import io.ktor.client.HttpClient
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.getAndUpdate
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentHashMapOf
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toPersistentList
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlin.coroutines.resume
import kotlin.time.Duration

/**
 * Tracks reducer call info so we can populate the Event.Reducer
 * with the correct name/args when the result comes back.
 */
private class ReducerCallInfo(
    val name: String,
    val typedArgs: Any,
)

/**
 * Decodes a BSATN-encoded reducer error into a human-readable string.
 * Reducer errors are BSATN strings (u32 length + UTF-8 bytes).
 * Falls back to hex dump if decoding fails.
 */
private fun decodeReducerError(bytes: ByteArray): String {
    return try {
        val reader = BsatnReader(bytes)
        reader.readString()
    } catch (_: Exception) {
        "Reducer returned undecodable BSATN error bytes (len=${bytes.size})"
    }
}


/**
 * Compression mode for the WebSocket connection.
 */
public enum class CompressionMode(internal val wireValue: String) {
    /** Brotli compression (JVM/Android only). */
    BROTLI("Brotli"),
    /** Gzip compression. */
    GZIP("Gzip"),
    /** No compression. */
    NONE("None"),
}

/**
 * Connection lifecycle state machine.
 *
 * Each variant owns the resources created in that phase.
 * [Connected] carries the coroutine jobs and exposes [Connected.shutdown]
 * to cancel/join them before the cache is cleared — preventing the
 * index-vs-_rows inconsistency that occurs when a CAS loop is still
 * in flight.
 *
 * ```
 * Disconnected ──▶ Connecting ──▶ Connected ──▶ Closed
 *                       │                          ▲
 *                       └──────────────────────────┘
 * ```
 */
public sealed interface ConnectionState {
    /** No connection has been established yet. */
    public data object Disconnected : ConnectionState
    /** A connection attempt is in progress. */
    public data object Connecting : ConnectionState

    /** The WebSocket connection is active and processing messages. */
    public class Connected internal constructor(
        internal val receiveJob: Job,
        internal val sendJob: Job,
    ) : ConnectionState {
        /**
         * Cancel and await the active connection's coroutines.
         * When called from within the receive loop (e.g. SubscriptionError
         * with null requestId triggers disconnect()), [callerJob] matches
         * [receiveJob] and both joins are skipped to avoid deadlock.
         */
        internal suspend fun shutdown(callerJob: Job?) {
            receiveJob.cancel()
            sendJob.cancel()
            if (callerJob != receiveJob) {
                receiveJob.join()
                sendJob.join()
            }
        }
    }

    /** The connection has been closed and cannot be reused. */
    public data object Closed : ConnectionState
}

/**
 * Main entry point for connecting to a SpacetimeDB module.
 *
 * Handles:
 * - WebSocket connection lifecycle
 * - Message send/receive loop
 * - Client cache management
 * - Subscription tracking
 * - Reducer call tracking
 */
public open class DbConnection internal constructor(
    private val transport: Transport,
    private val scope: CoroutineScope,
    onConnectCallbacks: List<(DbConnectionView, Identity, String) -> Unit>,
    onDisconnectCallbacks: List<(DbConnectionView, Throwable?) -> Unit>,
    onConnectErrorCallbacks: List<(DbConnectionView, Throwable) -> Unit>,
    private val clientConnectionId: ConnectionId,
    /** Performance statistics for this connection (request latencies, message counts, etc.). */
    public val stats: Stats,
    internal val moduleDescriptor: ModuleDescriptor?,
    private val callbackDispatcher: CoroutineDispatcher?,
) : DbConnectionView {
    /** Local cache of subscribed table rows, kept in sync with the server. */
    @InternalSpacetimeApi
    public val clientCache: ClientCache = ClientCache()

    private val _moduleTables = atomic<ModuleTables?>(null)
    public override var moduleTables: ModuleTables?
        get() = _moduleTables.value
        internal set(value) { _moduleTables.value = value }

    private val _moduleReducers = atomic<ModuleReducers?>(null)
    public override var moduleReducers: ModuleReducers?
        get() = _moduleReducers.value
        internal set(value) { _moduleReducers.value = value }

    private val _moduleProcedures = atomic<ModuleProcedures?>(null)
    public override var moduleProcedures: ModuleProcedures?
        get() = _moduleProcedures.value
        internal set(value) { _moduleProcedures.value = value }

    private val _identity = atomic<Identity?>(null)
    public override val identity: Identity?
        get() = _identity.value

    private val _connectionId = atomic<ConnectionId?>(null)
    public override val connectionId: ConnectionId?
        get() = _connectionId.value

    private val _token = atomic<String?>(null)
    /** Authentication token assigned by the server, or `null` before connection. */
    public var token: String?
        get() = _token.value
        private set(value) { _token.value = value }

    private val _state = atomic<ConnectionState>(ConnectionState.Disconnected)
    public override val isActive: Boolean get() = _state.value is ConnectionState.Connected

    private val sendChannel = Channel<ClientMessage>(Channel.UNLIMITED)
    private val _nextQuerySetId = atomic(0)
    private val subscriptions = atomic(persistentHashMapOf<UInt, SubscriptionHandle>())
    private val reducerCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Reducer<*>) -> Unit>())
    private val reducerCallInfo = atomic(persistentHashMapOf<UInt, ReducerCallInfo>())
    private val procedureCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit>())
    private val oneOffQueryCallbacks =
        atomic(persistentHashMapOf<UInt, (SdkResult<OneOffQueryData, QueryError>) -> Unit>())
    private val querySetIdToRequestId = atomic(persistentHashMapOf<UInt, UInt>())
    private val _eventId = atomic(0L)
    private val _onConnectCallbacks = onConnectCallbacks.toList()
    private val _onDisconnectCallbacks = atomic(onDisconnectCallbacks.toPersistentList())
    private val _onConnectErrorCallbacks = atomic(onConnectErrorCallbacks.toPersistentList())

    // --- Connection callbacks ---

    public override fun onDisconnect(cb: (DbConnectionView, Throwable?) -> Unit) {
        _onDisconnectCallbacks.update { it.add(cb) }
    }

    public override fun removeOnDisconnect(cb: (DbConnectionView, Throwable?) -> Unit) {
        _onDisconnectCallbacks.update { it.remove(cb) }
    }

    public override fun onConnectError(cb: (DbConnectionView, Throwable) -> Unit) {
        _onConnectErrorCallbacks.update { it.add(cb) }
    }

    public override fun removeOnConnectError(cb: (DbConnectionView, Throwable) -> Unit) {
        _onConnectErrorCallbacks.update { it.remove(cb) }
    }

    private fun nextEventId(): String {
        val id = _eventId.incrementAndGet()
        return "${connectionId?.toHexString() ?: clientConnectionId.toHexString()}:$id"
    }

    /**
     * Run a user callback, optionally dispatching to the configured [callbackDispatcher].
     * When no dispatcher is set, callbacks run on the current (receive-loop) thread.
     * Catches and logs exceptions from user code without crashing the receive loop.
     */
    internal suspend fun runUserCallback(block: () -> Unit) {
        try {
            val dispatcher = callbackDispatcher
            if (dispatcher != null) {
                withContext(dispatcher) { block() }
            } else {
                block()
            }
        } catch (e: Exception) {
            currentCoroutineContext().ensureActive()
            Logger.exception(e)
        }
    }

    /**
     * Connect to SpacetimeDB and start the message receive loop.
     * Called internally by [Builder.build]. Not intended for direct use.
     *
     * If the transport fails to connect, [onConnectError] callbacks are fired
     * and the connection transitions to [ConnectionState.Closed].
     * No exception is thrown — errors are reported via callbacks.
     */
    internal suspend fun connect() {
        val disconnected = _state.value as? ConnectionState.Disconnected
            ?: error(
                if (_state.value is ConnectionState.Closed)
                    "Connection is closed. Create a new DbConnection to reconnect."
                else
                    "connect() called in invalid state: ${_state.value}"
            )
        check(_state.compareAndSet(disconnected, ConnectionState.Connecting)) {
            "connect() called in invalid state: ${_state.value}"
        }
        Logger.info { "Connecting to SpacetimeDB..." }
        try {
            transport.connect()
        } catch (e: Exception) {
            _state.value = ConnectionState.Closed
            scope.cancel()
            for (cb in _onConnectErrorCallbacks.value) runUserCallback { cb(this, e) }
            return
        }

        // Start sender coroutine — drains any buffered messages in FIFO order
        val sendJob = scope.launch {
            for (msg in sendChannel) {
                transport.send(msg)
            }
        }

        // Start receive loop
        val receiveJob = scope.launch {
            try {
                transport.incoming().collect { message ->
                    val applyStart = kotlin.time.TimeSource.Monotonic.markNow()
                    processMessage(message)
                    stats.applyMessageTracker.insertSample(applyStart.elapsedNow())
                }
                // Normal completion — server closed the connection
                _state.value = ConnectionState.Closed
                sendChannel.close()
                failPendingOperations()
                val cbs = _onDisconnectCallbacks.getAndSet(persistentListOf())
                for (cb in cbs) runUserCallback { cb(this@DbConnection, null) }
                clientCache.clear()
            } catch (e: Exception) {
                currentCoroutineContext().ensureActive()
                Logger.error { "Connection error: ${e.message}" }
                _state.value = ConnectionState.Closed
                sendChannel.close()
                failPendingOperations()
                val cbs = _onDisconnectCallbacks.getAndSet(persistentListOf())
                for (cb in cbs) runUserCallback { cb(this@DbConnection, e) }
                clientCache.clear()
            } finally {
                withContext(NonCancellable) {
                    sendChannel.close()
                    try { transport.disconnect() } catch (_: Exception) {}
                }
            }
        }

        _state.compareAndSet(ConnectionState.Connecting, ConnectionState.Connected(receiveJob, sendJob))
    }

    /**
     * Disconnect from SpacetimeDB and release all resources.
     * The connection cannot be reused — create a new [DbConnection] to reconnect.
     *
     * @param reason if non-null, passed to onDisconnect callbacks to distinguish
     *               error-driven disconnects from graceful ones.
     */
    public override suspend fun disconnect(reason: Throwable?) {
        val prev = _state.getAndSet(ConnectionState.Closed)
        if (prev is ConnectionState.Disconnected || prev is ConnectionState.Closed) return
        Logger.info { "Disconnecting from SpacetimeDB" }
        // Close the send channel FIRST so concurrent callReducer/oneOffQuery/etc.
        // calls fail immediately instead of enqueuing messages that will never
        // get responses. This eliminates the TOCTOU window between state=CLOSED
        // and the channel close that previously lived in the receive job's finally block.
        // (Double-close is safe for Channels — it's a no-op.)
        sendChannel.close()
        if (prev is ConnectionState.Connected) {
            prev.shutdown(currentCoroutineContext()[Job])
        }
        failPendingOperations()
        val cbs = _onDisconnectCallbacks.getAndSet(persistentListOf())
        for (cb in cbs) runUserCallback { cb(this@DbConnection, reason) }
        clientCache.clear()
        scope.cancel()
    }

    /**
     * Fail all in-flight operations on disconnect.
     * Clears callback maps so captured lambdas can be GC'd, and marks all
     * subscription handles as ENDED so callers don't try to use stale handles.
     */
    private suspend fun failPendingOperations() {
        val pendingReducers = reducerCallbacks.getAndSet(persistentHashMapOf())
        reducerCallInfo.getAndSet(persistentHashMapOf())
        if (pendingReducers.isNotEmpty()) {
            Logger.warn { "Discarding ${pendingReducers.size} pending reducer callback(s) due to disconnect" }
        }

        val pendingProcedures = procedureCallbacks.getAndSet(persistentHashMapOf())
        if (pendingProcedures.isNotEmpty()) {
            Logger.warn { "Failing ${pendingProcedures.size} pending procedure callback(s) due to disconnect" }
            val errorMsg = "Connection closed before procedure result was received"
            for ((requestId, cb) in pendingProcedures) {
                val procedureEvent = ProcedureEvent(
                    timestamp = Timestamp.UNIX_EPOCH,
                    status = ProcedureStatus.InternalError(errorMsg),
                    callerIdentity = identity ?: Identity.zero(),
                    callerConnectionId = connectionId,
                    totalHostExecutionDuration = TimeDuration(Duration.ZERO),
                    requestId = requestId,
                )
                val ctx = EventContext.Procedure(
                    id = nextEventId(),
                    connection = this,
                    event = procedureEvent,
                )
                val resultMsg = ServerMessage.ProcedureResultMsg(
                    status = ProcedureStatus.InternalError(errorMsg),
                    timestamp = Timestamp.UNIX_EPOCH,
                    totalHostExecutionDuration = TimeDuration(Duration.ZERO),
                    requestId = requestId,
                )
                runUserCallback { cb.invoke(ctx, resultMsg) }
            }
        }

        val pendingQueries = oneOffQueryCallbacks.getAndSet(persistentHashMapOf())
        if (pendingQueries.isNotEmpty()) {
            Logger.warn { "Failing ${pendingQueries.size} pending one-off query callback(s) due to disconnect" }
            val errorResult: SdkResult<OneOffQueryData, QueryError> = SdkResult.Failure(QueryError.Disconnected)
            for ((_, cb) in pendingQueries) {
                runUserCallback { cb.invoke(errorResult) }
            }
        }

        querySetIdToRequestId.getAndSet(persistentHashMapOf())

        val pendingSubs = subscriptions.getAndSet(persistentHashMapOf())
        for ((_, handle) in pendingSubs) {
            handle.markEnded()
        }
    }

    // --- Subscription Builder ---

    public override fun subscriptionBuilder(): SubscriptionBuilder = SubscriptionBuilder(this)


    // --- Subscriptions ---

    /**
     * Subscribe to a set of SQL queries.
     * Returns a SubscriptionHandle to track the subscription lifecycle.
     */
    public override fun subscribe(
        queries: List<String>,
        onApplied: List<(EventContext.SubscribeApplied) -> Unit>,
        onError: List<(EventContext.Error, SubscriptionError) -> Unit>,
    ): SubscriptionHandle {
        val requestId = stats.subscriptionRequestTracker.startTrackingRequest()
        val querySetId = QuerySetId(_nextQuerySetId.incrementAndGet().toUInt())
        val handle = SubscriptionHandle(
            querySetId,
            queries,
            connection = this,
            onAppliedCallbacks = onApplied,
            onErrorCallbacks = onError
        )
        subscriptions.update { it.put(querySetId.id, handle) }
        querySetIdToRequestId.update { it.put(querySetId.id, requestId) }

        val message = ClientMessage.Subscribe(
            requestId = requestId,
            querySetId = querySetId,
            queryStrings = queries,
        )
        Logger.debug { "Subscribing with ${queries.size} queries (requestId=$requestId)" }
        if (!sendMessage(message)) {
            subscriptions.update { it.remove(querySetId.id) }
            querySetIdToRequestId.update { it.remove(querySetId.id) }
            stats.subscriptionRequestTracker.finishTrackingRequest(requestId)
            handle.markEnded()
        }
        return handle
    }

    public override fun subscribe(vararg queries: String): SubscriptionHandle =
        subscribe(queries.toList())

    internal fun unsubscribe(handle: SubscriptionHandle, flags: UnsubscribeFlags) {
        val requestId = stats.subscriptionRequestTracker.startTrackingRequest()
        val message = ClientMessage.Unsubscribe(
            requestId = requestId,
            querySetId = handle.querySetId,
            flags = flags,
        )
        if (!sendMessage(message)) {
            stats.subscriptionRequestTracker.finishTrackingRequest(requestId)
        }
    }

    // --- Reducers ---

    /**
     * Call a reducer on the server.
     * The encodedArgs should be BSATN-encoded reducer arguments.
     * The typedArgs is the typed args object stored for the event context.
     */
    @InternalSpacetimeApi
    public fun <A : Any> callReducer(
        reducerName: String,
        encodedArgs: ByteArray,
        typedArgs: A,
        callback: ((EventContext.Reducer<A>) -> Unit)? = null,
        flags: UByte = 0u,
    ): UInt {
        val requestId = stats.reducerRequestTracker.startTrackingRequest(reducerName)
        if (callback != null) {
            @Suppress("UNCHECKED_CAST")
            reducerCallbacks.update {
                it.put(
                    requestId,
                    callback as (EventContext.Reducer<*>) -> Unit
                )
            }
        }
        reducerCallInfo.update { it.put(requestId, ReducerCallInfo(reducerName, typedArgs as Any)) }
        val message = ClientMessage.CallReducer(
            requestId = requestId,
            flags = flags,
            reducer = reducerName,
            args = encodedArgs,
        )
        Logger.debug { "Calling reducer '$reducerName' (requestId=$requestId)" }
        if (!sendMessage(message)) {
            reducerCallbacks.update { it.remove(requestId) }
            reducerCallInfo.update { it.remove(requestId) }
            stats.reducerRequestTracker.finishTrackingRequest(requestId)
        }
        return requestId
    }

    // --- Procedures ---

    /**
     * Call a procedure on the server.
     * The args should be BSATN-encoded procedure arguments.
     */
    @InternalSpacetimeApi
    public fun callProcedure(
        procedureName: String,
        args: ByteArray,
        callback: ((EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit)? = null,
        flags: UByte = 0u,
    ): UInt {
        val requestId = stats.procedureRequestTracker.startTrackingRequest(procedureName)
        if (callback != null) {
            procedureCallbacks.update { it.put(requestId, callback) }
        }
        val message = ClientMessage.CallProcedure(
            requestId = requestId,
            flags = flags,
            procedure = procedureName,
            args = args,
        )
        Logger.debug { "Calling procedure '$procedureName' (requestId=$requestId)" }
        if (!sendMessage(message)) {
            procedureCallbacks.update { it.remove(requestId) }
            stats.procedureRequestTracker.finishTrackingRequest(requestId)
        }
        return requestId
    }

    // --- One-Off Queries ---

    /**
     * Execute a one-off SQL query against the database.
     * The result callback receives the query result or error.
     */
    public override fun oneOffQuery(
        queryString: String,
        callback: (SdkResult<OneOffQueryData, QueryError>) -> Unit,
    ): UInt {
        val requestId = stats.oneOffRequestTracker.startTrackingRequest()
        oneOffQueryCallbacks.update { it.put(requestId, callback) }
        val message = ClientMessage.OneOffQuery(
            requestId = requestId,
            queryString = queryString,
        )
        Logger.debug { "Executing one-off query (requestId=$requestId)" }
        if (!sendMessage(message)) {
            oneOffQueryCallbacks.update { it.remove(requestId) }
            stats.oneOffRequestTracker.finishTrackingRequest(requestId)
        }
        return requestId
    }

    /**
     * Execute a one-off SQL query against the database, suspending until the result is available.
     *
     * @param timeout maximum time to wait for a response. Defaults to [Duration.INFINITE].
     *                Throws [kotlinx.coroutines.TimeoutCancellationException] if exceeded.
     */
    public override suspend fun oneOffQuery(
        queryString: String,
        timeout: Duration,
    ): SdkResult<OneOffQueryData, QueryError> {
        suspend fun await(): SdkResult<OneOffQueryData, QueryError> =
            suspendCancellableCoroutine { cont ->
                val requestId = oneOffQuery(queryString) { result ->
                    cont.resume(result)
                }
                cont.invokeOnCancellation {
                    oneOffQueryCallbacks.update { it.remove(requestId) }
                }
            }
        return if (timeout.isInfinite()) await() else withTimeout(timeout) { await() }
    }

    // --- Internal ---

    private fun sendMessage(message: ClientMessage): Boolean {
        val result = sendChannel.trySend(message)
        if (result.isFailure) {
            Logger.warn { "Cannot send message: connection is not active" }
            return false
        }
        return true
    }

    private suspend fun processMessage(message: ServerMessage) {
        when (message) {
            is ServerMessage.InitialConnection -> {
                // Validate identity consistency
                val currentIdentity = identity
                if (currentIdentity != null && currentIdentity != message.identity) {
                    val error = IllegalStateException(
                        "Server returned unexpected identity: ${message.identity}, expected: $currentIdentity"
                    )
                    for (cb in _onConnectErrorCallbacks.value) runUserCallback { cb(this, error) }
                    // Throw so the receive loop's catch block transitions to CLOSED
                    // and cleans up resources. Without this, the connection stays in
                    // CONNECTED state with no identity — an inconsistent half-initialized state.
                    throw error
                }

                _identity.value = message.identity
                _connectionId.value = message.connectionId
                if (token == null && message.token.isNotEmpty()) {
                    token = message.token
                }
                Logger.info { "Connected with identity=${message.identity}" }
                for (cb in _onConnectCallbacks) runUserCallback { cb(this, message.identity, message.token) }
            }

            is ServerMessage.SubscribeApplied -> {
                val handle = subscriptions.value[message.querySetId.id] ?: return
                val ctx = EventContext.SubscribeApplied(id = nextEventId(), connection = this)
                var subRequestId: UInt? = null
                querySetIdToRequestId.getAndUpdate { map ->
                    subRequestId = map[message.querySetId.id]
                    map.remove(message.querySetId.id)
                }
                subRequestId?.let { stats.subscriptionRequestTracker.finishTrackingRequest(it) }

                // Inserts only — no pre-apply phase needed
                val callbacks = mutableListOf<PendingCallback>()
                for (tableRows in message.rows.tables) {
                    val table = clientCache.getUntypedTable(tableRows.table) ?: continue
                    callbacks.addAll(table.applyInserts(ctx, tableRows.rows))
                }

                for (cb in callbacks) runUserCallback { cb.invoke() }
                handle.handleApplied(ctx)
            }

            is ServerMessage.UnsubscribeApplied -> {
                val handle = subscriptions.value[message.querySetId.id] ?: return
                val ctx = EventContext.UnsubscribeApplied(id = nextEventId(), connection = this)

                val callbacks = mutableListOf<PendingCallback>()
                if (message.rows != null) {
                    // Parse: decode all rows once
                    val parsed = message.rows.tables.mapNotNull { tableRows ->
                        val table = clientCache.getUntypedTable(tableRows.table) ?: return@mapNotNull null
                        table to table.parseDeletes(tableRows.rows)
                    }
                    // Phase 1: PreApply ALL tables (fire onBeforeDelete before mutations)
                    for ((table, data) in parsed) {
                        table.preApplyDeletes(ctx, data)
                    }
                    // Phase 2: Apply ALL tables (mutate + collect post-callbacks)
                    for ((table, data) in parsed) {
                        callbacks.addAll(table.applyDeletes(ctx, data))
                    }
                }

                subscriptions.update { it.remove(message.querySetId.id) }
                handle.handleEnd(ctx)
                // Phase 3: Fire post-mutation callbacks
                for (cb in callbacks) runUserCallback { cb.invoke() }
            }

            is ServerMessage.SubscriptionError -> {
                val handle = subscriptions.value[message.querySetId.id] ?: run {
                    Logger.warn { "Received SubscriptionError for unknown querySetId=${message.querySetId.id}" }
                    return
                }
                val subError = SubscriptionError.ServerError(message.error)
                val ctx = EventContext.Error(id = nextEventId(), connection = this, error = Exception(message.error))
                Logger.error { "Subscription error: ${message.error}" }
                var subRequestId: UInt? = null
                querySetIdToRequestId.getAndUpdate { map ->
                    subRequestId = map[message.querySetId.id]
                    map.remove(message.querySetId.id)
                }
                subRequestId?.let { stats.subscriptionRequestTracker.finishTrackingRequest(it) }

                if (message.requestId == null) {
                    handle.handleError(ctx, subError)
                    disconnect(Exception(message.error))
                    return
                }

                handle.handleError(ctx, subError)
                subscriptions.update { it.remove(message.querySetId.id) }
            }

            is ServerMessage.TransactionUpdateMsg -> {
                val ctx = EventContext.Transaction(id = nextEventId(), connection = this)
                val callbacks = applyTransactionUpdate(ctx, message.update)
                for (cb in callbacks) runUserCallback { cb.invoke() }
            }

            is ServerMessage.ReducerResultMsg -> {
                val result = message.result
                var info: ReducerCallInfo? = null
                reducerCallInfo.getAndUpdate { map ->
                    info = map[message.requestId]
                    map.remove(message.requestId)
                }
                stats.reducerRequestTracker.finishTrackingRequest(message.requestId)
                val callerIdentity = identity ?: run {
                    Logger.error { "Received ReducerResultMsg before identity was set" }
                    reducerCallbacks.update { it.remove(message.requestId) }
                    return
                }
                val callerConnId = connectionId
                val capturedInfo = info

                when (result) {
                    is ReducerOutcome.Ok -> {
                        val ctx = if (capturedInfo != null) {
                            EventContext.Reducer(
                                id = nextEventId(),
                                connection = this,
                                timestamp = message.timestamp,
                                reducerName = capturedInfo.name,
                                args = capturedInfo.typedArgs,
                                status = Status.Committed,
                                callerIdentity = callerIdentity,
                                callerConnectionId = callerConnId,
                            )
                        } else {
                            EventContext.UnknownTransaction(id = nextEventId(), connection = this)
                        }
                        val callbacks = applyTransactionUpdate(ctx, result.transactionUpdate)
                        for (cb in callbacks) runUserCallback { cb.invoke() }

                        if (ctx is EventContext.Reducer<*>) {
                            fireReducerCallbacks(message.requestId, ctx)
                        }
                    }

                    is ReducerOutcome.OkEmpty -> {
                        if (capturedInfo != null) {
                            val ctx = EventContext.Reducer(
                                id = nextEventId(),
                                connection = this,
                                timestamp = message.timestamp,
                                reducerName = capturedInfo.name,
                                args = capturedInfo.typedArgs,
                                status = Status.Committed,
                                callerIdentity = callerIdentity,
                                callerConnectionId = callerConnId,
                            )
                            fireReducerCallbacks(message.requestId, ctx)
                        }
                    }

                    is ReducerOutcome.Err -> {
                        val errorMsg = decodeReducerError(result.error)
                        Logger.warn { "Reducer '${capturedInfo?.name}' failed: $errorMsg" }
                        if (capturedInfo != null) {
                            val ctx = EventContext.Reducer(
                                id = nextEventId(),
                                connection = this,
                                timestamp = message.timestamp,
                                reducerName = capturedInfo.name,
                                args = capturedInfo.typedArgs,
                                status = Status.Failed(errorMsg),
                                callerIdentity = callerIdentity,
                                callerConnectionId = callerConnId,
                            )
                            fireReducerCallbacks(message.requestId, ctx)
                        }
                    }

                    is ReducerOutcome.InternalError -> {
                        Logger.error { "Reducer '${capturedInfo?.name}' internal error: ${result.message}" }
                        if (capturedInfo != null) {
                            val ctx = EventContext.Reducer(
                                id = nextEventId(),
                                connection = this,
                                timestamp = message.timestamp,
                                reducerName = capturedInfo.name,
                                args = capturedInfo.typedArgs,
                                status = Status.Failed(result.message),
                                callerIdentity = callerIdentity,
                                callerConnectionId = callerConnId,
                            )
                            fireReducerCallbacks(message.requestId, ctx)
                        }
                    }
                }
            }

            is ServerMessage.ProcedureResultMsg -> {
                stats.procedureRequestTracker.finishTrackingRequest(message.requestId)
                var cb: ((EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit)? = null
                procedureCallbacks.getAndUpdate { map ->
                    cb = map[message.requestId]
                    map.remove(message.requestId)
                }
                val procIdentity = identity ?: run {
                    Logger.error { "Received ProcedureResultMsg before identity was set" }
                    return
                }
                val procConnId = connectionId
                cb?.let {
                    val procedureEvent = ProcedureEvent(
                        timestamp = message.timestamp,
                        status = message.status,
                        callerIdentity = procIdentity,
                        callerConnectionId = procConnId,
                        totalHostExecutionDuration = message.totalHostExecutionDuration,
                        requestId = message.requestId,
                    )
                    val ctx = EventContext.Procedure(
                        id = nextEventId(),
                        connection = this,
                        event = procedureEvent
                    )
                    runUserCallback { it.invoke(ctx, message) }
                }
            }

            is ServerMessage.OneOffQueryResult -> {
                stats.oneOffRequestTracker.finishTrackingRequest(message.requestId)
                var cb: ((SdkResult<OneOffQueryData, QueryError>) -> Unit)? = null
                oneOffQueryCallbacks.getAndUpdate { map ->
                    cb = map[message.requestId]
                    map.remove(message.requestId)
                }
                cb?.let { callback ->
                    val sdkResult: SdkResult<OneOffQueryData, QueryError> = when (val r = message.result) {
                        is QueryResult.Ok -> SdkResult.Success(OneOffQueryData(r.rows.tables.size))
                        is QueryResult.Err -> SdkResult.Failure(QueryError.ServerError(r.error))
                    }
                    runUserCallback { callback.invoke(sdkResult) }
                }
            }
        }
    }

    private suspend fun fireReducerCallbacks(requestId: UInt, ctx: EventContext.Reducer<*>) {
        var cb: ((EventContext.Reducer<*>) -> Unit)? = null
        reducerCallbacks.getAndUpdate { map ->
            cb = map[requestId]
            map.remove(requestId)
        }
        cb?.let { runUserCallback { it.invoke(ctx) } }
        moduleDescriptor?.let { runUserCallback { it.handleReducerEvent(this, ctx) } }
    }

    private fun applyTransactionUpdate(
        ctx: EventContext,
        update: TransactionUpdate,
    ): List<PendingCallback> {
        // Parse: decode all rows once
        val allUpdates = mutableListOf<Pair<TableCache<*, *>, ParsedTableData>>()
        for (querySetUpdate in update.querySets) {
            for (tableUpdate in querySetUpdate.tables) {
                val table = clientCache.getUntypedTable(tableUpdate.tableName) ?: continue
                for (rows in tableUpdate.rows) {
                    allUpdates.add(table to table.parseUpdate(rows))
                }
            }
        }

        // Phase 1: PreApply ALL tables (fire onBeforeDelete before any mutations)
        for ((table, parsed) in allUpdates) {
            table.preApplyUpdate(ctx, parsed)
        }

        // Phase 2: Apply ALL tables (mutate + collect post-callbacks)
        val allCallbacks = mutableListOf<PendingCallback>()
        for ((table, parsed) in allUpdates) {
            allCallbacks.addAll(table.applyUpdate(ctx, parsed))
        }

        return allCallbacks
    }

    // --- Builder ---

    /** Fluent builder for configuring and creating a [DbConnection]. */
    public class Builder {
        private var uri: String? = null
        private var nameOrAddress: String? = null
        private var authToken: String? = null
        private var compression: CompressionMode = defaultCompressionMode
        private var lightMode: Boolean = false
        private var confirmedReads: Boolean? = null
        private val onConnectCallbacks = mutableListOf<(DbConnectionView, Identity, String) -> Unit>()
        private val onDisconnectCallbacks = mutableListOf<(DbConnectionView, Throwable?) -> Unit>()
        private val onConnectErrorCallbacks = mutableListOf<(DbConnectionView, Throwable) -> Unit>()
        private var module: ModuleDescriptor? = null
        private var callbackDispatcher: CoroutineDispatcher? = null
        private var httpClient: HttpClient? = null

        /**
         * Provide the [HttpClient] for the WebSocket transport.
         * Must have the Ktor WebSockets plugin installed.
         */
        public fun withHttpClient(client: HttpClient): Builder = apply { httpClient = client }

        /** Sets the SpacetimeDB server URI (e.g. `http://localhost:3000`). */
        public fun withUri(uri: String): Builder = apply { this.uri = uri }
        /** Sets the database name or address to connect to. */
        public fun withDatabaseName(nameOrAddress: String): Builder =
            apply { this.nameOrAddress = nameOrAddress }

        /** Sets the authentication token, or `null` for anonymous connections. */
        public fun withToken(token: String?): Builder = apply { authToken = token }
        /** Sets the compression mode for the WebSocket connection. */
        public fun withCompression(compression: CompressionMode): Builder =
            apply { this.compression = compression }

        /** Enables or disables light mode (reduced initial data transfer). */
        public fun withLightMode(lightMode: Boolean): Builder = apply { this.lightMode = lightMode }
        /** Enables or disables confirmed reads from the server. */
        public fun withConfirmedReads(confirmed: Boolean): Builder = apply { confirmedReads = confirmed }

        /**
         * Set a [CoroutineDispatcher] for user callbacks (onInsert, onDelete, onUpdate,
         * onConnect, reducer callbacks, etc.). When set, all user callbacks are dispatched
         * via [withContext] to this dispatcher. When not set (the default), callbacks run
         * on the receive-loop thread ([kotlinx.coroutines.Dispatchers.Default]).
         *
         * Android example: `withCallbackDispatcher(Dispatchers.Main)`
         */
        public fun withCallbackDispatcher(dispatcher: CoroutineDispatcher): Builder =
            apply { this.callbackDispatcher = dispatcher }

        /**
         * Register the generated module bindings.
         * The generated `withModuleBindings()` extension calls this automatically.
         */
        @InternalSpacetimeApi
        public fun withModule(descriptor: ModuleDescriptor): Builder = apply { module = descriptor }

        /** Registers a callback invoked when the connection is established. */
        public fun onConnect(cb: (DbConnectionView, Identity, String) -> Unit): Builder =
            apply { onConnectCallbacks.add(cb) }

        /** Registers a callback invoked when the connection is closed. */
        public fun onDisconnect(cb: (DbConnectionView, Throwable?) -> Unit): Builder =
            apply { onDisconnectCallbacks.add(cb) }

        /** Registers a callback invoked when a connection attempt fails. */
        public fun onConnectError(cb: (DbConnectionView, Throwable) -> Unit): Builder =
            apply { onConnectErrorCallbacks.add(cb) }

        /** Builds and connects the [DbConnection]. Suspends until the WebSocket handshake completes. */
        public suspend fun build(): DbConnection {
            module?.let { ensureMinimumVersion(it.cliVersion) }
            require(compression in availableCompressionModes) {
                "Compression mode $compression is not supported on this platform. " +
                        "Available modes: $availableCompressionModes"
            }
            val resolvedUri = requireNotNull(uri) { "URI is required" }
            val resolvedModule = requireNotNull(nameOrAddress) { "Module name is required" }
            val resolvedClient = requireNotNull(httpClient) { "HttpClient is required. Call withHttpClient() on the builder." }
            val clientConnectionId = ConnectionId.random()
            val stats = Stats()

            val transport = SpacetimeTransport(
                client = resolvedClient,
                baseUrl = resolvedUri,
                nameOrAddress = resolvedModule,
                connectionId = clientConnectionId,
                authToken = authToken,
                compression = compression,
                lightMode = lightMode,
                confirmedReads = confirmedReads,
            )

            val scope = CoroutineScope(SupervisorJob())

            val conn = DbConnection(
                transport = transport,
                scope = scope,
                onConnectCallbacks = onConnectCallbacks,
                onDisconnectCallbacks = onDisconnectCallbacks,
                onConnectErrorCallbacks = onConnectErrorCallbacks,
                clientConnectionId = clientConnectionId,
                stats = stats,
                moduleDescriptor = module,
                callbackDispatcher = callbackDispatcher,
            )

            module?.let {
                it.registerTables(conn.clientCache)
                val accessors = it.createAccessors(conn)
                conn.moduleTables = accessors.tables
                conn.moduleReducers = accessors.reducers
                conn.moduleProcedures = accessors.procedures
            }
            conn.connect()

            return conn
        }

    }
}

/**
 * Executes [block] with this [DbConnection], then calls [disconnect] when done.
 * Ensures cleanup even if [block] throws or the coroutine is cancelled.
 */
public suspend inline fun <R> DbConnection.use(block: (DbConnection) -> R): R {
    try {
        return block(this)
    } finally {
        withContext(NonCancellable) {
            disconnect()
        }
    }
}

/** Marker interface for generated table accessors. */
public interface ModuleTables

/** Marker interface for generated reducer accessors. */
public interface ModuleReducers

/** Marker interface for generated procedure accessors. */
public interface ModuleProcedures

/** Accessor instances created by [ModuleDescriptor.createAccessors]. */
@InternalSpacetimeApi
public data class ModuleAccessors(
    public val tables: ModuleTables,
    public val reducers: ModuleReducers,
    public val procedures: ModuleProcedures,
)

/**
 * Describes a generated SpacetimeDB module's bindings.
 * Implemented by the generated code to register tables and dispatch reducer events.
 */
@InternalSpacetimeApi
public interface ModuleDescriptor {
    public val cliVersion: String
    /** Names of persistent (subscribable) tables. Event tables are excluded. */
    public val subscribableTableNames: List<String>
    public fun registerTables(cache: ClientCache)
    public fun createAccessors(conn: DbConnection): ModuleAccessors
    public fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>)
}

private val MINIMUM_CLI_VERSION = intArrayOf(2, 0, 0)

private fun parseVersion(version: String): IntArray {
    val parts = version.split("-")[0].split(".")
    return intArrayOf(
        parts.getOrNull(0)?.toIntOrNull() ?: 0,
        parts.getOrNull(1)?.toIntOrNull() ?: 0,
        parts.getOrNull(2)?.toIntOrNull() ?: 0,
    )
}

private fun ensureMinimumVersion(cliVersion: String) {
    val parsed = parseVersion(cliVersion)
    for (i in 0..2) {
        if (parsed[i] > MINIMUM_CLI_VERSION[i]) return
        if (parsed[i] < MINIMUM_CLI_VERSION[i]) {
            val min = MINIMUM_CLI_VERSION.joinToString(".")
            throw IllegalStateException(
                "Module bindings were generated with spacetimedb cli $cliVersion, " +
                        "but this SDK requires at least $min. " +
                        "Regenerate bindings with an updated CLI: spacetime generate"
            )
        }
    }
}
