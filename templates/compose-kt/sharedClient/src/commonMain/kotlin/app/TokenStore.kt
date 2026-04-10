package app

expect class TokenStore {
    fun load(clientId: String): String?
    fun save(clientId: String, token: String)
}
