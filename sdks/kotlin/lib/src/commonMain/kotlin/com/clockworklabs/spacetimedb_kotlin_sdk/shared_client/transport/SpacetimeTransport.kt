@file:Suppress("unused")

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
import kotlinx.coroutines.channels.ClosedReceiveChannelException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * WebSocket transport for SpacetimeDB.
 * Handles connection, message encoding/decoding, and compression.
 */
class SpacetimeTransport(
    private val client: HttpClient,
    private val baseUrl: String,
    private val nameOrAddress: String,
    private val connectionId: ConnectionId,
    private val authToken: String? = null,
    private val compression: CompressionMode = CompressionMode.GZIP,
    private val lightMode: Boolean = false,
    private val confirmedReads: Boolean? = null,
) {
    private var session: WebSocketSession? = null

    companion object {
        const val WS_PROTOCOL = "v2.bsatn.spacetimedb"
    }

    val isConnected: Boolean get() = session != null

    /**
     * Connects to the SpacetimeDB WebSocket endpoint.
     * Passes the auth token as a Bearer Authorization header directly
     * on the WebSocket connection (matching C# SDK).
     */
    suspend fun connect() {
        val wsUrl = buildWsUrl()

        session = client.webSocketSession(wsUrl) {
            header("Sec-WebSocket-Protocol", WS_PROTOCOL)
            if (authToken != null) {
                header("Authorization", "Bearer $authToken")
            }
        }
    }

    /**
     * Sends a ClientMessage over the WebSocket.
     * Matches TS SDK's #sendEncoded: serialize to BSATN then send as binary frame.
     */
    suspend fun send(message: ClientMessage) {
        val writer = BsatnWriter()
        message.encode(writer)
        val encoded = writer.toByteArray()
        session?.send(Frame.Binary(true, encoded))
            ?: error("Not connected")
    }

    /**
     * Returns a Flow of ServerMessages received from the WebSocket.
     * Handles decompression (prefix byte) then BSATN decoding.
     */
    fun incoming(): Flow<ServerMessage> = flow {
        val ws = session ?: error("Not connected")
        try {
            for (frame in ws.incoming) {
                if (frame is Frame.Binary) {
                    val raw = frame.readBytes()
                    val decompressed = decompressMessage(raw)
                    val message = ServerMessage.decodeFromBytes(decompressed)
                    emit(message)
                }
            }
        } catch (_: ClosedReceiveChannelException) {
            // Connection closed normally
        }
    }

    suspend fun disconnect() {
        session?.close()
        session = null
    }

    private fun buildWsUrl(): String {
        val base = Url(baseUrl)
        return URLBuilder(base).apply {
            protocol = when (base.protocol) {
                URLProtocol.HTTPS -> URLProtocol.WSS
                URLProtocol.HTTP -> URLProtocol.WS
                else -> base.protocol
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
