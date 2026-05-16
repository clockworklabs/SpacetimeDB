package com.clockworklabs.spacetimedb

class EventContext<R>(
    override val identity: Identity?,
    override val connectionId: ConnectionId,
    override val savedToken: String?,
    override val isActive: Boolean,
    override val connectionState: kotlinx.coroutines.flow.StateFlow<com.clockworklabs.spacetimedb.websocket.ConnectionState>,
    val event: Event<R>,
    private val conn: DbConnection,
) : DbContext {
    override fun disconnect() = conn.disconnect()
    override fun subscriptionBuilder() = conn.subscriptionBuilder()
}
