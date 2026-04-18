package app.composable

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.AppAction
import app.AppState

@Composable
fun LoginScreen(
    state: AppState.Login,
    onAction: (AppAction.Login) -> Unit,
    modifier: Modifier = Modifier,
) {
    val focusManager = LocalFocusManager.current

    Column(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("SpacetimeDB Chat", style = MaterialTheme.typography.headlineMedium)

        Spacer(Modifier.height(16.dp))

        OutlinedTextField(
            value = state.clientIdField.value,
            onValueChange = { onAction(AppAction.Login.OnClientChanged(it)) },
            label = { Text("Client ID") },
            singleLine = true,
            isError = state.clientIdField.error != null,
            supportingText = state.clientIdField.error?.let { error -> { Text(error) } },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
            keyboardActions = KeyboardActions(onNext = { focusManager.moveFocus(FocusDirection.Down) }),
            modifier = Modifier.width(300.dp),
        )

        Spacer(Modifier.height(8.dp))

        OutlinedTextField(
            value = state.hostField.value,
            onValueChange = { onAction(AppAction.Login.OnHostChanged(it)) },
            label = { Text("Server Host") },
            singleLine = true,
            isError = state.hostField.error != null,
            supportingText = state.hostField.error?.let { error -> { Text(error) } },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = {
                focusManager.clearFocus()
                onAction(AppAction.Login.OnSubmitClicked)
            }),
            modifier = Modifier.width(300.dp),
        )

        Spacer(Modifier.height(8.dp))

        Button(onClick = { onAction(AppAction.Login.OnSubmitClicked) }) {
            Text("Connect")
        }
    }
}
