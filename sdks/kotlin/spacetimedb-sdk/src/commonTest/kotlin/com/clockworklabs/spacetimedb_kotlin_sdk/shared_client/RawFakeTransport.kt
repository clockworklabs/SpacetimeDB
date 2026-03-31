package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.SpacetimeTransport
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import kotlinx.atomicfu.atomic
import kotlinx.atomicfu.update
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * A test transport that accepts raw byte arrays and decodes BSATN inside the
 * [incoming] flow, mirroring [SpacetimeTransport]'s
 * decode-in-flow behavior.
 *
 * This allows testing how [DbConnection] reacts to malformed frames:
 * truncated BSATN, invalid sum tags, empty frames, etc.
 * Decode errors surface as exceptions in the flow, which DbConnection's
 * receive loop catches and routes to onDisconnect(error).
 */
internal class RawFakeTransport : Transport {
    private val _rawIncoming = Channel<ByteArray>(Channel.UNLIMITED)
    private val _sent = atomic(persistentListOf<ClientMessage>())
    private var _connected = false

    override suspend fun connect() {
        _connected = true
    }

    override suspend fun send(message: ClientMessage) {
        _sent.update { it.add(message) }
    }

    override fun incoming(): Flow<ServerMessage> = flow {
        for (bytes in _rawIncoming) {
            emit(ServerMessage.decodeFromBytes(bytes))
        }
    }

    override suspend fun disconnect() {
        _connected = false
        _rawIncoming.close()
    }

    /** Send raw BSATN bytes to the client. Decode happens inside [incoming]. */
    suspend fun sendRawToClient(bytes: ByteArray) {
        _rawIncoming.send(bytes)
    }
}
