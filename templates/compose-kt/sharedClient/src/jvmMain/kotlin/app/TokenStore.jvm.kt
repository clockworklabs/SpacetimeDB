package app

import java.io.File

actual class TokenStore {
    private val tokenDir = File(System.getProperty("user.home"), ".spacetimedb/tokens")

    actual fun load(clientId: String): String? {
        val file = File(tokenDir, clientId)
        return if (file.exists()) file.readText().trim() else null
    }

    actual fun save(clientId: String, token: String) {
        tokenDir.mkdirs()
        File(tokenDir, clientId).writeText(token)
    }
}
