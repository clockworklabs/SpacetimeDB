package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ClientMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.decompressMessage
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.client.request.header
import io.ktor.http.URLBuilder
import io.ktor.http.URLProtocol
import io.ktor.http.Url
import io.ktor.http.appendPathSegments
import io.ktor.websocket.Frame
import io.ktor.websocket.WebSocketSession
import io.ktor.websocket.close
import io.ktor.websocket.readBytes
import kotlinx.atomicfu.atomic
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * Transport abstraction for SpacetimeDB connections.
 * Allows injecting a fake transport in tests.
 */
internal interface Transport {
    suspend fun connect()
    suspend fun send(message: ClientMessage)
    fun incoming(): Flow<ServerMessage>
    suspend fun disconnect()
}

/**
 * WebSocket transport for SpacetimeDB.
 * Handles connection, message encoding/decoding, and compression.
 */
internal class SpacetimeTransport(
    private val client: HttpClient,
    private val baseUrl: String,
    private val nameOrAddress: String,
    private val connectionId: ConnectionId,
    private val authToken: String? = null,
    private val compression: CompressionMode = CompressionMode.GZIP,
    private val lightMode: Boolean = false,
    private val confirmedReads: Boolean? = null,
) : Transport {
    private val _session = atomic<WebSocketSession?>(null)

    internal companion object {
        /** WebSocket sub-protocol identifier for BSATN v2. */
        const val WS_PROTOCOL: String = "v2.bsatn.spacetimedb"
    }



    /**
     * Connects to the SpacetimeDB WebSocket endpoint.
     * Passes the auth token as a Bearer Authorization header on the WebSocket connection.
     */
    override suspend fun connect() {
        val wsUrl = buildWsUrl()

        _session.value = client.webSocketSession(wsUrl) {
            header("Sec-WebSocket-Protocol", WS_PROTOCOL)
            if (authToken != null) {
                header("Authorization", "Bearer $authToken")
            }
        }
    }

    /**
     * Sends a [ClientMessage] over the WebSocket as a BSATN-encoded binary frame.
     */
    override suspend fun send(message: ClientMessage) {
        val writer = BsatnWriter()
        message.encode(writer)
        val encoded = writer.toByteArray()
        val ws = _session.value ?: error("Not connected")
        ws.send(Frame.Binary(true, encoded))
    }

    /**
     * Returns a Flow of ServerMessages received from the WebSocket.
     * Handles decompression (prefix byte) then BSATN decoding.
     */
    override fun incoming(): Flow<ServerMessage> = flow {
        val ws = _session.value ?: error("Not connected")
        // On clean close, the for-loop exits normally (hasNext() returns false).
        // On abnormal close, hasNext() throws the original cause (e.g. IOException),
        // which propagates to DbConnection's error handling path.
        for (frame in ws.incoming) {
            if (frame is Frame.Binary) {
                val raw = frame.readBytes()
                val decompressed = decompressMessage(raw)
                val message = ServerMessage.decodeFromBytes(decompressed.data, decompressed.offset)
                emit(message)
            }
        }
    }

    /** Closes the WebSocket session, if one is open. */
    override suspend fun disconnect() {
        val ws = _session.getAndSet(null)
        ws?.close()
    }

    private fun buildWsUrl(): String {
        val base = Url(baseUrl)
        return URLBuilder(base).apply {
            protocol = when (base.protocol) {
                URLProtocol.HTTPS -> URLProtocol.WSS
                URLProtocol.HTTP -> URLProtocol.WS
                URLProtocol.WSS -> URLProtocol.WSS
                URLProtocol.WS -> URLProtocol.WS
                else -> throw IllegalArgumentException(
                    "Unsupported protocol '${base.protocol.name}'. Use http://, https://, ws://, or wss://"
                )
            }
            appendPathSegments("v1", "database", nameOrAddress, "subscribe")
            parameters.append("connection_id", connectionId.toHexString())
            parameters.append("compression", compression.wireValue)
            if (lightMode) {
                parameters.append("light", "true")
            }
            if (confirmedReads != null) {
                parameters.append("confirmed", confirmedReads.toString())
            }
        }.buildString()
    }
}
