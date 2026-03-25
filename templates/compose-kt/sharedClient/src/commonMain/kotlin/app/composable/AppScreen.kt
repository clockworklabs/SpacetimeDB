package app.composable

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
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
        Scaffold(
            modifier = Modifier.fillMaxSize().imePadding()
        ) { innerPadding ->
            when (state.currentScreen) {
                AppState.Screen.LOGIN -> LoginScreen(
                    state = state.login,
                    onAction = viewModel::onAction,
                    modifier = Modifier.padding(innerPadding),
                )

                AppState.Screen.CHAT -> ChatScreen(
                    state = state.chat,
                    onAction = viewModel::onAction,
                    modifier = Modifier.padding(innerPadding),
                )
            }
        }
    }
}