@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QuerySetId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ReducerOutcome
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TransactionUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.UnsubscribeFlags
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.SpacetimeTransport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import io.ktor.client.HttpClient
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.getAndUpdate
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentHashMapOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

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
enum class CompressionMode(internal val wireValue: String) {
    GZIP("Gzip"),
    BROTLI("Brotli"),
    NONE("None"),
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
open class DbConnection private constructor(
    private val transport: SpacetimeTransport,
    private val scope: CoroutineScope,
    private val onConnectCallbacks: MutableList<(DbConnection, Identity, String) -> Unit>,
    private val onDisconnectCallbacks: MutableList<(DbConnection, Throwable?) -> Unit>,
    private val onConnectErrorCallbacks: MutableList<(DbConnection, Throwable) -> Unit>,
    private val clientConnectionId: ConnectionId,
    val stats: Stats,
    private val moduleDescriptor: ModuleDescriptor?,
) {
    val clientCache = ClientCache()

    var moduleTables: ModuleTables? = null
        internal set
    var moduleReducers: ModuleReducers? = null
        internal set
    var moduleProcedures: ModuleProcedures? = null
        internal set

    var identity: Identity? = null
        private set
    var connectionId: ConnectionId? = null
        private set
    var token: String? = null
        private set
    var isActive: Boolean = false
        private set

    private val mutex = Mutex()
    private var nextQuerySetId: UInt = 0u
    private val subscriptions = atomic(persistentHashMapOf<UInt, SubscriptionHandle>())
    private val reducerCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Reducer<*>) -> Unit>())
    private val reducerCallInfo = atomic(persistentHashMapOf<UInt, ReducerCallInfo>())
    private val procedureCallbacks =
        atomic(persistentHashMapOf<UInt, (EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit>())
    private val oneOffQueryCallbacks =
        atomic(persistentHashMapOf<UInt, (ServerMessage.OneOffQueryResult) -> Unit>())
    private val querySetIdToRequestId = atomic(persistentHashMapOf<UInt, UInt>())
    private val outboundQueue = mutableListOf<ClientMessage>()
    private var receiveJob: Job? = null
    private var eventId: Long = 0
    private var onConnectInvoked = false

    // --- Multiple connection callbacks ---

    fun onConnect(cb: (DbConnection, Identity, String) -> Unit) {
        onConnectCallbacks.add(cb)
    }

    fun removeOnConnect(cb: (DbConnection, Identity, String) -> Unit) {
        onConnectCallbacks.remove(cb)
    }

    fun onDisconnect(cb: (DbConnection, Throwable?) -> Unit) {
        onDisconnectCallbacks.add(cb)
    }

    fun removeOnDisconnect(cb: (DbConnection, Throwable?) -> Unit) {
        onDisconnectCallbacks.remove(cb)
    }

    fun onConnectError(cb: (DbConnection, Throwable) -> Unit) {
        onConnectErrorCallbacks.add(cb)
    }

    fun removeOnConnectError(cb: (DbConnection, Throwable) -> Unit) {
        onConnectErrorCallbacks.remove(cb)
    }

    private fun nextEventId(): String {
        eventId++
        return "${connectionId?.toHexString() ?: clientConnectionId.toHexString()}:$eventId"
    }

    /**
     * Connect to SpacetimeDB and start the message receive loop.
     */
    suspend fun connect() {
        Logger.info { "Connecting to SpacetimeDB..." }
        transport.connect()
        isActive = true

        // Flush queued messages
        mutex.withLock {
            for (msg in outboundQueue) {
                transport.send(msg)
            }
            outboundQueue.clear()
        }

        // Start receive loop
        receiveJob = scope.launch {
            try {
                transport.incoming().collect { message ->
                    val applyStart = kotlin.time.TimeSource.Monotonic.markNow()
                    processMessage(message)
                    stats.applyMessageTracker.insertSample(applyStart.elapsedNow())
                }
            } catch (e: Exception) {
                Logger.error { "Connection error: ${e.message}" }
                isActive = false
                for (cb in onDisconnectCallbacks) cb(this@DbConnection, e)
            }
        }
    }

    fun disconnect() {
        Logger.info { "Disconnecting from SpacetimeDB" }
        isActive = false
        scope.launch {
            try {
                transport.disconnect()
                receiveJob?.join()
                receiveJob = null
            } finally {
                clientCache.clear()
                for (cb in onDisconnectCallbacks) cb(this@DbConnection, null)
            }
        }
    }

    // --- Subscription Builder ---

    fun subscriptionBuilder(): SubscriptionBuilder = SubscriptionBuilder(this)

    fun subscribeToAllTables(): SubscriptionHandle {
        return subscriptionBuilder().subscribeToAllTables()
    }

    // --- Subscriptions ---

    /**
     * Subscribe to a set of SQL queries.
     * Returns a SubscriptionHandle to track the subscription lifecycle.
     */
    fun subscribe(
        queries: List<String>,
        onApplied: List<(EventContext.SubscribeApplied) -> Unit> = emptyList(),
        onError: List<(EventContext.Error, Throwable) -> Unit> = emptyList(),
    ): SubscriptionHandle {
        val requestId = stats.subscriptionRequestTracker.startTrackingRequest()
        nextQuerySetId++
        val querySetId = QuerySetId(nextQuerySetId)
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

    fun subscribe(vararg queries: String): SubscriptionHandle =
        subscribe(queries.toList())

    internal fun unsubscribe(handle: SubscriptionHandle) {
        val requestId = stats.subscriptionRequestTracker.startTrackingRequest()
        val message = ClientMessage.Unsubscribe(
            requestId = requestId,
            querySetId = handle.querySetId,
            flags = UnsubscribeFlags.Default,
        )
        sendMessage(message)
    }

    // --- Reducers ---

    /**
     * Call a reducer on the server.
     * The encodedArgs should be BSATN-encoded reducer arguments.
     * The typedArgs is the typed args object stored for the event context.
     */
    fun <A> callReducer(
        reducerName: String,
        encodedArgs: ByteArray,
        typedArgs: A,
        callback: ((EventContext.Reducer<A>) -> Unit)? = null,
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
            flags = 0u,
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
    fun callProcedure(
        procedureName: String,
        args: ByteArray,
        callback: ((EventContext.Procedure, ServerMessage.ProcedureResultMsg) -> Unit)? = null,
    ): UInt {
        val requestId = stats.procedureRequestTracker.startTrackingRequest(procedureName)
        if (callback != null) {
            procedureCallbacks.update { it.put(requestId, callback) }
        }
        val message = ClientMessage.CallProcedure(
            requestId = requestId,
            flags = 0u,
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
    fun oneOffQuery(
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

    // --- Internal ---

    private fun sendMessage(message: ClientMessage) {
        if (!isActive) {
            outboundQueue.add(message)
            return
        }
        scope.launch {
            mutex.withLock {
                transport.send(message)
            }
        }
    }

    private fun processMessage(message: ServerMessage) {
        when (message) {
            is ServerMessage.InitialConnection -> {
                // Validate identity consistency (matching C# SDK)
                val currentIdentity = identity
                if (currentIdentity != null && currentIdentity != message.identity) {
                    val error = IllegalStateException(
                        "Server returned unexpected identity: ${message.identity}, expected: $currentIdentity"
                    )
                    for (cb in onConnectErrorCallbacks) cb(this, error)
                    return
                }

                identity = message.identity
                connectionId = message.connectionId
                if (token == null && message.token.isNotEmpty()) {
                    token = message.token
                }
                Logger.info { "Connected with identity=${message.identity}" }
                // Guard: only fire onConnect once (matching TS/C# SDKs)
                if (!onConnectInvoked) {
                    onConnectInvoked = true
                    for (cb in onConnectCallbacks) cb(this, message.identity, message.token)
                    onConnectCallbacks.clear()
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
                for (cb in callbacks) cb.invoke()
            }

            is ServerMessage.UnsubscribeApplied -> {
                val handle = subscriptions.value[message.querySetId.id] ?: return
                val ctx = EventContext.UnsubscribeApplied(id = nextEventId(), connection = this)

                val callbacks = mutableListOf<PendingCallback>()
                if (message.rows != null) {
                    // Phase 1: PreApply ALL tables (fire onBeforeDelete before mutations)
                    for (tableRows in message.rows.tables) {
                        val table = clientCache.getUntypedTable(tableRows.table) ?: continue
                        table.preApplyDeletes(ctx, tableRows.rows)
                    }
                    // Phase 2: Apply ALL tables (mutate + collect post-callbacks)
                    for (tableRows in message.rows.tables) {
                        val table = clientCache.getUntypedTable(tableRows.table) ?: continue
                        callbacks.addAll(table.applyDeletes(ctx, tableRows.rows))
                    }
                }

                handle.handleEnd(ctx)
                subscriptions.update { it.remove(message.querySetId.id) }
                // Phase 3: Fire post-mutation callbacks
                for (cb in callbacks) cb.invoke()
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
                for (cb in callbacks) cb.invoke()
            }

            is ServerMessage.ReducerResultMsg -> {
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
                                callerIdentity = identity!!,
                                callerConnectionId = connectionId,
                            )
                        } else {
                            EventContext.UnknownTransaction(id = nextEventId(), connection = this)
                        }
                        val callbacks = applyTransactionUpdate(ctx, result.transactionUpdate)
                        for (cb in callbacks) cb.invoke()

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
                                callerIdentity = identity!!,
                                callerConnectionId = connectionId,
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
                                callerIdentity = identity!!,
                                callerConnectionId = connectionId,
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
                                callerIdentity = identity!!,
                                callerConnectionId = connectionId,
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
                cb?.let {
                    val procedureEvent = ProcedureEvent(
                        timestamp = message.timestamp,
                        status = message.status,
                        callerIdentity = identity!!,
                        callerConnectionId = connectionId,
                        totalHostExecutionDuration = message.totalHostExecutionDuration,
                        requestId = message.requestId,
                    )
                    val ctx = EventContext.Procedure(
                        id = nextEventId(),
                        connection = this,
                        event = procedureEvent
                    )
                    it.invoke(ctx, message)
                }
            }

            is ServerMessage.OneOffQueryResult -> {
                stats.oneOffRequestTracker.finishTrackingRequest(message.requestId)
                var cb: ((ServerMessage.OneOffQueryResult) -> Unit)? = null
                oneOffQueryCallbacks.getAndUpdate { map ->
                    cb = map[message.requestId]
                    map.remove(message.requestId)
                }
                cb?.invoke(message)
            }
        }
    }

    private fun fireReducerCallbacks(requestId: UInt, ctx: EventContext.Reducer<*>) {
        var cb: ((EventContext.Reducer<*>) -> Unit)? = null
        reducerCallbacks.getAndUpdate { map ->
            cb = map[requestId]
            map.remove(requestId)
        }
        cb?.invoke(ctx)
        moduleDescriptor?.handleReducerEvent(this, ctx)
    }

    private fun applyTransactionUpdate(
        ctx: EventContext,
        update: TransactionUpdate,
    ): List<PendingCallback> {
        // Collect all (table, rows) pairs
        val allUpdates = mutableListOf<Pair<TableCache<*, *>, TableUpdateRows>>()
        for (querySetUpdate in update.querySets) {
            for (tableUpdate in querySetUpdate.tables) {
                val table = clientCache.getUntypedTable(tableUpdate.tableName) ?: continue
                for (rows in tableUpdate.rows) {
                    allUpdates.add(table to rows)
                }
            }
        }

        // Phase 1: PreApply ALL tables (fire onBeforeDelete before any mutations)
        for ((table, rows) in allUpdates) {
            table.preApplyUpdate(ctx, rows)
        }

        // Phase 2: Apply ALL tables (mutate + collect post-callbacks)
        val allCallbacks = mutableListOf<PendingCallback>()
        for ((table, rows) in allUpdates) {
            allCallbacks.addAll(table.applyUpdate(ctx, rows))
        }

        return allCallbacks
    }

    // --- Builder ---

    class Builder {
        private var httpClient: HttpClient? = null
        private var uri: String? = null
        private var nameOrAddress: String? = null
        private var authToken: String? = null
        private var compression: CompressionMode = CompressionMode.GZIP
        private var lightMode: Boolean = false
        private var confirmedReads: Boolean? = null
        private val onConnectCallbacks = mutableListOf<(DbConnection, Identity, String) -> Unit>()
        private val onDisconnectCallbacks = mutableListOf<(DbConnection, Throwable?) -> Unit>()
        private val onConnectErrorCallbacks = mutableListOf<(DbConnection, Throwable) -> Unit>()
        private var module: ModuleDescriptor? = null

        fun withHttpClient(client: HttpClient): Builder = apply { httpClient = client }
        fun withUri(uri: String): Builder = apply { this.uri = uri }
        fun withDatabaseName(nameOrAddress: String): Builder =
            apply { this.nameOrAddress = nameOrAddress }

        fun withToken(token: String?): Builder = apply { authToken = token }
        fun withCompression(compression: CompressionMode): Builder =
            apply { this.compression = compression }

        fun withLightMode(lightMode: Boolean): Builder = apply { this.lightMode = lightMode }
        fun withConfirmedReads(confirmed: Boolean): Builder = apply { confirmedReads = confirmed }

        /**
         * Register the generated module bindings.
         * The generated `withModuleBindings()` extension calls this automatically.
         */
        fun withModule(descriptor: ModuleDescriptor): Builder = apply { module = descriptor }

        fun onConnect(cb: (DbConnection, Identity, String) -> Unit): Builder =
            apply { onConnectCallbacks.add(cb) }

        fun onDisconnect(cb: (DbConnection, Throwable?) -> Unit): Builder =
            apply { onDisconnectCallbacks.add(cb) }

        fun onConnectError(cb: (DbConnection, Throwable) -> Unit): Builder =
            apply { onConnectErrorCallbacks.add(cb) }

        suspend fun build(): DbConnection {
            module?.let { ensureMinimumVersion(it.cliVersion) }
            val resolvedUri = requireNotNull(uri) { "URI is required" }
            val resolvedModule = requireNotNull(nameOrAddress) { "Module name is required" }
            val resolvedClient = httpClient ?: createDefaultHttpClient()
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
            }
        }
    }
}

/**
 * Exception thrown when a reducer call fails.
 */
class ReducerException(
    message: String,
    reducerName: String? = null,
) : Exception(if (reducerName != null) "Reducer '$reducerName' failed: $message" else message)

/** Marker interface for generated table accessors. */
interface ModuleTables

/** Marker interface for generated reducer accessors. */
interface ModuleReducers

/** Marker interface for generated procedure accessors. */
interface ModuleProcedures

/** Accessor instances created by [ModuleDescriptor.createAccessors]. */
data class ModuleAccessors(
    val tables: ModuleTables,
    val reducers: ModuleReducers,
    val procedures: ModuleProcedures,
)

/**
 * Describes a generated SpacetimeDB module's bindings.
 * Implemented by the generated code to register tables and dispatch reducer events.
 */
interface ModuleDescriptor {
    val cliVersion: String
    fun registerTables(cache: ClientCache)
    fun createAccessors(conn: DbConnection): ModuleAccessors
    fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>)
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
