import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import app.AppViewModel
import app.ChatRepository
import app.TokenStore
import app.composable.App
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val httpClient = HttpClient(OkHttp) { install(WebSockets) }
        val tokenStore = TokenStore(applicationContext)
        // 10.0.2.2 is the Android emulator's alias for the host machine's loopback.
        // For physical devices, replace with your machine's LAN IP (e.g. "ws://192.168.1.x:3000").
        val repository = ChatRepository(httpClient, tokenStore, host = "ws://10.0.2.2:3000")
        val viewModel = AppViewModel(repository)
        setContent { App(viewModel) }
    }
}
