package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class BuilderAndCallbackTest {

    // --- Builder validation ---

    @Test
    fun `builder fails without uri`() = runTest {
        assertFailsWith<IllegalArgumentException> {
            DbConnection.Builder()
                .withDatabaseName("test")
                .build()
        }
    }

    @Test
    fun `builder fails without database name`() = runTest {
        assertFailsWith<IllegalArgumentException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .build()
        }
    }

    // --- Builder ensureMinimumVersion ---

    @Test
    fun `builder rejects old cli version`() = runTest {
        val oldModule = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "1.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        assertFailsWith<IllegalStateException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .withDatabaseName("test")
                .withModule(oldModule)
                .build()
        }
    }

    // --- ensureMinimumVersion edge cases ---

    @Test
    fun `builder accepts exact minimum version`() = runTest {
        val module = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Should not throw — 2.0.0 is the exact minimum
        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun `builder accepts newer version`() = runTest {
        val module = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "3.1.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun `builder accepts pre release suffix`() = runTest {
        val module = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "2.1.0-beta.1"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Pre-release suffix is stripped; 2.1.0 >= 2.0.0
        val conn = buildTestConnection(FakeTransport(), moduleDescriptor = module)
        conn.disconnect()
    }

    @Test
    fun `builder rejects old minor version`() = runTest {
        val module = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "1.9.9"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        assertFailsWith<IllegalStateException> {
            DbConnection.Builder()
                .withUri("ws://localhost:3000")
                .withDatabaseName("test")
                .withModule(module)
                .build()
        }
    }

    // --- Module descriptor integration ---

    @Test
    fun `db connection constructor does not call register tables`() = runTest {
        val transport = FakeTransport()
        var tablesRegistered = false

        val descriptor = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {
                tablesRegistered = true
                cache.register("sample", createSampleCache())
            }
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {}
        }

        // Use the module descriptor through DbConnection — pass it via the helper
        val conn = buildTestConnection(transport, moduleDescriptor = descriptor)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // registerTables is the Builder's responsibility, not DbConnection's
        assertFalse(tablesRegistered)
        assertNull(conn.clientCache.getUntypedTable("sample"))
        conn.disconnect()
    }

    // --- handleReducerEvent fires from module descriptor ---

    @Test
    fun `module descriptor handle reducer event fires`() = runTest {
        val transport = FakeTransport()
        var reducerEventName: String? = null

        val descriptor = object : ModuleDescriptor {
            override val subscribableTableNames = emptyList<String>()
            override val cliVersion = "2.0.0"
            override fun registerTables(cache: ClientCache) {}
            override fun createAccessors(conn: DbConnection) = ModuleAccessors(
                object : ModuleTables {},
                object : ModuleReducers {},
                object : ModuleProcedures {},
            )
            override fun handleReducerEvent(conn: DbConnection, ctx: EventContext.Reducer<*>) {
                reducerEventName = ctx.reducerName
            }
        }

        val conn = buildTestConnection(transport, moduleDescriptor = descriptor)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        conn.callReducer("myReducer", byteArrayOf(), "args", callback = null)
        advanceUntilIdle()

        val sent = transport.sentMessages.filterIsInstance<ClientMessage.CallReducer>().last()
        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = sent.requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertEquals("myReducer", reducerEventName)
        conn.disconnect()
    }

    // --- Callback removal ---

    @Test
    fun `remove on disconnect prevents callback`() = runTest {
        val transport = FakeTransport()
        var fired = false
        val cb: (DbConnectionView, Throwable?) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onDisconnect = cb)
        conn.removeOnDisconnect(cb)
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        transport.closeFromServer()
        advanceUntilIdle()

        assertFalse(fired)
        conn.disconnect()
    }

    // --- removeOnConnectError ---

    @Test
    fun `remove on connect error prevents callback`() = runTest {
        val transport = FakeTransport(connectError = RuntimeException("fail"))
        var fired = false
        val cb: (DbConnectionView, Throwable) -> Unit = { _, _ -> fired = true }

        val conn = createTestConnection(transport, onConnectError = cb)
        conn.removeOnConnectError(cb)

        try {
            conn.connect()
        } catch (_: Exception) { }
        advanceUntilIdle()

        assertFalse(fired)
        conn.disconnect()
    }

    // --- Multiple callbacks ---

    @Test
    fun `multiple on connect callbacks all fire`() = runTest {
        val transport = FakeTransport()
        var count = 0
        val cb: (DbConnectionView, Identity, String) -> Unit = { _, _, _ -> count++ }
        val conn = DbConnection(
            transport = transport,
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOf(cb, cb, cb),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()

        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertEquals(3, count)
        conn.disconnect()
    }

    // --- User callback exception does not crash receive loop ---

    @Test
    fun `user callback exception does not crash connection`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Register a callback that throws
        cache.onInsert { _, _ -> error("callback explosion") }

        val row = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()

        // Row should still be inserted despite callback exception
        assertEquals(1, cache.count())
        // Connection should still be active
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    // --- Callback exception handling ---

    @Test
    fun `on connect callback exception does not prevent other callbacks`() = runTest {
        val transport = FakeTransport()
        var secondFired = false
        val conn = DbConnection(
            transport = transport,
            scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
            onConnectCallbacks = listOf(
                { _, _, _ -> error("onConnect explosion") },
                { _, _, _ -> secondFired = true },
            ),
            onDisconnectCallbacks = emptyList(),
            onConnectErrorCallbacks = emptyList(),
            clientConnectionId = ConnectionId.random(),
            stats = Stats(),
            moduleDescriptor = null,
            callbackDispatcher = null,
        )
        conn.connect()
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        assertTrue(secondFired, "Second onConnect callback should fire despite first throwing")
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun `on delete callback exception does not prevent row removal`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        val cache = createSampleCache()
        conn.clientCache.register("sample", cache)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        // Insert a row first
        val row = SampleRow(1, "Alice")
        val handle = conn.subscribe(listOf("SELECT * FROM sample"))
        transport.sendToClient(
            ServerMessage.SubscribeApplied(
                requestId = 1u,
                querySetId = handle.querySetId,
                rows = QueryRows(listOf(SingleTableRows("sample", buildRowList(row.encode())))),
            )
        )
        advanceUntilIdle()
        assertEquals(1, cache.count())

        // Register a throwing onDelete callback
        cache.onDelete { _, _ -> error("delete callback explosion") }

        // Delete the row via transaction update
        transport.sendToClient(
            ServerMessage.TransactionUpdateMsg(
                update = TransactionUpdate(
                    listOf(
                        QuerySetUpdate(
                            handle.querySetId,
                            listOf(
                                TableUpdate(
                                    "sample",
                                    listOf(TableUpdateRows.PersistentTable(
                                        inserts = buildRowList(),
                                        deletes = buildRowList(row.encode()),
                                    ))
                                )
                            ),
                        )
                    )
                )
            )
        )
        advanceUntilIdle()

        // Row should still be deleted despite callback exception
        assertEquals(0, cache.count())
        assertTrue(conn.isActive)
        conn.disconnect()
    }

    @Test
    fun `reducer callback exception does not crash connection`() = runTest {
        val transport = FakeTransport()
        val conn = buildTestConnection(transport)
        transport.sendToClient(initialConnectionMsg())
        advanceUntilIdle()

        val requestId = conn.callReducer(
            reducerName = "boom",
            encodedArgs = byteArrayOf(),
            typedArgs = "args",
            callback = { _ -> error("reducer callback explosion") },
        )
        advanceUntilIdle()

        transport.sendToClient(
            ServerMessage.ReducerResultMsg(
                requestId = requestId,
                timestamp = Timestamp.UNIX_EPOCH,
                result = ReducerOutcome.OkEmpty,
            )
        )
        advanceUntilIdle()

        assertTrue(conn.isActive, "Connection should survive throwing reducer callback")
        conn.disconnect()
    }
}
