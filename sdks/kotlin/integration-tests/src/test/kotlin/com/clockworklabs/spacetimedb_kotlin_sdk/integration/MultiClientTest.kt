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
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertIs
import kotlin.test.assertTrue

class MultiClientTest {

    private suspend fun connectTwo(): Pair<ConnectedClient, ConnectedClient> {
        val a = connectToDb().subscribeAll()
        val b = connectToDb().subscribeAll()
        return a to b
    }

    private suspend fun cleanupBoth(a: ConnectedClient, b: ConnectedClient) {
        a.cleanup()
        b.cleanup()
    }

    // ── Message propagation ──

    @Test
    fun `client B sees message sent by client A via onInsert`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-msg-${System.nanoTime()}"
        val seen = CompletableDeferred<module_bindings.Message>()
        b.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tag) {
                seen.complete(msg)
            }
        }

        a.conn.reducers.sendMessage(tag)
        val msg = withTimeout(DEFAULT_TIMEOUT_MS) { seen.await() }

        assertEquals(tag, msg.text)
        assertEquals(a.identity, msg.sender, "Sender should be client A's identity")

        cleanupBoth(a, b)
    }

    @Test
    fun `client B cache contains message after client A sends it`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-cache-${System.nanoTime()}"
        val inserted = CompletableDeferred<ULong>()
        b.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tag) {
                inserted.complete(msg.id)
            }
        }

        a.conn.reducers.sendMessage(tag)
        val msgId = withTimeout(DEFAULT_TIMEOUT_MS) { inserted.await() }

        val cached = b.conn.db.message.id.find(msgId)
        assertNotNull(cached, "Client B cache should contain the message")
        assertEquals(tag, cached.text)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B sees message deleted by client A via onDelete`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-del-${System.nanoTime()}"

        // A sends a message, wait for B to see it
        val insertSeen = CompletableDeferred<ULong>()
        b.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tag) {
                insertSeen.complete(msg.id)
            }
        }
        a.conn.reducers.sendMessage(tag)
        val msgId = withTimeout(DEFAULT_TIMEOUT_MS) { insertSeen.await() }

        // B listens for deletion, A deletes
        val deleteSeen = CompletableDeferred<ULong>()
        b.conn.db.message.onDelete { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.id == msgId) {
                deleteSeen.complete(msg.id)
            }
        }
        a.conn.reducers.deleteMessage(msgId)

        val deletedId = withTimeout(DEFAULT_TIMEOUT_MS) { deleteSeen.await() }
        assertEquals(msgId, deletedId)
        assertEquals(null, b.conn.db.message.id.find(msgId), "Message should be gone from B's cache")

        cleanupBoth(a, b)
    }

    // ── User table propagation ──

    @Test
    fun `client B sees client A set name via onUpdate`() = runBlocking {
        val (a, b) = connectTwo()

        val newName = "multi-name-${System.nanoTime()}"
        val updateSeen = CompletableDeferred<Pair<module_bindings.User, module_bindings.User>>()
        b.conn.db.user.onUpdate { _, old, new ->
            if (new.identity == a.identity && new.name == newName) {
                updateSeen.complete(old to new)
            }
        }

        a.conn.reducers.setName(newName)
        val (old, new) = withTimeout(DEFAULT_TIMEOUT_MS) { updateSeen.await() }

        assertNotEquals(newName, old.name, "Old name should differ from the new name")
        assertEquals(newName, new.name)
        assertEquals(a.identity, new.identity)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B sees client A come online via user table`() = runBlocking {
        val b = connectToDb().subscribeAll()

        // B listens for a new user insert
        val userSeen = CompletableDeferred<module_bindings.User>()
        b.conn.db.user.onInsert { ctx, user ->
            if (ctx !is EventContext.SubscribeApplied && user.online) {
                userSeen.complete(user)
            }
        }

        val a = connectToDb().subscribeAll()

        val newUser = withTimeout(DEFAULT_TIMEOUT_MS) { userSeen.await() }
        assertEquals(a.identity, newUser.identity)
        assertTrue(newUser.online)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B sees client A go offline via onUpdate`() = runBlocking {
        val (a, b) = connectTwo()

        val offlineSeen = CompletableDeferred<module_bindings.User>()
        b.conn.db.user.onUpdate { _, old, new ->
            if (new.identity == a.identity && old.online && !new.online) {
                offlineSeen.complete(new)
            }
        }

        a.conn.disconnect()

        val offlineUser = withTimeout(DEFAULT_TIMEOUT_MS) { offlineSeen.await() }
        assertEquals(a.identity, offlineUser.identity)
        assertFalse(offlineUser.online)

        // Only cleanup B (A already disconnected)
        b.cleanup()
    }

    // ── Note propagation ──

    @Test
    fun `client B sees note added by client A`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-note-${System.nanoTime()}"
        val noteSeen = CompletableDeferred<module_bindings.Note>()
        b.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.tag == tag) {
                noteSeen.complete(note)
            }
        }

        a.conn.reducers.addNote("content from A", tag)
        val note = withTimeout(DEFAULT_TIMEOUT_MS) { noteSeen.await() }

        assertEquals(a.identity, note.owner)
        assertEquals("content from A", note.content)
        assertEquals(tag, note.tag)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B sees note deleted by client A`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-notedel-${System.nanoTime()}"

        // A adds note, B waits for it
        val insertSeen = CompletableDeferred<ULong>()
        b.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.tag == tag) {
                insertSeen.complete(note.id)
            }
        }
        a.conn.reducers.addNote("to-delete", tag)
        val noteId = withTimeout(DEFAULT_TIMEOUT_MS) { insertSeen.await() }

        // B listens for deletion, A deletes
        val deleteSeen = CompletableDeferred<ULong>()
        b.conn.db.note.onDelete { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.id == noteId) {
                deleteSeen.complete(note.id)
            }
        }
        a.conn.reducers.deleteNote(noteId)

        val deletedId = withTimeout(DEFAULT_TIMEOUT_MS) { deleteSeen.await() }
        assertEquals(noteId, deletedId)

        cleanupBoth(a, b)
    }

    // ── EventContext cross-client ──

    @Test
    fun `client A onInsert context is Reducer for own call`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-ctx-own-${System.nanoTime()}"
        val ctxSeen = CompletableDeferred<EventContext>()
        a.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tag) {
                ctxSeen.complete(ctx)
            }
        }

        a.conn.reducers.sendMessage(tag)
        val ctx = withTimeout(DEFAULT_TIMEOUT_MS) { ctxSeen.await() }
        assertIs<EventContext.Reducer<*>>(ctx, "Own reducer should produce Reducer context, got: ${ctx::class.simpleName}")
        assertEquals(a.identity, ctx.callerIdentity)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B onInsert context is Transaction for other client's call`() = runBlocking {
        val (a, b) = connectTwo()

        val tag = "multi-ctx-other-${System.nanoTime()}"
        val ctxSeen = CompletableDeferred<EventContext>()
        b.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tag) {
                ctxSeen.complete(ctx)
            }
        }

        a.conn.reducers.sendMessage(tag)
        val ctx = withTimeout(DEFAULT_TIMEOUT_MS) { ctxSeen.await() }
        assertTrue(
            ctx is EventContext.Transaction,
            "Cross-client reducer should produce Transaction context, got: ${ctx::class.simpleName}"
        )

        cleanupBoth(a, b)
    }

    // ── Concurrent operations ──

    @Test
    fun `both clients send messages and both see all messages`() = runBlocking {
        val (a, b) = connectTwo()

        val tagA = "multi-both-a-${System.nanoTime()}"
        val tagB = "multi-both-b-${System.nanoTime()}"

        // A waits to see B's message, B waits to see A's message
        val aSeesB = CompletableDeferred<module_bindings.Message>()
        val bSeesA = CompletableDeferred<module_bindings.Message>()

        a.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tagB) {
                aSeesB.complete(msg)
            }
        }
        b.conn.db.message.onInsert { ctx, msg ->
            if (ctx !is EventContext.SubscribeApplied && msg.text == tagA) {
                bSeesA.complete(msg)
            }
        }

        // Both send simultaneously
        a.conn.reducers.sendMessage(tagA)
        b.conn.reducers.sendMessage(tagB)

        val msgFromB = withTimeout(DEFAULT_TIMEOUT_MS) { aSeesB.await() }
        val msgFromA = withTimeout(DEFAULT_TIMEOUT_MS) { bSeesA.await() }

        assertEquals(tagB, msgFromB.text)
        assertEquals(b.identity, msgFromB.sender)
        assertEquals(tagA, msgFromA.text)
        assertEquals(a.identity, msgFromA.sender)

        cleanupBoth(a, b)
    }

    @Test
    fun `client B count updates after client A inserts`() = runBlocking {
        val (a, b) = connectTwo()

        val beforeCount = b.conn.db.note.count()

        val tag = "multi-count-${System.nanoTime()}"
        val insertSeen = CompletableDeferred<Unit>()
        b.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied && note.tag == tag) {
                insertSeen.complete(Unit)
            }
        }

        a.conn.reducers.addNote("count-test", tag)
        withTimeout(DEFAULT_TIMEOUT_MS) { insertSeen.await() }

        assertEquals(beforeCount + 1, b.conn.db.note.count(), "B's cache count should increment")

        cleanupBoth(a, b)
    }

    // ── Identity isolation ──

    @Test
    fun `two anonymous clients have different identities`() = runBlocking {
        val (a, b) = connectTwo()

        assertNotEquals(a.identity, b.identity, "Two anonymous clients should have different identities")

        cleanupBoth(a, b)
    }

    @Test
    fun `client B can look up client A by identity in user table`() = runBlocking {
        val (a, b) = connectTwo()

        val userA = b.conn.db.user.identity.find(a.identity)
        assertNotNull(userA, "Client B should find client A in user table")
        assertTrue(userA.online, "Client A should be online")

        cleanupBoth(a, b)
    }
}
