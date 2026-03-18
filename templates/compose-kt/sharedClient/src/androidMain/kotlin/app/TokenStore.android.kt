package app

import android.content.Context
import java.io.File

actual class TokenStore(private val context: Context) {
    private val tokenDir: File
        get() = File(context.filesDir, "spacetimedb/tokens")

    actual fun load(clientId: String): String? {
        val file = File(tokenDir, clientId)
        return if (file.exists()) file.readText().trim() else null
    }

    actual fun save(clientId: String, token: String) {
        tokenDir.mkdirs()
        File(tokenDir, clientId).writeText(token)
    }
}
