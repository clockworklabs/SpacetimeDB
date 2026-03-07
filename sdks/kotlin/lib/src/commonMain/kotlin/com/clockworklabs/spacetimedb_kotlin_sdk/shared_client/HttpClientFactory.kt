package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import io.ktor.client.HttpClient

internal expect fun createPlatformHttpClient(): HttpClient
