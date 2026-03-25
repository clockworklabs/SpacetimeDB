package app.composable

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.AppAction
import app.AppState
import app.AppViewModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.launch

@Composable
fun ChatScreen(
    state: AppState.Chat,
    onAction: (AppAction.Chat) -> Unit,
    modifier: Modifier = Modifier,
) {
    val listState = rememberLazyListState()

    LaunchedEffect(state.lines) {
        if (state.lines.isNotEmpty()) {
            listState.animateScrollToItem(state.lines.size - 1)
        }
    }

    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val isCompact = maxWidth < 600.dp

        if (isCompact) {
            CompactChatScreen(state, onAction, listState)
        } else {
            WideChatScreen(state, onAction, listState)
        }
    }
}

@Composable
private fun WideChatScreen(
    state: AppState.Chat,
    onAction: (AppAction.Chat) -> Unit,
    listState: LazyListState,
) {
    Row(modifier = Modifier.fillMaxSize()) {
        ChatPanel(
            state = state,
            onAction = onAction,
            listState = listState,
            modifier = Modifier.weight(1f).fillMaxHeight(),
        )

        VerticalDivider()

        Sidebar(
            onlineUsers = state.onlineUsers,
            offlineUsers = state.offlineUsers,
            notes = state.notes,
            noteSubState = state.noteSubState,
            modifier = Modifier.width(200.dp).fillMaxHeight(),
            onLogout = { onAction(AppAction.Chat.Logout) },
        )
    }
}

@Composable
private fun CompactChatScreen(
    state: AppState.Chat,
    onAction: (AppAction.Chat) -> Unit,
    listState: LazyListState,
) {
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            ModalDrawerSheet {
                Sidebar(
                    onlineUsers = state.onlineUsers,
                    offlineUsers = state.offlineUsers,
                    notes = state.notes,
                    noteSubState = state.noteSubState,
                    modifier = Modifier.fillMaxHeight().padding(8.dp),
                    onLogout = { onAction(AppAction.Chat.Logout) },
                )
            }
        },
    ) {
        ChatPanel(
            state = state,
            onAction = onAction,
            listState = listState,
            modifier = Modifier.fillMaxSize(),
            onUsersClicked = { scope.launch { drawerState.open() } },
        )
    }
}

@Composable
private fun ChatPanel(
    state: AppState.Chat,
    onAction: (AppAction.Chat) -> Unit,
    listState: LazyListState,
    modifier: Modifier = Modifier,
    onUsersClicked: (() -> Unit)? = null,
) {
    val imeBottom = WindowInsets.ime.getBottom(LocalDensity.current)

    LaunchedEffect(imeBottom) {
        if (imeBottom > 0 && state.lines.isNotEmpty()) {
            listState.animateScrollToItem(state.lines.size - 1)
        }
    }

    Column(modifier = modifier.padding(8.dp)) {
        if (onUsersClicked != null) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    state.dbName,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Bold,
                )
                OutlinedButton(onClick = onUsersClicked) {
                    Text("Users (${state.onlineUsers.size})")
                }
            }
            Spacer(Modifier.height(4.dp))
        }

        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            if (!state.connected) {
                item {
                    Text(
                        "Connecting to ${state.dbName}...",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            items(
                items = state.lines,
                key = { line ->
                    when (line) {
                        is AppState.Chat.ChatLine.Msg -> "msg-${line.id}"
                        is AppState.Chat.ChatLine.System -> "sys-${line.hashCode()}"
                    }
                },
            ) { line ->
                when (line) {
                    is AppState.Chat.ChatLine.Msg -> Row(verticalAlignment = Alignment.Bottom) {
                        Text(
                            "#${line.id} ${line.sender}: ${line.text}",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            AppViewModel.formatTimeStamp(line.sent),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }

                    is AppState.Chat.ChatLine.System -> Text(
                        line.text,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        Spacer(Modifier.height(4.dp))

        Text(
            "/name | /del | /note | /delnote | /unsub | /resub | /query | /squery | /remind | /remind-repeat | /remind-cancel",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(Modifier.height(4.dp))

        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = state.input,
                onValueChange = { onAction(AppAction.Chat.UpdateInput(it)) },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { onAction(AppAction.Chat.Submit) }),
                modifier = Modifier.weight(1f),
                placeholder = { Text("Type a message...") },
                singleLine = true,
                enabled = state.connected,
            )

            Spacer(Modifier.width(8.dp))

            Button(
                onClick = { onAction(AppAction.Chat.Submit) },
                enabled = state.connected && state.input.isNotBlank(),
            ) {
                Text("Send")
            }
        }
    }
}

@Composable
private fun Sidebar(
    onlineUsers: ImmutableList<String>,
    offlineUsers: ImmutableList<String>,
    notes: ImmutableList<AppState.Chat.NoteUi>,
    noteSubState: String,
    modifier: Modifier = Modifier,
    onLogout: (() -> Unit)? = null,
) {
    Column(modifier = modifier) {
        Text(
            "Online",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
        )

        Spacer(Modifier.height(4.dp))

        if (onlineUsers.isEmpty()) {
            Text(
                "No users online",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        onlineUsers.forEach { name ->
            Text(
                name,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }

        if (offlineUsers.isNotEmpty()) {
            Spacer(Modifier.height(12.dp))

            Text(
                "Offline",
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
            )

            Spacer(Modifier.height(4.dp))

            offlineUsers.forEach { name ->
                Text(
                    name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        Spacer(Modifier.height(16.dp))

        HorizontalDivider()

        Spacer(Modifier.height(8.dp))

        Text(
            "Notes",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
        )

        Text(
            "sub: $noteSubState",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        Spacer(Modifier.height(4.dp))

        if (notes.isEmpty()) {
            Text(
                "No notes",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        notes.forEach { note ->
            Text(
                "#${note.id} [${note.tag}] ${note.content}",
                style = MaterialTheme.typography.bodySmall,
            )
        }

        if (onLogout != null) {
            Spacer(Modifier.weight(1f))
            HorizontalDivider()
            Spacer(Modifier.height(8.dp))
            OutlinedButton(
                onClick = onLogout,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Logout")
            }
        }
    }
}
