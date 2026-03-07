package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import io.ktor.client.HttpClient
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.websocket.WebSockets

internal actual fun createPlatformHttpClient(): HttpClient {
    return HttpClient {
        install(WebSockets)
        install(HttpTimeout) {
            connectTimeoutMillis = 10_000
        }
    }
}
