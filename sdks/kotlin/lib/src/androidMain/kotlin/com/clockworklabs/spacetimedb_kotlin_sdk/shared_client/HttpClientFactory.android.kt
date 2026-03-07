package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.websocket.WebSockets
import okhttp3.Dns
import java.net.Inet4Address
import java.net.InetAddress

private val Ipv4FirstDns = object : Dns {
    override fun lookup(hostname: String): List<InetAddress> {
        return Dns.SYSTEM.lookup(hostname)
            .sortedBy { if (it is Inet4Address) 0 else 1 }
    }
}

internal actual fun createPlatformHttpClient(): HttpClient {
    return HttpClient(OkHttp) {
        engine {
            config {
                dns(Ipv4FirstDns)
            }
        }
        install(WebSockets)
        install(HttpTimeout) {
            connectTimeoutMillis = 10_000
        }
    }
}
