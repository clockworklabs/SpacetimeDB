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
        val repository = ChatRepository(httpClient, tokenStore, host = "ws://10.0.2.2:3000")
        val viewModel = AppViewModel(repository)
        setContent { App(viewModel) }
    }
}
