import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import app.AppViewModel
import app.ChatRepository
import app.TokenStore
import app.composable.App
import app.createHttpClient

fun main() = application {
    val tokenStore = TokenStore()
    val repository = ChatRepository(createHttpClient(), tokenStore)
    val viewModel = AppViewModel(repository)
    Window(
        onCloseRequest = ::exitApplication,
        title = "SpacetimeDB Chat",
    ) {
        App(viewModel)
    }
}
