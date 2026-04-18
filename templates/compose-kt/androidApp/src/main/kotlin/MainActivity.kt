package com.clockworklabs.spacetimedb_compose_kt

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory.Companion.APPLICATION_KEY
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.AppViewModel
import app.ChatRepository
import app.TokenStore
import app.composable.App
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val factory = viewModelFactory {
            initializer {
                val context = this[APPLICATION_KEY]!!
                val httpClient = HttpClient(OkHttp) { install(WebSockets) }
                val tokenStore = TokenStore(context)
                val repository = ChatRepository(httpClient, tokenStore)
                // 10.0.2.2 is the Android emulator's alias for the host machine's loopback.
                // For physical devices, replace with your machine's LAN IP (e.g. "ws://192.168.1.x:3000").
                AppViewModel(repository, defaultHost = "ws://10.0.2.2:3000")
            }
        }
        val viewModel = ViewModelProvider(this, factory)[AppViewModel::class.java]
        setContent { App(viewModel) }
    }
}
