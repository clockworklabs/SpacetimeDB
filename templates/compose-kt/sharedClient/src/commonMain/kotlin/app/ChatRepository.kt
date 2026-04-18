package app

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnectionView
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Status
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SubscriptionHandle
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.onFailure
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.onSuccess
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import io.ktor.client.HttpClient
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import module_bindings.RemoteTables
import module_bindings.SpacetimeConfig
import module_bindings.User
import module_bindings.addQuery
import module_bindings.db
import module_bindings.reducers
import module_bindings.withModuleBindings

sealed interface ChatLineData {
    data class Message(
        val id: ULong,
        val sender: String,
        val text: String,
        val sent: Timestamp,
    ) : ChatLineData

    data class System(val text: String) : ChatLineData
}

data class NoteData(
    val id: ULong,
    val tag: String,
    val content: String,
)

class ChatRepository(
    private val httpClient: HttpClient,
    private val tokenStore: TokenStore,
) {
    @Volatile private var conn: DbConnection? = null
    @Volatile private var mainSubHandle: SubscriptionHandle? = null
    @Volatile private var noteSubHandle: SubscriptionHandle? = null
    @Volatile private var localIdentity: Identity? = null
    @Volatile private var clientId: String? = null

    private val _connected = MutableStateFlow(false)
    val connected: StateFlow<Boolean> = _connected.asStateFlow()

    private val _connectionError = MutableStateFlow<String?>(null)
    val connectionError: StateFlow<String?> = _connectionError.asStateFlow()

    private val _lines = MutableStateFlow<List<ChatLineData>>(emptyList())
    val lines: StateFlow<List<ChatLineData>> = _lines.asStateFlow()

    private val _onlineUsers = MutableStateFlow<List<String>>(emptyList())
    val onlineUsers: StateFlow<List<String>> = _onlineUsers.asStateFlow()

    private val _offlineUsers = MutableStateFlow<List<String>>(emptyList())
    val offlineUsers: StateFlow<List<String>> = _offlineUsers.asStateFlow()

    private val _notes = MutableStateFlow<List<NoteData>>(emptyList())
    val notes: StateFlow<List<NoteData>> = _notes.asStateFlow()

    private val _noteSubState = MutableStateFlow("none")
    val noteSubState: StateFlow<String> = _noteSubState.asStateFlow()

    fun log(text: String) {
        _lines.update { it + ChatLineData.System(text) }
    }

    suspend fun connect(clientId: String, host: String) {
        _connectionError.value = null
        this.clientId = clientId
        val connection = DbConnection.Builder()
            .withHttpClient(httpClient)
            .withUri(host)
            .withDatabaseName(SpacetimeConfig.DATABASE_NAME)
            .withToken(tokenStore.load(clientId))
            .withModuleBindings()
            .onConnect { c, identity, token ->
                localIdentity = identity
                CoroutineScope(Dispatchers.IO).launch {
                    tokenStore.save(clientId, token)
                }
                log("Identity: ${identity.toHexString().take(16)}...")

                registerTableCallbacks(c)
                registerReducerCallbacks(c)
                registerSubscriptions(c)
            }
            .onConnectError { _, e ->
                _connectionError.value = e.message ?: "Connection failed"
            }
            .onDisconnect { _, error ->
                _connected.value = false
                _onlineUsers.value = emptyList()
                _offlineUsers.value = emptyList()
                _notes.value = emptyList()
                if (error != null) {
                    log("Disconnected abnormally: $error")
                } else {
                    log("Disconnected.")
                }
            }
            .build()

        conn = connection
    }

    suspend fun disconnect() {
        conn?.disconnect()
        conn = null
        mainSubHandle = null
        noteSubHandle = null
        localIdentity = null
        clientId = null
        _connected.value = false
        _lines.value = emptyList()
        _onlineUsers.value = emptyList()
        _offlineUsers.value = emptyList()
        _notes.value = emptyList()
        _noteSubState.value = "none"
    }

    // --- Commands ---

    fun sendMessage(text: String) {
        conn?.reducers?.sendMessage(text)
    }

    fun setName(name: String) {
        conn?.reducers?.setName(name)
    }

    fun deleteMessage(id: ULong) {
        conn?.reducers?.deleteMessage(id)
    }

    fun addNote(content: String, tag: String) {
        conn?.reducers?.addNote(content, tag)
    }

    fun deleteNote(id: ULong) {
        conn?.reducers?.deleteNote(id)
    }

    fun unsubscribeNotes() {
        val handle = noteSubHandle
        if (handle != null && handle.isActive) {
            handle.unsubscribeThen { _ ->
                _notes.value = emptyList()
                _noteSubState.value = "ended"
                log("Note subscription unsubscribed.")
            }
        } else {
            log("Note subscription is not active (state: ${handle?.state})")
        }
    }

    fun resubscribeNotes() {
        val c = conn ?: return
        noteSubHandle = c.subscriptionBuilder()
            .onApplied { ctx ->
                refreshNotes(ctx.db)
                log("Note subscription re-applied (${_notes.value.size} notes).")
                _noteSubState.value = noteSubHandle?.state?.toString() ?: "applied"
            }
            .onError { _, error ->
                log("Note subscription error: $error")
            }
            .addQuery { qb -> qb.note() }
            .subscribe()
        _noteSubState.value = noteSubHandle?.state?.toString() ?: "pending"
        log("Re-subscribing to notes...")
    }

    fun oneOffQuery(sql: String) {
        val c = conn ?: return
        c.oneOffQuery(sql) { result ->
            result
                .onSuccess { data -> log("OneOffQuery OK: ${data.tableCount} table(s)") }
                .onFailure { error -> log("OneOffQuery error: $error") }
        }
        log("Executing: $sql")
    }

    suspend fun suspendOneOffQuery(sql: String) {
        val c = conn ?: return
        log("Executing (suspend): $sql")
        c.oneOffQuery(sql)
            .onSuccess { data -> log("SuspendQuery OK: ${data.tableCount} table(s)") }
            .onFailure { error -> log("SuspendQuery error: $error") }
    }

    fun scheduleReminder(text: String, delayMs: ULong) {
        conn?.reducers?.scheduleReminder(text, delayMs)
    }

    fun cancelReminder(id: ULong) {
        conn?.reducers?.cancelReminder(id)
    }

    fun scheduleReminderRepeat(text: String, intervalMs: ULong) {
        conn?.reducers?.scheduleReminderRepeat(text, intervalMs)
    }

    // --- Private ---

    private fun registerTableCallbacks(c: DbConnectionView) {
        c.db.user.onInsert { ctx, user ->
            refreshUsers(c.db)
            if (ctx !is EventContext.SubscribeApplied && user.online) {
                log("${userNameOrIdentity(user)} is online")
            }
        }

        c.db.user.onUpdate { _, oldUser, newUser ->
            refreshUsers(c.db)
            if (oldUser.name != newUser.name) {
                log("${userNameOrIdentity(oldUser)} renamed to ${newUser.name}")
            }
            if (oldUser.online != newUser.online) {
                if (newUser.online) {
                    log("${userNameOrIdentity(newUser)} connected.")
                } else {
                    log("${userNameOrIdentity(newUser)} disconnected.")
                }
            }
        }

        c.db.message.onInsert { ctx, message ->
            if (ctx is EventContext.SubscribeApplied) return@onInsert
            _lines.update {
                it + ChatLineData.Message(
                    message.id,
                    senderName(c.db, message.sender),
                    message.text,
                    message.sent,
                )
            }
        }

        c.db.message.onDelete { ctx, message ->
            if (ctx is EventContext.SubscribeApplied) return@onDelete
            _lines.update { lines ->
                lines.filter { it !is ChatLineData.Message || it.id != message.id }
            }
            log("Message #${message.id} deleted")
        }

        c.db.note.onInsert { ctx, _ ->
            if (ctx is EventContext.SubscribeApplied) return@onInsert
            refreshNotes(c.db)
        }

        c.db.note.onDelete { ctx, note ->
            if (ctx is EventContext.SubscribeApplied) return@onDelete
            refreshNotes(c.db)
            log("Note #${note.id} deleted")
        }

        c.db.reminder.onInsert { ctx, reminder ->
            if (ctx is EventContext.SubscribeApplied) return@onInsert
            log("Reminder scheduled: \"${reminder.text}\" (id=${reminder.scheduledId})")
        }

        c.db.reminder.onDelete { ctx, reminder ->
            if (ctx is EventContext.SubscribeApplied) return@onDelete
            log("Reminder consumed: \"${reminder.text}\" (id=${reminder.scheduledId})")
        }
    }

    private fun registerReducerCallbacks(c: DbConnectionView) {
        c.reducers.onSetName { ctx, name ->
            if (ctx.callerIdentity == localIdentity && ctx.status is Status.Failed) {
                log("Failed to change name to $name: ${(ctx.status as Status.Failed).message}")
            }
        }

        c.reducers.onSendMessage { ctx, text ->
            if (ctx.callerIdentity == localIdentity && ctx.status is Status.Failed) {
                log("Failed to send message \"$text\": ${(ctx.status as Status.Failed).message}")
            }
        }

        c.reducers.onDeleteMessage { ctx, messageId ->
            if (ctx.callerIdentity == localIdentity && ctx.status is Status.Failed) {
                log("Failed to delete message #$messageId: ${(ctx.status as Status.Failed).message}")
            }
        }

        c.reducers.onAddNote { ctx, _, tag ->
            if (ctx.callerIdentity == localIdentity) {
                if (ctx.status is Status.Committed) {
                    log("Note added (tag=$tag)")
                } else if (ctx.status is Status.Failed) {
                    log("Failed to add note: ${(ctx.status as Status.Failed).message}")
                }
            }
        }

        c.reducers.onDeleteNote { ctx, noteId ->
            if (ctx.callerIdentity == localIdentity && ctx.status is Status.Failed) {
                log("Failed to delete note #$noteId: ${(ctx.status as Status.Failed).message}")
            }
        }

        c.reducers.onScheduleReminder { ctx, text, delayMs ->
            if (ctx.callerIdentity == localIdentity) {
                if (ctx.status is Status.Committed) {
                    log("Reminder scheduled in ${delayMs}ms: \"$text\"")
                } else if (ctx.status is Status.Failed) {
                    log("Failed to schedule reminder: ${(ctx.status as Status.Failed).message}")
                }
            }
        }

        c.reducers.onCancelReminder { ctx, reminderId ->
            if (ctx.callerIdentity == localIdentity) {
                if (ctx.status is Status.Committed) {
                    log("Reminder #$reminderId cancelled")
                } else if (ctx.status is Status.Failed) {
                    log("Failed to cancel reminder #$reminderId: ${(ctx.status as Status.Failed).message}")
                }
            }
        }

        c.reducers.onScheduleReminderRepeat { ctx, text, intervalMs ->
            if (ctx.callerIdentity == localIdentity) {
                if (ctx.status is Status.Committed) {
                    log("Repeating reminder every ${intervalMs}ms: \"$text\"")
                } else if (ctx.status is Status.Failed) {
                    log("Failed to schedule repeating reminder: ${(ctx.status as Status.Failed).message}")
                }
            }
        }
    }

    private fun registerSubscriptions(c: DbConnectionView) {
        mainSubHandle = c.subscriptionBuilder()
            .onApplied { ctx ->
                _connected.value = true
                refreshUsers(ctx.db)
                val initialMessages = ctx.db.message.all()
                    .sortedBy { it.sent }
                    .map { msg ->
                        ChatLineData.Message(
                            msg.id,
                            senderName(ctx.db, msg.sender),
                            msg.text,
                            msg.sent,
                        )
                    }
                _lines.update { initialMessages }
                log("Main subscription applied.")
            }
            .onError { _, error ->
                log("Main subscription error: $error")
            }
            .subscribe(
                listOf(
                    "SELECT * FROM user",
                    "SELECT * FROM message",
                    "SELECT * FROM reminder",
                )
            )

        // Type-safe query builder — equivalent to .subscribe("SELECT * FROM note")
        noteSubHandle = c.subscriptionBuilder()
            .onApplied { ctx ->
                refreshNotes(ctx.db)
                log("Note subscription applied (${_notes.value.size} notes).")
                _noteSubState.value = noteSubHandle?.state?.toString() ?: "applied"
            }
            .onError { _, error ->
                log("Note subscription error: $error")
            }
            .addQuery { qb -> qb.note() }
            .subscribe()
        _noteSubState.value = noteSubHandle?.state?.toString() ?: "pending"
    }

    private fun refreshUsers(db: RemoteTables) {
        val all = db.user.all()
        _onlineUsers.value = all.filter { it.online }.map { userNameOrIdentity(it) }
        _offlineUsers.value = all.filter { !it.online }.map { userNameOrIdentity(it) }
    }

    private fun refreshNotes(db: RemoteTables) {
        _notes.value = db.note.all().map { NoteData(it.id, it.tag, it.content) }
    }

    companion object {
        private fun userNameOrIdentity(user: User): String =
            user.name ?: user.identity.toHexString().take(8)

        private fun senderName(db: RemoteTables, sender: Identity): String {
            val user = db.user.identity.find(sender)
            return if (user != null) userNameOrIdentity(user) else "unknown"
        }
    }
}
