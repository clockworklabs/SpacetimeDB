import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import app.AppViewModel
import app.ChatRepository
import app.TokenStore
import app.composable.App
import app.createHttpClient

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val tokenStore = TokenStore(applicationContext)
        val repository = ChatRepository(createHttpClient(), tokenStore)
        val viewModel = AppViewModel(repository)
        setContent { App(viewModel) }
    }
}
