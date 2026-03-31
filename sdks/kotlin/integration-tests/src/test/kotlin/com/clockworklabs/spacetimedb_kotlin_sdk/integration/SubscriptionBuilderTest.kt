package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SubscriptionError
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SubscriptionState
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.db
import module_bindings.subscribeToAllTables
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SubscriptionBuilderTest {

    @Test
    fun `addQuery with subscribe builds multi-query subscription`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery("SELECT * FROM user")
            .addQuery("SELECT * FROM message")
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        val users = client.conn.db.user.all()
        assertTrue(users.isNotEmpty(), "Should see at least our own user after subscribe")

        client.conn.disconnect()
    }

    @Test
    fun `subscribeToAllTables subscribes to every table`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribeToAllTables()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        val users = client.conn.db.user.all()
        assertTrue(users.isNotEmpty(), "Should see at least our own user after subscribeToAllTables")

        client.conn.disconnect()
    }

    @Test
    fun `subscribe with no queries throws`() = runBlocking {
        val client = connectToDb()

        assertFailsWith<IllegalStateException> {
            client.conn.subscriptionBuilder()
                .onApplied { _ -> }
                .subscribe()
        }

        client.conn.disconnect()
    }

    @Test
    fun `onError fires on invalid SQL`() = runBlocking {
        val client = connectToDb()
        val error = CompletableDeferred<SubscriptionError>()

        client.conn.subscriptionBuilder()
            .onApplied { _ -> error.completeExceptionally(AssertionError("Should not apply invalid SQL")) }
            .onError { _, err -> error.complete(err) }
            .subscribe("THIS IS NOT VALID SQL")

        val err = withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertTrue(err is SubscriptionError.ServerError, "Should be ServerError")
        assertTrue(err.message.isNotEmpty(), "Error message should be non-empty: ${err.message}")

        client.conn.disconnect()
    }

    @Test
    fun `multiple onApplied callbacks all fire`() = runBlocking {
        val client = connectToDb()
        val first = CompletableDeferred<Unit>()
        val second = CompletableDeferred<Unit>()

        client.conn.subscriptionBuilder()
            .onApplied { _ -> first.complete(Unit) }
            .onApplied { _ -> second.complete(Unit) }
            .onError { _, err -> first.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { first.await() }
        withTimeout(DEFAULT_TIMEOUT_MS) { second.await() }

        client.conn.disconnect()
    }

    @Test
    fun `subscription handle state transitions from pending to active`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        // Immediately after subscribe(), handle should be pending
        // (may already be active if server responds fast, so check both)
        assertTrue(
            handle.state == SubscriptionState.PENDING || handle.state == SubscriptionState.ACTIVE,
            "State should be PENDING or ACTIVE immediately after subscribe, got: ${handle.state}"
        )

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        assertEquals(SubscriptionState.ACTIVE, handle.state, "State should be ACTIVE after onApplied")
        assertTrue(handle.isActive, "isActive should be true")
        assertFalse(handle.isPending, "isPending should be false")

        client.conn.disconnect()
    }

    @Test
    fun `unsubscribeThen transitions handle to ended`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM note")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        assertTrue(handle.isActive)

        val unsubDone = CompletableDeferred<Unit>()
        handle.unsubscribeThen { _ -> unsubDone.complete(Unit) }
        withTimeout(DEFAULT_TIMEOUT_MS) { unsubDone.await() }

        assertEquals(SubscriptionState.ENDED, handle.state, "State should be ENDED after unsubscribe")
        assertFalse(handle.isActive, "isActive should be false after unsubscribe")
    }

    @Test
    fun `queries contains the subscribed query`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        assertEquals(1, handle.queries.size, "Should have 1 query")
        assertEquals("SELECT * FROM user", handle.queries[0])

        client.conn.disconnect()
    }

    @Test
    fun `queries contains multiple subscribed queries`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery("SELECT * FROM user")
            .addQuery("SELECT * FROM note")
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        assertEquals(2, handle.queries.size, "Should have 2 queries")
        assertTrue(handle.queries.contains("SELECT * FROM user"))
        assertTrue(handle.queries.contains("SELECT * FROM note"))

        client.conn.disconnect()
    }

    @Test
    fun `isUnsubscribing is false while active`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        assertFalse(handle.isUnsubscribing, "Should not be unsubscribing while active")

        client.conn.disconnect()
    }

    @Test
    fun `isEnded is false while active`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        assertFalse(handle.isEnded, "Should not be ended while active")

        client.conn.disconnect()
    }

    @Test
    fun `isEnded is true after unsubscribeThen completes`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM note")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        val unsubDone = CompletableDeferred<Unit>()
        handle.unsubscribeThen { _ -> unsubDone.complete(Unit) }
        withTimeout(DEFAULT_TIMEOUT_MS) { unsubDone.await() }

        assertTrue(handle.isEnded, "Should be ended after unsubscribe")
        assertEquals(SubscriptionState.ENDED, handle.state)
    }

    @Test
    fun `querySetId is assigned`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        val id = handle.querySetId
        assertTrue(id.id >= 0u, "querySetId should be non-negative: ${id.id}")

        client.conn.disconnect()
    }
}
