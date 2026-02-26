package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.protocol.*
import com.clockworklabs.spacetimedb.websocket.ConnectionState
import com.clockworklabs.spacetimedb.websocket.WebSocketTransport
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.atomicfu.atomic

/** Called when a connection is established. Receives the connection, the user's [Identity], and an auth token. */
typealias ConnectCallback = (DbConnection, Identity, String) -> Unit
/** Called when a connection is lost. The [Throwable] is null for clean disconnects. */
typealias DisconnectCallback = (DbConnection, Throwable?) -> Unit
/** Called when the initial connection attempt fails. */
typealias ConnectErrorCallback = (Throwable) -> Unit

/**
 * Primary client for interacting with a SpacetimeDB module.
 *
 * Create instances via [DbConnection.builder]:
 * ```kotlin
 * val conn = DbConnection.builder()
 *     .withUri("ws://localhost:3000")
 *     .withModuleName("my_module")
 *     .onConnect { conn, identity, token -> println("Connected as $identity") }
 *     .build()
 * ```
 *
 * The connection is opened immediately on [build][DbConnectionBuilder.build]. Use [disconnect]
 * to tear it down, or configure automatic reconnection via [DbConnectionBuilder.withReconnectPolicy].
 */
class DbConnection internal constructor(
    private val uri: String,
    private val moduleName: String,
    private val token: String?,
    private val connectCallbacks: List<ConnectCallback>,
    private val disconnectCallbacks: List<DisconnectCallback>,
    private val connectErrorCallbacks: List<ConnectErrorCallback>,
    private val keepAliveIntervalMs: Long = 30_000L,
    private val reconnectPolicy: ReconnectPolicy? = null,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val requestCounter = atomic(0)
    private val mutex = Mutex()

    internal val clientCache = ClientCache()
    private val tableHandles = mutableMapOf<String, TableHandle>()
    private val subscriptions = mutableMapOf<UInt, SubscriptionHandle>()
    private val subscriptionsByQuerySet = mutableMapOf<QuerySetId, SubscriptionHandle>()
    private val reducerCallbacks = mutableMapOf<UInt, (ReducerResult) -> Unit>()
    private val pendingOneOffQueries = mutableMapOf<UInt, CompletableDeferred<ServerMessage.OneOffQueryResult>>()

    var identity: Identity? = null
        private set
    var connectionId: ConnectionId? = null
        private set
    var savedToken: String? = null
        private set

    private val transport = WebSocketTransport(
        scope = scope,
        onMessage = { handleMessage(it) },
        onConnect = {},
        onDisconnect = { error ->
            failPendingOperations()
            disconnectCallbacks.forEach { it(this, error) }
        },
        onConnectError = { error -> connectErrorCallbacks.forEach { it(error) } },
        keepAliveIntervalMs = keepAliveIntervalMs,
        reconnectPolicy = reconnectPolicy,
    )

    val connectionState: StateFlow<ConnectionState> get() = transport.state
    val isActive: Boolean get() = transport.state.value == ConnectionState.CONNECTED

    init {
        transport.connect(uri, moduleName, token)
    }

    /** Closes the connection, cancels pending operations, and stops any reconnection attempts. */
    fun disconnect() {
        transport.disconnect()
        failPendingOperations()
        scope.cancel()
    }

    /** Returns the [TableHandle] for [name], creating it if needed. Register callbacks before connecting. */
    fun table(name: String): TableHandle {
        // tableHandles is only read/written from user thread (registration)
        // and from handleMessage under mutex (firing callbacks).
        // Reads from handleMessage never mutate, so this is safe for the
        // typical pattern of registering table handles before connecting.
        return tableHandles.getOrPut(name) { TableHandle(name) }
    }

    /** Creates a [SubscriptionBuilder] for subscribing to SQL queries on this connection. */
    fun subscriptionBuilder(): SubscriptionBuilder = SubscriptionBuilder(this)

    /** Invokes a server-side reducer by name with BSATN-encoded [args]. Optionally receives the [ReducerResult]. */
    fun callReducer(reducerName: String, args: ByteArray, callback: ((ReducerResult) -> Unit)? = null) {
        val reqId = nextRequestId()
        if (callback != null) {
            // Register synchronously before sending to avoid race with server response
            reducerCallbacks[reqId] = callback
        }
        transport.send(
            ClientMessage.CallReducer(
                requestId = reqId,
                reducer = reducerName,
                args = args,
            )
        )
    }

    /** Executes a one-off SQL query against the module and suspends until the result arrives. */
    suspend fun oneOffQuery(query: String): ServerMessage.OneOffQueryResult {
        val reqId = nextRequestId()
        val deferred = CompletableDeferred<ServerMessage.OneOffQueryResult>()
        mutex.withLock { pendingOneOffQueries[reqId] = deferred }
        transport.send(ClientMessage.OneOffQuery(requestId = reqId, queryString = query))
        return deferred.await()
    }

    /** Callback variant of [oneOffQuery] — launches a coroutine and invokes [callback] with the result. */
    fun oneOffQuery(query: String, callback: (ServerMessage.OneOffQueryResult) -> Unit) {
        val reqId = nextRequestId()
        val deferred = CompletableDeferred<ServerMessage.OneOffQueryResult>()
        scope.launch {
            mutex.withLock { pendingOneOffQueries[reqId] = deferred }
            transport.send(ClientMessage.OneOffQuery(requestId = reqId, queryString = query))
            callback(deferred.await())
        }
    }

    internal fun subscribe(
        queries: List<String>,
        handle: SubscriptionHandle,
    ): UInt {
        val reqId = nextRequestId()
        val qsId = QuerySetId(reqId)
        handle.querySetId = qsId
        handle.requestId = reqId
        // Register synchronously before sending to avoid race with server response
        subscriptions[reqId] = handle
        subscriptionsByQuerySet[qsId] = handle
        transport.send(
            ClientMessage.Subscribe(
                requestId = reqId,
                querySetId = qsId,
                queryStrings = queries,
            )
        )
        return reqId
    }

    internal fun unsubscribe(handle: SubscriptionHandle) {
        val qsId = handle.querySetId ?: return
        val reqId = nextRequestId()
        transport.send(
            ClientMessage.Unsubscribe(
                requestId = reqId,
                querySetId = qsId,
                flags = 1u, // SendDroppedRows — ensures server sends rows to remove from cache
            )
        )
    }

    private fun nextRequestId(): UInt = requestCounter.incrementAndGet().toUInt()

    private fun failPendingOperations() {
        val error = CancellationException("Connection closed")
        pendingOneOffQueries.values.forEach { it.cancel(error) }
        pendingOneOffQueries.clear()
        reducerCallbacks.clear()
    }

    private suspend fun handleMessage(msg: ServerMessage) {
        mutex.withLock {
            when (msg) {
                is ServerMessage.InitialConnection -> {
                    identity = msg.identity
                    connectionId = msg.connectionId
                    savedToken = msg.token
                    connectCallbacks.forEach { it(this, msg.identity, msg.token) }
                }

                is ServerMessage.SubscribeApplied -> {
                    clientCache.applySubscribeRows(msg.rows)
                    val handle = subscriptions[msg.requestId]
                    handle?.state = SubscriptionState.ACTIVE
                    handle?.onAppliedCallback?.invoke()
                }

                is ServerMessage.UnsubscribeApplied -> {
                    msg.rows?.let { clientCache.applyUnsubscribeRows(it) }
                    // Look up by querySetId since the requestId here is the unsubscribe requestId
                    val handle = subscriptionsByQuerySet[msg.querySetId]
                    handle?.state = SubscriptionState.ENDED
                    handle?.requestId?.let { subscriptions.remove(it) }
                    subscriptionsByQuerySet.remove(msg.querySetId)
                }

                is ServerMessage.SubscriptionError -> {
                    val handle = if (msg.requestId != null) {
                        subscriptions[msg.requestId]
                    } else {
                        subscriptionsByQuerySet[msg.querySetId]
                    }
                    handle?.state = SubscriptionState.ENDED
                    handle?.onErrorCallback?.invoke(msg.error)
                    handle?.requestId?.let { subscriptions.remove(it) }
                    subscriptionsByQuerySet.remove(msg.querySetId)
                }

                is ServerMessage.TransactionUpdate -> {
                    val ops = clientCache.applyTransactionUpdate(msg.querySets)
                    fireTableCallbacks(ops)
                }

                is ServerMessage.ReducerResult -> {
                    if (msg.result is ReducerOutcome.Ok) {
                        val txUpdate = msg.result.transactionUpdate
                        val ops = clientCache.applyTransactionUpdate(txUpdate.querySets)
                        fireTableCallbacks(ops)
                    }
                    reducerCallbacks.remove(msg.requestId)?.invoke(
                        ReducerResult(msg.requestId, msg.timestamp, msg.result)
                    )
                }

                is ServerMessage.ProcedureResult -> {}

                is ServerMessage.OneOffQueryResult -> {
                    pendingOneOffQueries.remove(msg.requestId)?.complete(msg)
                }
            }
        }
    }

    private fun fireTableCallbacks(ops: List<TableOperation>) {
        for (op in ops) {
            when (op) {
                is TableOperation.Insert -> tableHandles[op.tableName]?.fireInsert(op.row)
                is TableOperation.Delete -> tableHandles[op.tableName]?.fireDelete(op.row)
                is TableOperation.Update -> tableHandles[op.tableName]?.fireUpdate(op.oldRow, op.newRow)
                is TableOperation.EventInsert -> tableHandles[op.tableName]?.fireInsert(op.row)
            }
        }
    }

    companion object {
        fun builder(): DbConnectionBuilder = DbConnectionBuilder()
    }
}

/** Result of a reducer invocation, including the server-side [timestamp] and [outcome]. */
data class ReducerResult(
    val requestId: UInt,
    val timestamp: Timestamp,
    val outcome: ReducerOutcome,
)

/** Builder for configuring and creating a [DbConnection]. */
class DbConnectionBuilder {
    private var uri: String? = null
    private var moduleName: String? = null
    private var token: String? = null
    private var keepAliveIntervalMs: Long = 30_000L
    private var reconnectPolicy: ReconnectPolicy? = null
    private val connectCallbacks = mutableListOf<ConnectCallback>()
    private val disconnectCallbacks = mutableListOf<DisconnectCallback>()
    private val connectErrorCallbacks = mutableListOf<ConnectErrorCallback>()

    fun withUri(uri: String) = apply { this.uri = uri }

    fun withModuleName(name: String) = apply { this.moduleName = name }

    fun withToken(token: String?) = apply { this.token = token }

    fun onConnect(callback: ConnectCallback) = apply { connectCallbacks.add(callback) }

    fun onDisconnect(callback: DisconnectCallback) = apply { disconnectCallbacks.add(callback) }

    fun onConnectError(callback: ConnectErrorCallback) = apply { connectErrorCallbacks.add(callback) }

    fun withKeepAliveInterval(intervalMs: Long) = apply { this.keepAliveIntervalMs = intervalMs }

    fun withReconnectPolicy(policy: ReconnectPolicy) = apply { this.reconnectPolicy = policy }

    fun build(): DbConnection {
        val uri = requireNotNull(uri) { "URI is required. Call withUri() before build()." }
        val module = requireNotNull(moduleName) { "Module name is required. Call withModuleName() before build()." }
        return DbConnection(
            uri = uri,
            moduleName = module,
            token = token,
            connectCallbacks = connectCallbacks.toList(),
            disconnectCallbacks = disconnectCallbacks.toList(),
            connectErrorCallbacks = connectErrorCallbacks.toList(),
            keepAliveIntervalMs = keepAliveIntervalMs,
            reconnectPolicy = reconnectPolicy,
        )
    }
}
