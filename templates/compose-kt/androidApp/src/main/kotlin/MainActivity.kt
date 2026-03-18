import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import app.composable.App

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { App() }
    }
}