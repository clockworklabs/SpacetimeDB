package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Status
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Reducer/row callback interaction tests.
 */
class ReducerCallbackOrderTest {

    // --- Row callbacks fire during reducer event ---

    @Test
    fun `onInsert fires during reducer callback`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val events = mutableListOf<String>()
        val done = CompletableDeferred<Unit>()

        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.owner == client.identity && note.tag == "order-test") {
                events.add("onInsert")
            }
        }

        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) {
                events.add("onReducer")
                done.complete(Unit)
            }
        }

        client.conn.reducers.addNote("order-test-content", "order-test")
        withTimeout(DEFAULT_TIMEOUT_MS) { done.await() }

        assertTrue(events.contains("onInsert"), "onInsert should have fired: $events")
        assertTrue(events.contains("onReducer"), "onReducer should have fired: $events")
        // Both should fire in the same transaction update
        assertEquals(2, events.size, "Should have exactly 2 events: $events")
    }

    // --- Failed reducer produces Status.Failed ---

    @Test
    fun `failed reducer has Status Failed`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val status = CompletableDeferred<Status>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) {
                status.complete(ctx.status)
            }
        }

        // Empty content triggers validation error
        client.conn.reducers.addNote("", "fail-test")
        val result = withTimeout(DEFAULT_TIMEOUT_MS) { status.await() }
        assertTrue(result is Status.Failed, "Empty content should fail: $result")
    }

    @Test
    fun `failed reducer does not fire onInsert`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        var insertFired = false
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.owner == client.identity && note.tag == "no-insert-test") {
                insertFired = true
            }
        }

        val reducerDone = CompletableDeferred<Unit>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) {
                reducerDone.complete(Unit)
            }
        }

        // Empty content → validation error → no row inserted
        client.conn.reducers.addNote("", "no-insert-test")
        withTimeout(DEFAULT_TIMEOUT_MS) { reducerDone.await() }
        kotlinx.coroutines.delay(200)

        assertTrue(!insertFired, "onInsert should NOT fire for failed reducer")
        client.cleanup()
    }

    @Test
    fun `failed reducer error message is available`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val errorMsg = CompletableDeferred<String>()
        client.conn.reducers.onSendMessage { ctx, _ ->
            if (ctx.callerIdentity == client.identity) {
                val s = ctx.status
                if (s is Status.Failed) {
                    errorMsg.complete(s.message)
                }
            }
        }

        // Empty message triggers validation error
        client.conn.reducers.sendMessage("")
        val msg = withTimeout(DEFAULT_TIMEOUT_MS) { errorMsg.await() }
        assertTrue(msg.contains("must not be empty"), "Error message should explain: $msg")

        client.cleanup()
    }

    // --- onUpdate fires for modified row ---

    @Test
    fun `onUpdate fires when row is modified`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        // Set initial name
        val nameDone1 = CompletableDeferred<Unit>()
        client.conn.reducers.onSetName { ctx, _ ->
            if (ctx.callerIdentity == client.identity && ctx.status is Status.Committed) {
                nameDone1.complete(Unit)
            }
        }
        val uniqueName1 = "update-test-${System.nanoTime()}"
        client.conn.reducers.setName(uniqueName1)
        withTimeout(DEFAULT_TIMEOUT_MS) { nameDone1.await() }

        // Register onUpdate, then change name again
        val updateDone = CompletableDeferred<Pair<String?, String?>>()
        client.conn.db.user.onUpdate { ctx, oldRow, newRow ->
            if (ctx !is EventContext.SubscribeApplied
                && newRow.identity == client.identity
                && oldRow.name == uniqueName1) {
                updateDone.complete(oldRow.name to newRow.name)
            }
        }

        val uniqueName2 = "update-test2-${System.nanoTime()}"
        client.conn.reducers.setName(uniqueName2)
        val (oldName, newName) = withTimeout(DEFAULT_TIMEOUT_MS) { updateDone.await() }

        assertEquals(uniqueName1, oldName, "Old name should be first name")
        assertEquals(uniqueName2, newName, "New name should be second name")

        client.cleanup()
    }

    // --- Reducer callerIdentity matches connection ---

    @Test
    fun `reducer context has correct callerIdentity`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val callerIdentity = CompletableDeferred<Identity>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) {
                callerIdentity.complete(ctx.callerIdentity)
            }
        }

        client.conn.reducers.addNote("identity-check", "id-test")
        val identity = withTimeout(DEFAULT_TIMEOUT_MS) { callerIdentity.await() }
        assertEquals(client.identity, identity)

        client.cleanup()
    }

    @Test
    fun `reducer context has reducerName`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val name = CompletableDeferred<String>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) {
                name.complete(ctx.reducerName)
            }
        }

        client.conn.reducers.addNote("name-check", "rn-test")
        val reducerName = withTimeout(DEFAULT_TIMEOUT_MS) { name.await() }
        assertEquals("add_note", reducerName)

        client.cleanup()
    }

    @Test
    fun `reducer context has args matching what was sent`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val argsContent = CompletableDeferred<String>()
        val argsTag = CompletableDeferred<String>()
        client.conn.reducers.onAddNote { ctx, content, tag ->
            if (ctx.callerIdentity == client.identity) {
                argsContent.complete(content)
                argsTag.complete(tag)
            }
        }

        client.conn.reducers.addNote("specific-content-xyz", "specific-tag-abc")
        assertEquals("specific-content-xyz", withTimeout(DEFAULT_TIMEOUT_MS) { argsContent.await() })
        assertEquals("specific-tag-abc", withTimeout(DEFAULT_TIMEOUT_MS) { argsTag.await() })

        client.cleanup()
    }

    // --- Multi-client: one client's reducer is observed by another ---

    @Test
    fun `client B observes client A reducer via onInsert`() = runBlocking {
        val clientA = connectToDb()
        val clientB = connectToDb()
        clientA.subscribeAll()
        clientB.subscribeAll()

        val tag = "multi-client-${System.nanoTime()}"

        val bSawInsert = CompletableDeferred<Boolean>()
        clientB.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.tag == tag) {
                bSawInsert.complete(true)
            }
        }

        clientA.conn.reducers.addNote("hello from A", tag)
        val result = withTimeout(DEFAULT_TIMEOUT_MS) { bSawInsert.await() }
        assertTrue(result, "Client B should see client A's insert")

        clientA.cleanup()
        clientB.cleanup()
    }

    @Test
    fun `client B observes client A name change via onUpdate`() = runBlocking {
        val clientA = connectToDb()
        val clientB = connectToDb()
        clientA.subscribeAll()
        clientB.subscribeAll()

        val uniqueName = "multi-update-${System.nanoTime()}"

        val bSawUpdate = CompletableDeferred<String?>()
        clientB.conn.db.user.onUpdate { ctx, _, newRow ->
            if (ctx !is EventContext.SubscribeApplied && newRow.name == uniqueName) {
                bSawUpdate.complete(newRow.name)
            }
        }

        clientA.conn.reducers.setName(uniqueName)
        val name = withTimeout(DEFAULT_TIMEOUT_MS) { bSawUpdate.await() }
        assertEquals(uniqueName, name)

        clientA.cleanup()
        clientB.cleanup()
    }
}
