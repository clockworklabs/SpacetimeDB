package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Status
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertIs
import kotlin.test.assertTrue

class EventContextTest {

    @Test
    fun `reducer context has callerIdentity matching our identity`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val callerIdentityDeferred = CompletableDeferred<Identity>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) callerIdentityDeferred.complete(c.callerIdentity)
        }
        client.conn.reducers.setName("ctx-test-${System.nanoTime()}")

        val callerIdentity = withTimeout(DEFAULT_TIMEOUT_MS) { callerIdentityDeferred.await() }
        assertEquals(client.identity, callerIdentity, "callerIdentity should match our identity")

        client.conn.disconnect()
    }

    @Test
    fun `reducer context has non-null callerConnectionId`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val connIdDeferred = CompletableDeferred<Any?>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) connIdDeferred.complete(c.callerConnectionId)
        }
        client.conn.reducers.setName("ctx-connid-${System.nanoTime()}")

        val connId = withTimeout(DEFAULT_TIMEOUT_MS) { connIdDeferred.await() }
        assertNotNull(connId, "callerConnectionId should not be null for our own reducer call")

        client.conn.disconnect()
    }

    @Test
    fun `successful reducer has Status Committed`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val statusDeferred = CompletableDeferred<Status>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) statusDeferred.complete(c.status)
        }
        client.conn.reducers.setName("status-ok-${System.nanoTime()}")

        val s = withTimeout(DEFAULT_TIMEOUT_MS) { statusDeferred.await() }
        assertTrue(s is Status.Committed, "Successful reducer should have Status.Committed, got: $s")

        client.conn.disconnect()
    }

    @Test
    fun `failed reducer has Status Failed`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val statusDeferred = CompletableDeferred<Status>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) statusDeferred.complete(c.status)
        }
        // Setting empty name should fail (server validates non-empty)
        client.conn.reducers.setName("")

        val s = withTimeout(DEFAULT_TIMEOUT_MS) { statusDeferred.await() }
        assertIs<Status.Failed>(s, "Empty name reducer should have Status.Failed, got: $s")
        val failedMsg = s.message
        assertTrue(failedMsg.isNotEmpty(), "Failed status should have a message: $failedMsg")

        client.conn.disconnect()
    }

    @Test
    fun `reducer context has reducerName`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val nameDeferred = CompletableDeferred<String>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) nameDeferred.complete(c.reducerName)
        }
        client.conn.reducers.setName("reducer-name-test-${System.nanoTime()}")

        val reducerName = withTimeout(DEFAULT_TIMEOUT_MS) { nameDeferred.await() }
        assertEquals("set_name", reducerName, "reducerName should be 'set_name'")

        client.conn.disconnect()
    }

    @Test
    fun `reducer context has timestamp`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val tsDeferred = CompletableDeferred<Timestamp>()
        client.conn.reducers.onSetName { c, _ ->
            if (c.callerIdentity == client.identity) tsDeferred.complete(c.timestamp)
        }
        client.conn.reducers.setName("ts-test-${System.nanoTime()}")

        val ts = withTimeout(DEFAULT_TIMEOUT_MS) { tsDeferred.await() }
        assertNotNull(ts, "timestamp should not be null")

        client.conn.disconnect()
    }

    @Test
    fun `reducer context args contain the argument passed`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val uniqueName = "args-test-${System.nanoTime()}"
        val argsDeferred = CompletableDeferred<String>()
        client.conn.reducers.onSetName { c, name ->
            if (c.callerIdentity == client.identity && name == uniqueName) {
                argsDeferred.complete(name)
            }
        }
        client.conn.reducers.setName(uniqueName)

        val receivedName = withTimeout(DEFAULT_TIMEOUT_MS) { argsDeferred.await() }
        assertEquals(uniqueName, receivedName, "Callback should receive the name argument")

        client.conn.disconnect()
    }

    @Test
    fun `onInsert receives SubscribeApplied context during initial subscription`() = runBlocking {
        val client = connectToDb()

        val gotSubscribeApplied = CompletableDeferred<Boolean>()
        client.conn.db.user.onInsert { ctx, _ ->
            if (ctx is EventContext.SubscribeApplied) {
                gotSubscribeApplied.complete(true)
            }
        }

        client.subscribeAll()

        val result = withTimeout(DEFAULT_TIMEOUT_MS) { gotSubscribeApplied.await() }
        assertTrue(result, "onInsert during subscribe should receive SubscribeApplied context")

        client.conn.disconnect()
    }

    @Test
    fun `onInsert receives non-SubscribeApplied context for live inserts`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val ctxClass = CompletableDeferred<String>()
        client.conn.db.note.onInsert { c, note ->
            if (c !is EventContext.SubscribeApplied && note.owner == client.identity && note.tag == "live-ctx") {
                ctxClass.complete(c::class.simpleName ?: "unknown")
            }
        }
        client.conn.reducers.addNote("live-context-test", "live-ctx")

        val className = withTimeout(DEFAULT_TIMEOUT_MS) { ctxClass.await() }
        assertTrue(className != "SubscribeApplied", "Live insert should NOT be SubscribeApplied, got: $className")

        client.cleanup()
    }
}
