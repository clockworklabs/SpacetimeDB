package com.clockworklabs.spacetimedb.websocket

import com.clockworklabs.spacetimedb.CompressionMode
import com.clockworklabs.spacetimedb.ReconnectPolicy
import com.clockworklabs.spacetimedb.decompressBrotli
import com.clockworklabs.spacetimedb.decompressGzip
import com.clockworklabs.spacetimedb.protocol.ClientMessage
import com.clockworklabs.spacetimedb.protocol.ServerMessage
import io.ktor.client.*
import io.ktor.client.plugins.websocket.*
import io.ktor.websocket.*
import kotlinx.atomicfu.atomic
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

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
) {
    private val client = HttpClient {
        install(WebSockets)
    }

    private val _state = MutableStateFlow(ConnectionState.DISCONNECTED)
    val state: StateFlow<ConnectionState> = _state

    private val outboundQueue = Channel<ByteArray>(Channel.UNLIMITED)
    private var session: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null
    private val intentionalDisconnect = atomic(false)

    // Ping/pong idle detection (mirrors Rust SDK's 30s idle timeout)
    private val idle = atomic(true)
    private val wantPong = atomic(false)

    fun connect(uri: String, moduleName: String, token: String?) {
        if (_state.value != ConnectionState.DISCONNECTED) return
        intentionalDisconnect.value = false
        _state.value = ConnectionState.CONNECTING

        connectJob = scope.launch {
            runConnection(uri, moduleName, token)
        }
    }

    private suspend fun runConnection(uri: String, moduleName: String, token: String?) {
        try {
            connectSession(uri, moduleName, token)
            // Session ended normally
            if (!intentionalDisconnect.value && reconnectPolicy != null) {
                attemptReconnect(uri, moduleName, token)
            } else {
                _state.value = ConnectionState.DISCONNECTED
                onDisconnect(null)
            }
        } catch (e: CancellationException) {
            _state.value = ConnectionState.DISCONNECTED
            if (!intentionalDisconnect.value) {
                onDisconnect(null)
            }
        } catch (e: Throwable) {
            val previousState = _state.value
            if (!intentionalDisconnect.value && reconnectPolicy != null && previousState == ConnectionState.CONNECTED) {
                // Was connected, lost connection unexpectedly — try to reconnect
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

    private suspend fun connectSession(uri: String, moduleName: String, token: String?) {
        val wsUri = buildWsUri(uri, moduleName, token)
        client.webSocket(
            urlString = wsUri,
            request = {
                headers.append("Sec-WebSocket-Protocol", "v2.bsatn.spacetimedb")
            }
        ) {
            session = this
            idle.value = true
            wantPong.value = false
            _state.value = ConnectionState.CONNECTED
            onConnect()

            val sendJob = launch { processSendQueue() }
            val receiveJob = launch { processIncoming() }
            val keepAliveJob = if (keepAliveIntervalMs > 0) {
                launch { runKeepAlive() }
            } else null

            receiveJob.join()
            keepAliveJob?.cancelAndJoin()
            sendJob.cancelAndJoin()
        }
    }

    private suspend fun attemptReconnect(uri: String, moduleName: String, token: String?) {
        val policy = reconnectPolicy ?: return
        _state.value = ConnectionState.RECONNECTING

        for (attempt in 0 until policy.maxRetries) {
            if (intentionalDisconnect.value) {
                _state.value = ConnectionState.DISCONNECTED
                return
            }

            val delayMs = policy.delayForAttempt(attempt)
            delay(delayMs)

            if (intentionalDisconnect.value) {
                _state.value = ConnectionState.DISCONNECTED
                return
            }

            try {
                connectSession(uri, moduleName, token)
                // If connectSession returns normally, the session ended cleanly.
                // If we still want to reconnect (not intentional), loop again.
                if (intentionalDisconnect.value) {
                    _state.value = ConnectionState.DISCONNECTED
                    return
                }
                _state.value = ConnectionState.RECONNECTING
            } catch (e: CancellationException) {
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
        intentionalDisconnect.value = true
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
                    idle.value = false
                    val raw = frame.readBytes()
                    val payload = decompressIfNeeded(raw)
                    val msg = ServerMessage.decode(payload)
                    onMessage(msg)
                }
                is Frame.Pong -> {
                    idle.value = false
                    wantPong.value = false
                }
                is Frame.Close -> return
                else -> {
                    idle.value = false
                }
            }
        }
    }

    /**
     * Idle timeout keep-alive, modeled on the Rust SDK pattern:
     *
     * Every [keepAliveIntervalMs]:
     * - If no data arrived and we're waiting for a pong -> connection is dead, close it.
     * - If no data arrived -> send a Ping, start waiting for pong.
     * - If data arrived -> reset idle flag for the next interval.
     */
    private suspend fun DefaultClientWebSocketSession.runKeepAlive() {
        while (true) {
            delay(keepAliveIntervalMs)
            if (idle.value) {
                if (wantPong.value) {
                    close(CloseReason(CloseReason.Codes.GOING_AWAY, "Idle timeout"))
                    return
                }
                send(Frame.Ping(ByteArray(0)))
                wantPong.value = true
            }
            idle.value = true
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
        sb.append("?${params.joinToString("&")}")
        return sb.toString()
    }
}
