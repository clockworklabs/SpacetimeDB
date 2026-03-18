import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import app.composable.App

fun main() = application {
    Window(
        onCloseRequest = ::exitApplication,
        title = "SpacetimeDB Chat",
    ) {
        App()
    }
}
