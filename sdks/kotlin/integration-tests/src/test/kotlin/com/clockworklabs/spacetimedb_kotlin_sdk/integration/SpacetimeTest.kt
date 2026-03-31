package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.reducers
import module_bindings.withModuleBindings
import java.net.Socket

val HOST: String = System.getenv("SPACETIMEDB_HOST") ?: "ws://localhost:3000"
val DB_NAME: String = System.getenv("SPACETIMEDB_DB_NAME") ?: "chat-all"
const val DEFAULT_TIMEOUT_MS = 10_000L

private fun checkServerReachable() {
    val url = java.net.URI(HOST.replace("ws://", "http://").replace("wss://", "https://"))
    val host = url.host ?: "localhost"
    val port = if (url.port > 0) url.port else 3000
    try {
        Socket().use { it.connect(java.net.InetSocketAddress(host, port), 2000) }
    } catch (_: Exception) {
        throw AssertionError(
            "SpacetimeDB server is not reachable at $host:$port. " +
            "Start it with: spacetimedb-cli start"
        )
    }
}

fun createTestHttpClient(): HttpClient = HttpClient(OkHttp) {
    install(WebSockets)
}

data class ConnectedClient(
    val conn: DbConnection,
    val identity: Identity,
    val token: String,
)

suspend fun connectToDb(token: String? = null): ConnectedClient {
    checkServerReachable()
    val identityDeferred = CompletableDeferred<Pair<Identity, String>>()

    val connection = DbConnection.Builder()
        .withHttpClient(createTestHttpClient())
        .withUri(HOST)
        .withDatabaseName(DB_NAME)
        .withToken(token)
        .withModuleBindings()
        .onConnect { _, identity, tok ->
            identityDeferred.complete(identity to tok)
        }
        .onConnectError { _, e ->
            identityDeferred.completeExceptionally(e)
        }
        .build()

    val (identity, tok) = withTimeout(DEFAULT_TIMEOUT_MS) { identityDeferred.await() }
    return ConnectedClient(conn = connection, identity = identity, token = tok)
}

suspend fun ConnectedClient.subscribeAll(): ConnectedClient {
    val applied = CompletableDeferred<Unit>()
    conn.subscriptionBuilder()
        .onApplied { _ -> applied.complete(Unit) }
        .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
        .subscribe(listOf(
            "SELECT * FROM user",
            "SELECT * FROM message",
            "SELECT * FROM note",
            "SELECT * FROM reminder",
            "SELECT * FROM big_int_row",
        ))
    withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
    return this
}

suspend fun ConnectedClient.cleanup() {
    for (msg in conn.db.message.all()) {
        if (msg.sender == identity) {
            conn.reducers.deleteMessage(msg.id)
        }
    }
    for (note in conn.db.note.all()) {
        if (note.owner == identity) {
            conn.reducers.deleteNote(note.id)
        }
    }
    for (reminder in conn.db.reminder.all()) {
        if (reminder.owner == identity) {
            conn.reducers.cancelReminder(reminder.scheduledId)
        }
    }
    conn.disconnect()
}
