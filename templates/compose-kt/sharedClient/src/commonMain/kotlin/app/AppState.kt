package app

import androidx.compose.runtime.Immutable
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Immutable
data class FieldInput(val value: String = "", val error: String? = null)

@Immutable
data class AppState(
    val login: Login = Login(),
    val chat: Chat = Chat(),
    val currentScreen: Screen = Screen.LOGIN,
) {
    enum class Screen {
        LOGIN, CHAT
    }

    @Immutable
    data class Login(
        val clientIdField: FieldInput = FieldInput(),
        val hostField: FieldInput = FieldInput(),
    )

    @Immutable
    data class Chat(
        val lines: ImmutableList<ChatLine> = persistentListOf(),
        val input: String = "",
        val connected: Boolean = false,
        val onlineUsers: ImmutableList<String> = persistentListOf(),
        val offlineUsers: ImmutableList<String> = persistentListOf(),
        val notes: ImmutableList<NoteUi> = persistentListOf(),
        val noteSubState: String = "none",
        val dbName: String = "",
    ) {

        @Immutable
        sealed interface ChatLine {
            @Immutable
            data class Msg(
                val id: ULong,
                val sender: String,
                val text: String,
                val sent: Timestamp,
            ) : ChatLine

            @Immutable
            data class System(val text: String) : ChatLine
        }

        @Immutable
        data class NoteUi(
            val id: ULong,
            val tag: String,
            val content: String,
        )
    }
}
