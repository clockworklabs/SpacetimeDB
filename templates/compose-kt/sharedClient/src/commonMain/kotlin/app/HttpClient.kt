package app

import io.ktor.client.HttpClient

expect fun createHttpClient(): HttpClient

expect val defaultHost: String
