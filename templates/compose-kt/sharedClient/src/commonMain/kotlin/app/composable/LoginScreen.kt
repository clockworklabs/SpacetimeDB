package app.composable

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.unit.dp
import app.AppAction
import app.AppState

@Composable
fun LoginScreen(
    state: AppState.Login,
    onAction: (AppAction.Login) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("SpacetimeDB Chat", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(16.dp))
        OutlinedTextField(
            value = state.clientId,
            onValueChange = { onAction(AppAction.Login.OnClientChanged(it)) },
            label = { Text("Client ID") },
            singleLine = true,
            isError = state.error != null,
            supportingText = state.error?.let { error -> { Text(error) } },
            modifier = Modifier.width(300.dp)
                .onKeyEvent { event ->
                    if (event.type == KeyEventType.KeyDown && event.key == Key.Enter) {
                        onAction(AppAction.Login.OnSubmitClicked)
                        true
                    } else false
                },
        )
        Spacer(Modifier.height(8.dp))
        Button(onClick = { onAction(AppAction.Login.OnSubmitClicked) }) {
            Text("Connect")
        }
    }
}
