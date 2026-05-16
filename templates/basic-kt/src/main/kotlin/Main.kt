import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.use
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
import kotlinx.coroutines.delay
import module_bindings.db
import module_bindings.reducers
import module_bindings.subscribeToAllTables
import module_bindings.withModuleBindings
import kotlin.time.Duration.Companion.seconds

suspend fun main() {
    val host = System.getenv("SPACETIMEDB_HOST") ?: "ws://localhost:3000"
    val httpClient = HttpClient(OkHttp) { install(WebSockets) }

    DbConnection.Builder()
        .withHttpClient(httpClient)
        .withUri(host)
        .withDatabaseName(module_bindings.SpacetimeConfig.DATABASE_NAME)
        .withModuleBindings()
        .onConnect { conn, identity, _ ->
            println("Connected to SpacetimeDB!")
            println("Identity: ${identity.toHexString().take(16)}...")

            conn.db.person.onInsert { _, person ->
                println("New person: ${person.name}")
            }

            conn.reducers.onAdd { ctx, name ->
                println("[onAdd] Added person: $name (status=${ctx.status})")
            }

            conn.subscriptionBuilder()
                .onError { _, error -> println("Subscription error: $error") }
                .subscribeToAllTables()

            conn.reducers.add("Alice") { ctx ->
                println("[one-shot] Add completed: status=${ctx.status}")
                conn.reducers.sayHello()
            }
        }
        .onDisconnect { _, error ->
            if (error != null) {
                println("Disconnected with error: $error")
            } else {
                println("Disconnected")
            }
        }
        .onConnectError { _, error ->
            println("Connection error: $error")
        }
        .build()
        .use { delay(5.seconds) }
}
