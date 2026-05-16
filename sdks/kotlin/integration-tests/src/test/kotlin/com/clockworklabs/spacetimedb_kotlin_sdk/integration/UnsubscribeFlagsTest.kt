package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SubscriptionState
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class UnsubscribeFlagsTest {

    @Test
    fun `unsubscribeThen transitions to ENDED`() = runBlocking {
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

        assertEquals(SubscriptionState.ENDED, handle.state)

        client.conn.disconnect()
    }

    @Test
    fun `unsubscribeThen callback receives context`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM note")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        val gotContext = CompletableDeferred<Any?>()
        handle.unsubscribeThen { ctx ->
            gotContext.complete(ctx)
        }

        val result = withTimeout(DEFAULT_TIMEOUT_MS) { gotContext.await() }
        assertNotNull(result, "unsubscribeThen callback should receive non-null context")

        client.conn.disconnect()
    }

    @Test
    fun `unsubscribe completes without error`() = runBlocking {
        val client = connectToDb()

        val applied = CompletableDeferred<Unit>()
        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        assertTrue(handle.isActive, "Should be active after applied")

        val unsubDone = CompletableDeferred<Unit>()
        handle.unsubscribeThen { _ -> unsubDone.complete(Unit) }
        withTimeout(DEFAULT_TIMEOUT_MS) { unsubDone.await() }

        // After unsubscribeThen callback fires, the handle should be ENDED
        assertTrue(handle.isEnded, "Should be ended after unsubscribe completes")

        client.conn.disconnect()
    }

    @Test
    fun `multiple subscriptions can be independently unsubscribed`() = runBlocking {
        val client = connectToDb()

        val applied1 = CompletableDeferred<Unit>()
        val handle1 = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied1.complete(Unit) }
            .onError { _, err -> applied1.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        val applied2 = CompletableDeferred<Unit>()
        val handle2 = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied2.complete(Unit) }
            .onError { _, err -> applied2.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM note")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied1.await() }
        withTimeout(DEFAULT_TIMEOUT_MS) { applied2.await() }

        // Unsubscribe only handle1
        val unsub1 = CompletableDeferred<Unit>()
        handle1.unsubscribeThen { _ -> unsub1.complete(Unit) }
        withTimeout(DEFAULT_TIMEOUT_MS) { unsub1.await() }

        assertEquals(SubscriptionState.ENDED, handle1.state, "handle1 should be ENDED")
        assertEquals(SubscriptionState.ACTIVE, handle2.state, "handle2 should still be ACTIVE")

        client.conn.disconnect()
    }

    @Test
    fun `unsubscribe then re-subscribe works`() = runBlocking {
        val client = connectToDb()

        // Subscribe
        val applied1 = CompletableDeferred<Unit>()
        val handle1 = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied1.complete(Unit) }
            .onError { _, err -> applied1.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied1.await() }

        // Unsubscribe
        val unsub = CompletableDeferred<Unit>()
        handle1.unsubscribeThen { _ -> unsub.complete(Unit) }
        withTimeout(DEFAULT_TIMEOUT_MS) { unsub.await() }
        assertTrue(handle1.isEnded)

        // Re-subscribe
        val applied2 = CompletableDeferred<Unit>()
        val handle2 = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied2.complete(Unit) }
            .onError { _, err -> applied2.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied2.await() }
        assertTrue(handle2.isActive, "Re-subscribed handle should be active")
        assertNotEquals(handle1.querySetId, handle2.querySetId, "New subscription should get new querySetId")

        client.conn.disconnect()
    }
}
