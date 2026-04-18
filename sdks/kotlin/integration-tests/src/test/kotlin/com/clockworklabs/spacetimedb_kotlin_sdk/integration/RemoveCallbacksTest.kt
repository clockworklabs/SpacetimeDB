package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.User
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertTrue

class RemoveCallbacksTest {

    @Test
    fun `removeOnUpdate prevents callback from firing`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        var callbackFired = false
        val cb: (EventContext, User, User) -> Unit = { _, _, _ -> callbackFired = true }

        client.conn.db.user.onUpdate(cb)
        client.conn.db.user.removeOnUpdate(cb)

        // Trigger an update by setting name
        val done = CompletableDeferred<Unit>()
        client.conn.reducers.onSetName { ctx, _ ->
            if (ctx.callerIdentity == client.identity) done.complete(Unit)
        }
        client.conn.reducers.setName("removeOnUpdate-test-${System.nanoTime()}")
        withTimeout(DEFAULT_TIMEOUT_MS) { done.await() }

        kotlinx.coroutines.delay(200)
        assertTrue(!callbackFired, "Removed onUpdate callback should not fire")

        client.cleanup()
    }

    @Test
    fun `removeOnBeforeDelete prevents callback from firing`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        var callbackFired = false
        val cb: (EventContext, module_bindings.Note) -> Unit = { _, _ -> callbackFired = true }

        client.conn.db.note.onBeforeDelete(cb)
        client.conn.db.note.removeOnBeforeDelete(cb)

        // Insert then delete a note
        val insertDone = CompletableDeferred<ULong>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "rm-bd-test") {
                insertDone.complete(note.id)
            }
        }
        client.conn.reducers.addNote("removeOnBeforeDelete-test", "rm-bd-test")
        val noteId = withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        val delDone = CompletableDeferred<Unit>()
        client.conn.reducers.onDeleteNote { ctx, _ ->
            if (ctx.callerIdentity == client.identity) delDone.complete(Unit)
        }
        client.conn.reducers.deleteNote(noteId)
        withTimeout(DEFAULT_TIMEOUT_MS) { delDone.await() }

        kotlinx.coroutines.delay(200)
        assertTrue(!callbackFired, "Removed onBeforeDelete callback should not fire")

        client.cleanup()
    }
}
