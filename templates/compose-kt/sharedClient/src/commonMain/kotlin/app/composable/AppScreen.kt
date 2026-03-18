package app.composable

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.AppState
import app.AppViewModel
import app.ChatRepository
import app.createHttpClient

@Composable
fun App() {
    val httpClient = remember { createHttpClient() }
    val repository = remember { ChatRepository(httpClient) }
    val viewModel = remember { AppViewModel(repository) }

    val state by viewModel.state.collectAsStateWithLifecycle()

    MaterialTheme(colorScheme = darkColorScheme()) {
        Surface(modifier = Modifier.fillMaxSize()) {
            when (val s = state) {
                is AppState.Login -> LoginScreen(
                    state = s,
                    onAction = viewModel::onAction,
                )

                is AppState.Chat -> ChatScreen(
                    state = s,
                    onAction = viewModel::onAction,
                )
            }
        }
    }
}
