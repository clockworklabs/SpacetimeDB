import androidx.compose.runtime.remember
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import app.AppViewModel
import app.ChatRepository
import app.TokenStore
import app.composable.App
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
fun main() = application {
    val httpClient = remember { HttpClient(OkHttp) { install(WebSockets) } }
    val tokenStore = remember { TokenStore() }
    val repository = remember { ChatRepository(httpClient, tokenStore) }
    val viewModel = remember { AppViewModel(repository, defaultHost = "ws://localhost:3000") }
    Window(
        onCloseRequest = {
            // ViewModel.onCleared handles disconnect via runBlocking.
            // Just close the HTTP client and exit.
            httpClient.close()
            exitApplication()
        },
        title = "SpacetimeDB Chat",
    ) {
        App(viewModel)
    }
}
