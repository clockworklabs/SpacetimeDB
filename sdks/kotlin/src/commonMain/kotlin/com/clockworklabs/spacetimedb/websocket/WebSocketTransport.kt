package com.clockworklabs.spacetimedb.websocket

import com.clockworklabs.spacetimedb.CompressionMode
import com.clockworklabs.spacetimedb.ConnectionId
import com.clockworklabs.spacetimedb.ReconnectPolicy
import com.clockworklabs.spacetimedb.decompressBrotli
import com.clockworklabs.spacetimedb.decompressGzip
import com.clockworklabs.spacetimedb.protocol.ClientMessage
import com.clockworklabs.spacetimedb.protocol.ServerMessage
import io.ktor.client.*
import io.ktor.client.call.*
import io.ktor.client.plugins.websocket.*
import io.ktor.client.request.*
import io.ktor.http.*
import io.ktor.websocket.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlin.concurrent.atomics.AtomicBoolean
import kotlin.time.Duration.Companion.milliseconds

private val HEX = "0123456789ABCDEF".toCharArray()

enum class ConnectionState {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    RECONNECTING,
}

class WebSocketTransport(
    private val scope: CoroutineScope,
    private val onMessage: suspend (ServerMessage) -> Unit,
    private val onConnect: () -> Unit,
    private val onDisconnect: (Throwable?) -> Unit,
    private val onConnectError: (Throwable) -> Unit,
    private val keepAliveIntervalMs: Long = 30_000L,
    private val reconnectPolicy: ReconnectPolicy? = null,
    private val compression: CompressionMode = CompressionMode.GZIP,
    private val connectionId: ConnectionId = ConnectionId.random(),
    private val confirmedReads: Boolean? = null,
    private val lightMode: Boolean = false,
) {
    private val client = HttpClient {
        install(WebSockets) {
            pingInterval = keepAliveIntervalMs.milliseconds
        }
    }

    private val _state = MutableStateFlow(ConnectionState.DISCONNECTED)
    val state: StateFlow<ConnectionState> = _state

    private val outboundQueue = Channel<ByteArray>(Channel.UNLIMITED)
    private var session: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null
    private val intentionalDisconnect = AtomicBoolean(false)

    // Tracks whether any data has arrived since the last keep-alive check.
    // Used to send pings only when idle to avoid flooding the server.
    private val idle = AtomicBoolean(true)
    private val wantPong = AtomicBoolean(false)

    fun connect(uri: String, moduleName: String, token: String?) {
        if (_state.value != ConnectionState.DISCONNECTED) return
        intentionalDisconnect.store(false)
        _state.value = ConnectionState.CONNECTING

        connectJob = scope.launch {
            runConnection(uri, moduleName, token)
        }
    }

    private suspend fun runConnection(uri: String, moduleName: String, token: String?) {
        try {
            connectSession(uri, moduleName, token)
            // Session ended normally
            if (!intentionalDisconnect.load() && reconnectPolicy != null) {
                attemptReconnect(uri, moduleName, token)
            } else {
                _state.value = ConnectionState.DISCONNECTED
                onDisconnect(null)
            }
        } catch (_: CancellationException) {
            _state.value = ConnectionState.DISCONNECTED
            if (!intentionalDisconnect.load()) {
                onDisconnect(null)
            }
        } catch (e: Throwable) {
            val previousState = _state.value
            if (!intentionalDisconnect.load() && reconnectPolicy != null) {
                attemptReconnect(uri, moduleName, token)
            } else if (previousState == ConnectionState.CONNECTING) {
                _state.value = ConnectionState.DISCONNECTED
                onConnectError(e)
            } else {
                _state.value = ConnectionState.DISCONNECTED
                onDisconnect(e)
            }
        }
    }

    private suspend fun exchangeToken(baseUri: String, token: String): String {
        val httpBase = when {
            baseUri.startsWith("ws://") -> "http://" + baseUri.removePrefix("ws://")
            baseUri.startsWith("wss://") -> "https://" + baseUri.removePrefix("wss://")
            baseUri.startsWith("http://") || baseUri.startsWith("https://") -> baseUri
            else -> "http://$baseUri"
        }
        val response = client.post("${httpBase.trimEnd('/')}/v1/identity/websocket-token") {
            header(HttpHeaders.Authorization, "Bearer $token")
        }
        val body = response.body<ByteArray>().decodeToString()
        // Parse {"token":"..."} from JSON
        val tokenKey = "\"token\":\""
        val start = body.indexOf(tokenKey)
        if (start < 0) throw IllegalStateException("Token exchange failed: $body")
        val valueStart = start + tokenKey.length
        val valueEnd = body.indexOf('"', valueStart)
        return body.substring(valueStart, valueEnd)
    }

    private suspend fun connectSession(uri: String, moduleName: String, token: String?) {
        val wsToken = if (token != null) exchangeToken(uri, token) else null
        val wsUri = buildWsUri(uri, moduleName, wsToken)
        client.webSocket(
            urlString = wsUri,
            request = {
                headers.append("Sec-WebSocket-Protocol", "v2.bsatn.spacetimedb")
            }
        ) {
            session = this
            idle.store(true)
            wantPong.store(false)
            _state.value = ConnectionState.CONNECTED
            onConnect()

            val sendJob = launch { processSendQueue() }
            val receiveJob = launch { processIncoming() }

            receiveJob.join()
            sendJob.cancelAndJoin()
        }
    }

    private suspend fun attemptReconnect(uri: String, moduleName: String, token: String?) {
        val policy = reconnectPolicy ?: return
        _state.value = ConnectionState.RECONNECTING

        for (attempt in 0 until policy.maxRetries) {
            if (intentionalDisconnect.load()) {
                _state.value = ConnectionState.DISCONNECTED
                return
            }

            val delayMs = policy.delayForAttempt(attempt)
            delay(delayMs.milliseconds)

            if (intentionalDisconnect.load()) {
                _state.value = ConnectionState.DISCONNECTED
                return
            }

            try {
                connectSession(uri, moduleName, token)
                // If connectSession returns normally, the session ended cleanly.
                // If we still want to reconnect (not intentionally), loop again.
                if (intentionalDisconnect.load()) {
                    _state.value = ConnectionState.DISCONNECTED
                    return
                }
                _state.value = ConnectionState.RECONNECTING
            } catch (_: CancellationException) {
                _state.value = ConnectionState.DISCONNECTED
                return
            } catch (_: Throwable) {
                // Connection attempt failed — continue to next retry
                _state.value = ConnectionState.RECONNECTING
            }
        }

        // Exhausted all retries
        _state.value = ConnectionState.DISCONNECTED
        onDisconnect(null)
    }

    fun disconnect() {
        intentionalDisconnect.store(true)
        connectJob?.cancel()
        session = null
        _state.value = ConnectionState.DISCONNECTED
        client.close()
    }

    fun send(message: ClientMessage) {
        val encoded = message.encode()
        outboundQueue.trySend(encoded)
    }

    private suspend fun DefaultClientWebSocketSession.processSendQueue() {
        for (bytes in outboundQueue) {
            send(Frame.Binary(true, bytes))
        }
    }

    private suspend fun DefaultClientWebSocketSession.processIncoming() {
        for (frame in incoming) {
            when (frame) {
                is Frame.Binary -> {
                    idle.store(false)
                    val raw = frame.readBytes()
                    val payload = decompressIfNeeded(raw)
                    val msg = ServerMessage.decode(payload)
                    onMessage(msg)
                }

                is Frame.Pong -> {
                    idle.store(false)
                    wantPong.store(false)
                }

                is Frame.Close -> return
                else -> {
                    idle.store(false)
                }
            }
        }
    }

    private fun decompressIfNeeded(data: ByteArray): ByteArray {
        if (data.isEmpty()) return data
        val tag = data[0].toUByte().toInt()
        val payload = data.copyOfRange(1, data.size)
        return when (tag) {
            0 -> payload
            1 -> decompressBrotli(payload)
            2 -> decompressGzip(payload)
            else -> throw IllegalStateException("Unknown compression tag: $tag")
        }
    }

    private fun urlEncode(value: String): String = buildString {
        for (c in value) {
            when {
                c.isLetterOrDigit() || c in "-._~" -> append(c)
                else -> {
                    for (b in c.toString().encodeToByteArray()) {
                        append('%')
                        append(HEX[(b.toInt() shr 4) and 0xF])
                        append(HEX[b.toInt() and 0xF])
                    }
                }
            }
        }
    }

    private fun buildWsUri(uri: String, moduleName: String, token: String?): String {
        val base = uri.trimEnd('/')
        val wsBase = when {
            base.startsWith("ws://") || base.startsWith("wss://") -> base
            base.startsWith("http://") -> "ws://" + base.removePrefix("http://")
            base.startsWith("https://") -> "wss://" + base.removePrefix("https://")
            else -> "ws://$base"
        }
        val sb = StringBuilder("$wsBase/v1/database/$moduleName/subscribe")
        val params = mutableListOf<String>()
        if (token != null) params.add("token=${urlEncode(token)}")
        params.add("compression=${compression.queryValue}")
        params.add("connection_id=${connectionId.toHex()}")
        if (confirmedReads != null) {
            params.add("confirmed=$confirmedReads")
        }
        if (lightMode) {
            params.add("light=true")
        }
        sb.append("?${params.joinToString("&")}")
        return sb.toString()
    }
}
