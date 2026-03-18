package app

expect fun loadToken(clientId: String): String?
expect fun saveToken(clientId: String, token: String)
