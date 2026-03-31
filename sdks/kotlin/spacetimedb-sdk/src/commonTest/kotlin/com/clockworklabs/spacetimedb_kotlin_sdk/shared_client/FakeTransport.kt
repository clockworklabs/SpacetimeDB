package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class, kotlinx.coroutines.DelicateCoroutinesApi::class)
internal class FakeTransport(
    private val connectError: Throwable? = null,
) : Transport {
    private var _incoming = Channel<ServerMessage>(Channel.UNLIMITED)
    private val _sent = atomic(persistentListOf<ClientMessage>())
    private val _sendError = atomic<Throwable?>(null)
    private var _connected = false

    override suspend fun connect() {
        connectError?.let { throw it }
        // Recreate channel on reconnect (closed channels can't be reused)
        if (_incoming.isClosedForSend) {
            _incoming = Channel(Channel.UNLIMITED)
        }
        _connected = true
    }

    override suspend fun send(message: ClientMessage) {
        _sendError.value?.let { throw it }
        _sent.update { it.add(message) }
    }

    override fun incoming(): Flow<ServerMessage> = _incoming.consumeAsFlow()

    override suspend fun disconnect() {
        _connected = false
        _incoming.close()
    }

    val sentMessages: List<ClientMessage> get() = _sent.value

    suspend fun sendToClient(message: ServerMessage) {
        _incoming.send(message)
    }

    /** Close the incoming channel normally (flow completes, onDisconnect fires with null error). */
    fun closeFromServer() {
        _incoming.close()
    }

    /** Close the incoming channel with an error (flow throws, onDisconnect fires with the error). */
    fun closeWithError(cause: Throwable) {
        _incoming.close(cause)
    }

    /** When set, subsequent [send] calls throw this error (simulates send-path failure). */
    var sendError: Throwable?
        get() = _sendError.value
        set(value) { _sendError.value = value }
}
