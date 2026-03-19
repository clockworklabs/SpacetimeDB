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
    val httpClient = HttpClient(OkHttp) { install(WebSockets) }
    val tokenStore = TokenStore()
    val repository = ChatRepository(httpClient, tokenStore, host = "ws://localhost:3000")
    val viewModel = AppViewModel(repository)
    Window(
        onCloseRequest = ::exitApplication,
        title = "SpacetimeDB Chat",
    ) {
        App(viewModel)
    }
}
