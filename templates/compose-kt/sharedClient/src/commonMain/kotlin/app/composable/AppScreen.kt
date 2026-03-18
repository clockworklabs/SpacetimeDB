package app.composable

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.AppState
import app.AppViewModel

@Composable
fun App(viewModel: AppViewModel) {
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
