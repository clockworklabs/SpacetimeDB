package app

import android.content.Context
import java.io.File

actual class TokenStore(private val context: Context) {
    private val tokenDir: File
        get() = File(context.filesDir, "spacetimedb/tokens")

    private fun tokenFile(clientId: String): File {
        require(clientId.isNotEmpty() && clientId.all { it.isLetterOrDigit() || it == '-' || it == '_' }) {
            "Invalid clientId: must be non-empty and contain only alphanumeric, '-', or '_' characters"
        }
        return File(tokenDir, clientId)
    }

    actual fun load(clientId: String): String? {
        val file = tokenFile(clientId)
        return if (file.exists()) file.readText().trim() else null
    }

    actual fun save(clientId: String, token: String) {
        tokenDir.mkdirs()
        tokenFile(clientId).writeText(token)
    }
}
