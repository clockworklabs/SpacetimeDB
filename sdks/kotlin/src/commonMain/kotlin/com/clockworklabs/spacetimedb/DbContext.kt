package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.websocket.ConnectionState
import kotlinx.coroutines.flow.StateFlow

interface DbContext {
    val identity: Identity?
    val connectionId: ConnectionId
    val savedToken: String?
    val isActive: Boolean
    val connectionState: StateFlow<ConnectionState>

    fun disconnect()
    fun subscriptionBuilder(): SubscriptionBuilder
}
