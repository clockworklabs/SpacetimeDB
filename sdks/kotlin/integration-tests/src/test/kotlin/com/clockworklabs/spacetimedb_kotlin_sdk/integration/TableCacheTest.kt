package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class TableCacheTest {

    @Test
    fun `count returns number of cached rows`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val count = client.conn.db.user.count()
        assertTrue(count > 0, "Should have at least 1 user (ourselves), got $count")

        client.conn.disconnect()
    }

    @Test
    fun `count is zero before subscribe`() = runBlocking {
        val client = connectToDb()

        // Before subscribing, cache should be empty
        assertEquals(0, client.conn.db.note.count(), "count should be 0 before subscribe")
        assertTrue(client.conn.db.note.all().isEmpty(), "all() should be empty before subscribe")
        assertFalse(client.conn.db.note.iter().any(), "iter() should have no elements before subscribe")

        client.conn.disconnect()
    }

    @Test
    fun `count updates after insert`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val before = client.conn.db.note.count()

        val insertDone = CompletableDeferred<Unit>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "count-test") {
                insertDone.complete(Unit)
            }
        }
        client.conn.reducers.addNote("count-test-content", "count-test")
        withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        val after = client.conn.db.note.count()
        assertEquals(before + 1, after, "count should increment by 1 after insert")

        client.cleanup()
    }

    @Test
    fun `iter iterates over cached rows`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val first = client.conn.db.user.iter().firstOrNull()
        assertNotNull(first, "iter() should have at least one element")

        client.conn.disconnect()
    }

    @Test
    fun `all returns list of cached rows`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val all = client.conn.db.user.all()
        assertTrue(all.isNotEmpty(), "all() should return non-empty list")
        assertEquals(client.conn.db.user.count(), all.size, "all().size should match count()")

        client.conn.disconnect()
    }

    @Test
    fun `all and count are consistent with iter`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val all = client.conn.db.user.all()
        val count = client.conn.db.user.count()
        val iterCount = client.conn.db.user.iter().count()

        assertEquals(count, all.size, "all().size should match count()")
        assertEquals(count, iterCount, "iter count should match count()")

        client.conn.disconnect()
    }

    @Test
    fun `UniqueIndex find returns row by key`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        // Look up our own user by identity (UniqueIndex)
        val user = client.conn.db.user.identity.find(client.identity)
        assertNotNull(user, "Should find our own user by identity")
        assertTrue(user.online, "Our user should be online")

        client.conn.disconnect()
    }

    @Test
    fun `UniqueIndex find returns null for missing key`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        // Note.id UniqueIndex — look up non-existent id
        val note = client.conn.db.note.id.find(ULong.MAX_VALUE)
        assertEquals(null, note, "Should return null for non-existent key")

        client.conn.disconnect()
    }

    @Test
    fun `removeOnInsert prevents callback from firing`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        var callbackFired = false
        val cb: (EventContext, module_bindings.Note) -> Unit =
            { _, _ -> callbackFired = true }

        client.conn.db.note.onInsert(cb)
        client.conn.db.note.removeOnInsert(cb)

        // Insert a note — the removed callback should NOT fire
        val done = CompletableDeferred<Unit>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) done.complete(Unit)
        }
        client.conn.reducers.addNote("remove-insert-test", "test")
        withTimeout(DEFAULT_TIMEOUT_MS) { done.await() }

        // Small delay to ensure callback would have fired if registered
        kotlinx.coroutines.delay(200)
        assertTrue(!callbackFired, "Removed onInsert callback should not fire")

        client.cleanup()
    }

    @Test
    fun `removeOnDelete prevents callback from firing`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        var callbackFired = false
        val cb: (EventContext, module_bindings.Note) -> Unit =
            { _, _ -> callbackFired = true }

        client.conn.db.note.onDelete(cb)
        client.conn.db.note.removeOnDelete(cb)

        // Insert then delete a note
        val insertDone = CompletableDeferred<ULong>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "rm-del-test") {
                insertDone.complete(note.id)
            }
        }
        client.conn.reducers.addNote("remove-delete-test", "rm-del-test")
        val noteId = withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        val delDone = CompletableDeferred<Unit>()
        client.conn.reducers.onDeleteNote { ctx, _ ->
            if (ctx.callerIdentity == client.identity) delDone.complete(Unit)
        }
        client.conn.reducers.deleteNote(noteId)
        withTimeout(DEFAULT_TIMEOUT_MS) { delDone.await() }

        kotlinx.coroutines.delay(200)
        assertTrue(!callbackFired, "Removed onDelete callback should not fire")

        client.cleanup()
    }

    @Test
    fun `onBeforeDelete fires before row is removed from cache`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        // Insert a note first
        val insertDone = CompletableDeferred<ULong>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "before-del-test") {
                insertDone.complete(note.id)
            }
        }
        client.conn.reducers.addNote("before-delete-test", "before-del-test")
        val noteId = withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        // Register onBeforeDelete — row should still be in cache when this fires
        val beforeDeleteFired = CompletableDeferred<Boolean>()
        client.conn.db.note.onBeforeDelete { _, note ->
            if (note.id == noteId) {
                // Check if the row is still findable in cache
                val stillInCache = client.conn.db.note.id.find(noteId) != null
                beforeDeleteFired.complete(stillInCache)
            }
        }

        val delDone = CompletableDeferred<Unit>()
        client.conn.reducers.onDeleteNote { ctx, _ ->
            if (ctx.callerIdentity == client.identity) delDone.complete(Unit)
        }
        client.conn.reducers.deleteNote(noteId)
        withTimeout(DEFAULT_TIMEOUT_MS) { delDone.await() }

        val wasStillInCache = withTimeout(DEFAULT_TIMEOUT_MS) { beforeDeleteFired.await() }
        assertTrue(wasStillInCache, "Row should still be in cache during onBeforeDelete")

        client.cleanup()
    }
}
