package app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.WhileSubscribed
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.datetime.TimeZone
import kotlinx.datetime.number
import kotlinx.datetime.toLocalDateTime
import kotlin.time.Duration.Companion.seconds

class AppViewModel(
    private val chatRepository: ChatRepository,
    defaultHost: String,
) : ViewModel() {

    private var observationJob: Job? = null

    private val _state = MutableStateFlow(AppState(login = AppState.Login(hostField = FieldInput(defaultHost))))
    val state: StateFlow<AppState> = _state
        .stateIn(
            scope = viewModelScope,
            started = SharingStarted.WhileSubscribed(5.seconds),
            initialValue = _state.value
        )

    fun onAction(action: AppAction) {
        when (action) {
            is AppAction.Login.OnClientChanged -> updateLogin {
                copy(clientIdField = clientIdField.copy(value = action.client, error = null))
            }

            is AppAction.Login.OnHostChanged -> updateLogin {
                copy(hostField = hostField.copy(value = action.host, error = null))
            }

            AppAction.Login.OnSubmitClicked -> handleLoginSubmit()

            is AppAction.Chat.UpdateInput -> updateChat {
                copy(input = action.input)
            }

            AppAction.Chat.Submit -> handleChatSubmit()
            AppAction.Chat.Logout -> handleLogout()
        }
    }

    private fun handleLoginSubmit() {
        val currentState = _state.value
        val clientId = currentState.login.clientIdField.value
        val host = currentState.login.hostField.value

        if (clientId.isBlank()) {
            updateLogin { copy(clientIdField = clientIdField.copy(error = "Client ID cannot be empty")) }
            return
        }
        if (!clientId.all { it.isLetterOrDigit() || it == '-' || it == '_' }) {
            updateLogin { copy(clientIdField = clientIdField.copy(error ="Client ID may only contain letters, digits, '-', or '_'")) }
            return
        }
        if (host.isBlank()) {
            updateLogin { copy(hostField = hostField.copy(error = "Server host cannot be empty")) }
            return
        }

        _state.update {
            it.copy(currentScreen = AppState.Screen.CHAT, chat = AppState.Chat())
        }
        observeRepository()
        viewModelScope.launch {
            chatRepository.connect(clientId, host)
        }
    }

    private fun handleChatSubmit() {
        val currentState = _state.value
        val text = currentState.chat.input.trim()
        if (text.isEmpty()) return

        updateChat { copy(input = "") }

        val parts = text.split(" ", limit = 2)
        val cmd = parts[0]
        val arg = parts.getOrElse(1) { "" }

        when (cmd) {
            "/name" -> chatRepository.setName(arg)

            "/del" -> {
                val id = arg.trim().toULongOrNull()
                if (id != null) chatRepository.deleteMessage(id)
                else chatRepository.log("Usage: /del <message_id>")
            }

            "/note" -> {
                val noteParts = arg.trim().split(" ", limit = 2)
                if (noteParts.size == 2) chatRepository.addNote(noteParts[1], noteParts[0])
                else chatRepository.log("Usage: /note <tag> <content>")
            }

            "/delnote" -> {
                val id = arg.trim().toULongOrNull()
                if (id != null) chatRepository.deleteNote(id)
                else chatRepository.log("Usage: /delnote <note_id>")
            }

            "/unsub" -> chatRepository.unsubscribeNotes()
            "/resub" -> chatRepository.resubscribeNotes()

            "/query" -> {
                val sql = arg.trim()
                if (sql.isEmpty()) chatRepository.log("Usage: /query <sql>")
                else chatRepository.oneOffQuery(sql)
            }

            "/squery" -> {
                val sql = arg.trim()
                if (sql.isEmpty()) chatRepository.log("Usage: /squery <sql>")
                else viewModelScope.launch(Dispatchers.Default) {
                    chatRepository.suspendOneOffQuery(sql)
                }
            }

            "/remind" -> {
                val remindParts = arg.trim().split(" ", limit = 2)
                val delayMs = remindParts.getOrNull(0)?.toULongOrNull()
                val remindText = remindParts.getOrNull(1)
                if (delayMs != null && remindText != null) chatRepository.scheduleReminder(
                    remindText,
                    delayMs
                )
                else chatRepository.log("Usage: /remind <delay_ms> <text>")
            }

            "/remind-cancel" -> {
                val id = arg.trim().toULongOrNull()
                if (id != null) chatRepository.cancelReminder(id)
                else chatRepository.log("Usage: /remind-cancel <reminder_id>")
            }

            "/remind-repeat" -> {
                val remindParts = arg.trim().split(" ", limit = 2)
                val intervalMs = remindParts.getOrNull(0)?.toULongOrNull()
                val remindText = remindParts.getOrNull(1)
                if (intervalMs != null && remindText != null) chatRepository.scheduleReminderRepeat(
                    remindText,
                    intervalMs
                )
                else chatRepository.log("Usage: /remind-repeat <interval_ms> <text>")
            }

            else -> chatRepository.sendMessage(text)
        }
    }

    private fun handleLogout() {
        observationJob?.cancel()
        _state.update {
            it.copy(chat = AppState.Chat(), currentScreen = AppState.Screen.LOGIN)
        }
        viewModelScope.launch { chatRepository.disconnect() }
    }

    private fun observeRepository() {
        observationJob?.cancel()
        observationJob = viewModelScope.launch {
            chatRepository.connected
                .onEach { connected -> updateChat { copy(connected = connected) } }
                .launchIn(this)

            chatRepository.lines
                .onEach { lines ->
                    updateChat {
                        copy(lines = lines.map { it.toChatLine() }.toImmutableList())
                    }
                }
                .launchIn(this)

            chatRepository.onlineUsers
                .onEach { users -> updateChat { copy(onlineUsers = users.toImmutableList()) } }
                .launchIn(this)

            chatRepository.offlineUsers
                .onEach { users -> updateChat { copy(offlineUsers = users.toImmutableList()) } }
                .launchIn(this)

            chatRepository.notes
                .onEach { notes ->
                    updateChat {
                        copy(notes = notes.map { it.toNoteUi() }.toImmutableList())
                    }
                }
                .launchIn(this)

            chatRepository.noteSubState
                .onEach { state -> updateChat { copy(noteSubState = state) } }
                .launchIn(this)

            chatRepository.connectionError
                .onEach { error ->
                    if (error != null) {
                        _state.update {
                            it.copy(
                                currentScreen = AppState.Screen.LOGIN,
                                login = it.login.copy(
                                    hostField = it.login.hostField.copy(error = error),
                                ),
                                chat = AppState.Chat(),
                            )
                        }
                    }
                }
                .launchIn(this)
        }
    }

    private inline fun updateLogin(block: AppState.Login.() -> AppState.Login) {
        _state.update { it.copy(login = block(it.login)) }
    }

    private inline fun updateChat(block: AppState.Chat.() -> AppState.Chat) {
        _state.update { it.copy(chat = block(it.chat)) }
    }

    override fun onCleared() {
        observationJob?.cancel()
        runBlocking { chatRepository.disconnect() }
    }

    companion object {
        fun formatTimeStamp(timeStamp: Timestamp): String {
            val dt = timeStamp.instant.toLocalDateTime(TimeZone.currentSystemDefault())

            val year = dt.year.toString().padStart(4, '0')
            val month = dt.month.number.toString().padStart(2, '0')
            val day = dt.day.toString().padStart(2, '0')
            val hour = dt.hour.toString().padStart(2, '0')
            val minute = dt.minute.toString().padStart(2, '0')
            val second = dt.second.toString().padStart(2, '0')
            val millisecond = (dt.nanosecond / 1_000_000).toString().padStart(3, '0')

            return "$year-$month-$day $hour:$minute:$second.$millisecond"
        }

        private fun ChatLineData.toChatLine(): AppState.Chat.ChatLine = when (this) {
            is ChatLineData.Message -> AppState.Chat.ChatLine.Msg(id, sender, text, sent)
            is ChatLineData.System -> AppState.Chat.ChatLine.System(text)
        }

        private fun NoteData.toNoteUi(): AppState.Chat.NoteUi =
            AppState.Chat.NoteUi(id, tag, content)
    }
}
