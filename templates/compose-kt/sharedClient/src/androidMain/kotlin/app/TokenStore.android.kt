package app

import java.io.File

private val tokenDir = File(System.getProperty("user.home", "."), ".spacetimedb/tokens")

actual fun loadToken(clientId: String): String? {
    val file = File(tokenDir, clientId)
    return if (file.exists()) file.readText().trim() else null
}

actual fun saveToken(clientId: String, token: String) {
    tokenDir.mkdirs()
    File(tokenDir, clientId).writeText(token)
}
