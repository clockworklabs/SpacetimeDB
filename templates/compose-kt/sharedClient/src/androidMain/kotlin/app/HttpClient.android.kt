package app

import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets

actual fun createHttpClient(): HttpClient = HttpClient(OkHttp) { install(WebSockets) }

actual val defaultHost: String = "10.0.2.2"
