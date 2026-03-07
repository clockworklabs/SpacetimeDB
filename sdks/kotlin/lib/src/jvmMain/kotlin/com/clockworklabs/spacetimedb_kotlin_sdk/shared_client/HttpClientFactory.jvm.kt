package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.websocket.WebSockets
import okhttp3.Dns
import java.net.Inet4Address
import java.net.InetAddress

/**
 * OkHttp resolves "localhost" to both [::1] and 127.0.0.1 and tries IPv6 first.
 * If the server only listens on IPv4, the connection fails or has a long delay.
 * Sorting IPv4 addresses first matches the behavior of C# and TS SDKs.
 */
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
