package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
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
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume

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
    GZIP("Gzip"),
    NONE("None"),
}

/**
 * Connection lifecycle state (matches C#'s isClosing/connectionClosed pattern as a single enum).
 */
public enum class ConnectionState {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    CLOSED,
}

/**
 * Main entry point for connecting to a SpacetimeDB module.
 * Mirrors TS SDK's DbConnectionImpl.
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
    private val httpClient: HttpClient,
    private val scope: CoroutineScope,
    onConnectCallbacks: List<(DbConnection, Identity, String) -> Unit>,
    onDisconnectCallbacks: List<(DbConnection, Throwable?) -> Unit>,
    onConnectErrorCallbacks: List<(DbConnection, Throwable) -> Unit>,
    private val clientConnectionId: ConnectionId,
    public val stats: Stats,
    private val moduleDescriptor: ModuleDescriptor?,
    private val callbackDispatcher: CoroutineDispatcher?,
) {
    public val clientCache: ClientCache = ClientCache()

    private val _moduleTables = atomic<ModuleTables?>(null)
    public var moduleTables: ModuleTables?
        get() = _moduleTables.value
        internal set(value) { _moduleTables.value = value }

    private val _moduleReducers = atomic<ModuleReducers?>(null)
    public var moduleReducers: ModuleReducers?
        get() = _moduleReducers.value
        internal set(value) { _moduleReducers.value = value }

    private val _moduleProcedures = atomic<ModuleProcedures?>(null)
    public var moduleProcedures: ModuleProcedures?
        get() = _moduleProcedures.value
        internal set(value) { _moduleProcedures.value = value }

    private val _identity = atomic<Identity?>(null)
    public var identity: Identity?
        get() = _identity.value
        private set(value) { _identity.value = value }

    private val _connectionId = atomic<ConnectionId?>(null)
    public var connectionId: ConnectionId?
        get() = _connectionId.value
        private set(value) { _connectionId.value = value }

    private val _token = atomic<String?>(null)
    public var token: String?
        get() = _token.value
        private set(value) { _token.value = value }

    private val _state = atomic(ConnectionState.DISCONNECTED)
    public val isActive: Boolean get() = _state.value == ConnectionState.CONNECTED

    private val sendChannel = Channel<ClientMessage>(Channel.UNLIMITED)
    private val _sendJob = atomic<Job?>(null)
    private val _nextQuerySetId = atomic(0)
    private val subscriptions = atomic(persistentHashMapOf<UInt, SubscriptionHandle>())
    private val reducerCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Reducer<*>) -> Unit>())
    private val reducerCallInfo = atomic(persistentHashMapOf<UInt, ReducerCallInfo>())
    private val procedureCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit>())
    private val oneOffQueryCallbacks =
        atomic(persistentHashMapOf<UInt, (ServerMessage.OneOffQueryResult) -> Unit>())
    private val querySetIdToRequestId = atomic(persistentHashMapOf<UInt, UInt>())
    private val _receiveJob = atomic<Job?>(null)
    private val _eventId = atomic(0L)
    private val _onConnectInvoked = atomic(false)
    private val _onConnectCallbacks = atomic(onConnectCallbacks.toPersistentList())
    private val _onDisconnectCallbacks = atomic(onDisconnectCallbacks.toPersistentList())
    private val _onConnectErrorCallbacks = atomic(onConnectErrorCallbacks.toPersistentList())

    // --- Multiple connection callbacks ---

    public fun onConnect(cb: (DbConnection, Identity, String) -> Unit) {
        // Add first, then check — avoids TOCTOU race where the receive loop
        // drains the list between our check and add.
        _onConnectCallbacks.update { it.add(cb) }
        if (_onConnectInvoked.value) {
            // Already connected — drain and fire. getAndSet ensures each
            // callback is claimed by exactly one thread (us or the receive loop).
            val cbs = _onConnectCallbacks.getAndSet(persistentListOf())
            val id = identity
            val tok = token
            if (id == null || tok == null) {
                Logger.error { "onConnect called after connection but identity or token is null" }
                return
            }
            scope.launch {
                for (c in cbs) runUserCallback { c(this@DbConnection, id, tok) }
            }
        }
    }

    public fun removeOnConnect(cb: (DbConnection, Identity, String) -> Unit) {
        _onConnectCallbacks.update { it.remove(cb) }
    }

    public fun onDisconnect(cb: (DbConnection, Throwable?) -> Unit) {
        _onDisconnectCallbacks.update { it.add(cb) }
    }

    public fun removeOnDisconnect(cb: (DbConnection, Throwable?) -> Unit) {
        _onDisconnectCallbacks.update { it.remove(cb) }
    }

    public fun onConnectError(cb: (DbConnection, Throwable) -> Unit) {
        _onConnectErrorCallbacks.update { it.add(cb) }
    }

    public fun removeOnConnectError(cb: (DbConnection, Throwable) -> Unit) {
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
     * and the connection transitions to [ConnectionState.CLOSED].
     * No exception is thrown — errors are reported via callbacks
     * (matching C# and TS SDK behavior).
     */
    internal suspend fun connect() {
        check(_state.value != ConnectionState.CLOSED) {
            "Connection is closed. Create a new DbConnection to reconnect."
        }
        check(_state.compareAndSet(ConnectionState.DISCONNECTED, ConnectionState.CONNECTING)) {
            "connect() called in invalid state: ${_state.value}"
        }
        Logger.info { "Connecting to SpacetimeDB..." }
        try {
            transport.connect()
        } catch (e: Exception) {
            _state.value = ConnectionState.CLOSED
            httpClient.close()
            scope.cancel()
            for (cb in _onConnectErrorCallbacks.value) runUserCallback { cb(this, e) }
            return
        }

        _state.value = ConnectionState.CONNECTED

        // Start sender coroutine — drains any buffered messages in FIFO order
        _sendJob.value = scope.launch {
            for (msg in sendChannel) {
                transport.send(msg)
            }
        }

        // Start receive loop
        _receiveJob.value = scope.launch {
            try {
                transport.incoming().collect { message ->
                    val applyStart = kotlin.time.TimeSource.Monotonic.markNow()
                    processMessage(message)
                    stats.applyMessageTracker.insertSample(applyStart.elapsedNow())
                }
                // Normal completion — server closed the connection
                _state.value = ConnectionState.CLOSED
                failPendingOperations()
                for (cb in _onDisconnectCallbacks.value) runUserCallback { cb(this@DbConnection, null) }
            } catch (e: Exception) {
                currentCoroutineContext().ensureActive()
                Logger.error { "Connection error: ${e.message}" }
                _state.value = ConnectionState.CLOSED
                failPendingOperations()
                for (cb in _onDisconnectCallbacks.value) runUserCallback { cb(this@DbConnection, e) }
            } finally {
                // Release resources so the JVM can exit (OkHttp connection pool threads)
                withContext(NonCancellable) {
                    sendChannel.close()
                    try { transport.disconnect() } catch (_: Exception) {}
                    httpClient.close()
                }
            }
        }
    }

    /**
     * Disconnect from SpacetimeDB and release all resources.
     * The connection cannot be reused — create a new [DbConnection] to reconnect.
     */
    public suspend fun disconnect() {
        val prev = _state.getAndSet(ConnectionState.CLOSED)
        if (prev != ConnectionState.CONNECTED && prev != ConnectionState.CONNECTING) return
        Logger.info { "Disconnecting from SpacetimeDB" }
        _receiveJob.getAndSet(null)?.cancel()
        _sendJob.getAndSet(null)?.cancel()
        failPendingOperations()
        clientCache.clear()
        for (cb in _onDisconnectCallbacks.value) runUserCallback { cb(this@DbConnection, null) }
        sendChannel.close()
        try { transport.disconnect() } catch (_: Exception) {}
        httpClient.close()
        scope.cancel()
    }

    /**
     * Fail all in-flight operations on disconnect (matches C#'s FailPendingOperations).
     * Clears callback maps so captured lambdas can be GC'd, and marks all
     * subscription handles as ENDED so callers don't try to use stale handles.
     */
    private fun failPendingOperations() {
        val pendingReducers = reducerCallbacks.getAndSet(persistentHashMapOf())
        reducerCallInfo.getAndSet(persistentHashMapOf())
        if (pendingReducers.isNotEmpty()) {
            Logger.warn { "Discarding ${pendingReducers.size} pending reducer callback(s) due to disconnect" }
        }

        val pendingProcedures = procedureCallbacks.getAndSet(persistentHashMapOf())
        if (pendingProcedures.isNotEmpty()) {
            Logger.warn { "Discarding ${pendingProcedures.size} pending procedure callback(s) due to disconnect" }
        }

        val pendingQueries = oneOffQueryCallbacks.getAndSet(persistentHashMapOf())
        if (pendingQueries.isNotEmpty()) {
            Logger.warn { "Discarding ${pendingQueries.size} pending one-off query callback(s) due to disconnect" }
        }

        querySetIdToRequestId.getAndSet(persistentHashMapOf())

        val pendingSubs = subscriptions.getAndSet(persistentHashMapOf())
        for ((_, handle) in pendingSubs) {
            handle.markEnded()
        }
    }

    // --- Subscription Builder ---

    public fun subscriptionBuilder(): SubscriptionBuilder = SubscriptionBuilder(this)

    public fun subscribeToAllTables(): SubscriptionHandle {
        return subscriptionBuilder().subscribeToAllTables()
    }

    // --- Subscriptions ---

    /**
     * Subscribe to a set of SQL queries.
     * Returns a SubscriptionHandle to track the subscription lifecycle.
     */
    public fun subscribe(
        queries: List<String>,
        onApplied: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
        onError: List<(EventContext.Error, Throwable) -> Unit> = emptyList(),
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
        sendMessage(message)
        return handle
    }

    public fun subscribe(vararg queries: String): SubscriptionHandle =
        subscribe(queries.toList())

    internal fun unsubscribe(handle: SubscriptionHandle, flags: UnsubscribeFlags) {
        val requestId = stats.subscriptionRequestTracker.startTrackingRequest()
        val message = ClientMessage.Unsubscribe(
            requestId = requestId,
            querySetId = handle.querySetId,
            flags = flags,
        )
        sendMessage(message)
    }

    // --- Reducers ---

    /**
     * Call a reducer on the server.
     * The encodedArgs should be BSATN-encoded reducer arguments.
     * The typedArgs is the typed args object stored for the event context.
     */
    public fun <A> callReducer(
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
        sendMessage(message)
        return requestId
    }

    // --- Procedures ---

    /**
     * Call a procedure on the server.
     * The args should be BSATN-encoded procedure arguments.
     */
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
        sendMessage(message)
        return requestId
    }

    // --- One-Off Queries ---

    /**
     * Execute a one-off SQL query against the database.
     * The result callback receives the query result or error.
     */
    public fun oneOffQuery(
        queryString: String,
        callback: (ServerMessage.OneOffQueryResult) -> Unit,
    ): UInt {
        val requestId = stats.oneOffRequestTracker.startTrackingRequest()
        oneOffQueryCallbacks.update { it.put(requestId, callback) }
        val message = ClientMessage.OneOffQuery(
            requestId = requestId,
            queryString = queryString,
        )
        Logger.debug { "Executing one-off query (requestId=$requestId)" }
        sendMessage(message)
        return requestId
    }

    /**
     * Execute a one-off SQL query against the database, suspending until the result is available.
     */
    public suspend fun oneOffQuery(queryString: String): ServerMessage.OneOffQueryResult =
        suspendCancellableCoroutine { cont ->
            val requestId = oneOffQuery(queryString) { result ->
                cont.resume(result)
            }
            cont.invokeOnCancellation {
                oneOffQueryCallbacks.update { it.remove(requestId) }
            }
        }

    // --- Internal ---

    private fun sendMessage(message: ClientMessage) {
        val result = sendChannel.trySend(message)
        if (result.isClosed) {
            Logger.warn { "Message dropped (connection closed): $message" }
        }
    }

    private suspend fun processMessage(message: ServerMessage) {
        when (message) {
            is ServerMessage.InitialConnection -> {
                // Validate identity consistency (matching C# SDK)
                val currentIdentity = identity
                if (currentIdentity != null && currentIdentity != message.identity) {
                    val error = IllegalStateException(
                        "Server returned unexpected identity: ${message.identity}, expected: $currentIdentity"
                    )
                    for (cb in _onConnectErrorCallbacks.value) runUserCallback { cb(this, error) }
                    return
                }

                identity = message.identity
                connectionId = message.connectionId
                if (token == null && message.token.isNotEmpty()) {
                    token = message.token
                }
                Logger.info { "Connected with identity=${message.identity}" }
                // One-shot: fire onConnect callbacks once, then discard (matches C# SDK)
                if (_onConnectInvoked.compareAndSet(expect = false, update = true)) {
                    val cbs = _onConnectCallbacks.getAndSet(persistentListOf())
                    for (cb in cbs) runUserCallback { cb(this, message.identity, message.token) }
                }
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

                handle.handleApplied(ctx)
                for (cb in callbacks) runUserCallback { cb.invoke() }
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

                handle.handleEnd(ctx)
                subscriptions.update { it.remove(message.querySetId.id) }
                // Phase 3: Fire post-mutation callbacks
                for (cb in callbacks) runUserCallback { cb.invoke() }
            }

            is ServerMessage.SubscriptionError -> {
                val handle = subscriptions.value[message.querySetId.id] ?: run {
                    Logger.warn { "Received SubscriptionError for unknown querySetId=${message.querySetId.id}" }
                    return
                }
                val error = Exception(message.error)
                val ctx = EventContext.Error(id = nextEventId(), connection = this, error = error)
                Logger.error { "Subscription error: ${message.error}" }
                var subRequestId: UInt? = null
                querySetIdToRequestId.getAndUpdate { map ->
                    subRequestId = map[message.querySetId.id]
                    map.remove(message.querySetId.id)
                }
                subRequestId?.let { stats.subscriptionRequestTracker.finishTrackingRequest(it) }

                if (message.requestId == null) {
                    handle.handleError(ctx, error)
                    disconnect()
                    return
                }

                handle.handleError(ctx, error)
                subscriptions.update { it.remove(message.querySetId.id) }
            }

            is ServerMessage.TransactionUpdateMsg -> {
                val ctx = EventContext.Transaction(id = nextEventId(), connection = this)
                val callbacks = applyTransactionUpdate(ctx, message.update)
                for (cb in callbacks) runUserCallback { cb.invoke() }
            }

            is ServerMessage.ReducerResultMsg -> {
                val callerIdentity = identity ?: run {
                    Logger.error { "Received ReducerResultMsg before identity was set" }
                    return
                }
                val callerConnId = connectionId
                val result = message.result
                var info: ReducerCallInfo? = null
                reducerCallInfo.getAndUpdate { map ->
                    info = map[message.requestId]
                    map.remove(message.requestId)
                }
                stats.reducerRequestTracker.finishTrackingRequest(message.requestId)
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
                val procIdentity = identity ?: run {
                    Logger.error { "Received ProcedureResultMsg before identity was set" }
                    return
                }
                val procConnId = connectionId
                stats.procedureRequestTracker.finishTrackingRequest(message.requestId)
                var cb: ((EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit)? = null
                procedureCallbacks.getAndUpdate { map ->
                    cb = map[message.requestId]
                    map.remove(message.requestId)
                }
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
                var cb: ((ServerMessage.OneOffQueryResult) -> Unit)? = null
                oneOffQueryCallbacks.getAndUpdate { map ->
                    cb = map[message.requestId]
                    map.remove(message.requestId)
                }
                cb?.let { runUserCallback { it.invoke(message) } }
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

    public class Builder {
        private var uri: String? = null
        private var nameOrAddress: String? = null
        private var authToken: String? = null
        private var compression: CompressionMode = defaultCompressionMode
        private var lightMode: Boolean = false
        private var confirmedReads: Boolean? = null
        private val onConnectCallbacks = atomic(persistentListOf<(DbConnection, Identity, String) -> Unit>())
        private val onDisconnectCallbacks = atomic(persistentListOf<(DbConnection, Throwable?) -> Unit>())
        private val onConnectErrorCallbacks = atomic(persistentListOf<(DbConnection, Throwable) -> Unit>())
        private var module: ModuleDescriptor? = null
        private var callbackDispatcher: CoroutineDispatcher? = null

        public fun withUri(uri: String): Builder = apply { this.uri = uri }
        public fun withDatabaseName(nameOrAddress: String): Builder =
            apply { this.nameOrAddress = nameOrAddress }

        public fun withToken(token: String?): Builder = apply { authToken = token }
        public fun withCompression(compression: CompressionMode): Builder =
            apply { this.compression = compression }

        public fun withLightMode(lightMode: Boolean): Builder = apply { this.lightMode = lightMode }
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
        public fun withModule(descriptor: ModuleDescriptor): Builder = apply { module = descriptor }

        public fun onConnect(cb: (DbConnection, Identity, String) -> Unit): Builder =
            apply { onConnectCallbacks.update { it.add(cb) } }

        public fun onDisconnect(cb: (DbConnection, Throwable?) -> Unit): Builder =
            apply { onDisconnectCallbacks.update { it.add(cb) } }

        public fun onConnectError(cb: (DbConnection, Throwable) -> Unit): Builder =
            apply { onConnectErrorCallbacks.update { it.add(cb) } }

        public suspend fun build(): DbConnection {
            module?.let { ensureMinimumVersion(it.cliVersion) }
            require(compression in availableCompressionModes) {
                "Compression mode $compression is not supported on this platform. " +
                        "Available modes: $availableCompressionModes"
            }
            val resolvedUri = requireNotNull(uri) { "URI is required" }
            val resolvedModule = requireNotNull(nameOrAddress) { "Module name is required" }
            val resolvedClient = createDefaultHttpClient()
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
                httpClient = resolvedClient,
                scope = scope,
                onConnectCallbacks = onConnectCallbacks.value,
                onDisconnectCallbacks = onDisconnectCallbacks.value,
                onConnectErrorCallbacks = onConnectErrorCallbacks.value,
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

        private fun createDefaultHttpClient(): HttpClient {
            return HttpClient {
                install(io.ktor.client.plugins.websocket.WebSockets)
                install(io.ktor.client.plugins.HttpTimeout) {
                    connectTimeoutMillis = 10_000
                }
            }
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
public data class ModuleAccessors(
    public val tables: ModuleTables,
    public val reducers: ModuleReducers,
    public val procedures: ModuleProcedures,
)

/**
 * Describes a generated SpacetimeDB module's bindings.
 * Implemented by the generated code to register tables and dispatch reducer events.
 */
public interface ModuleDescriptor {
    public val cliVersion: String
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
