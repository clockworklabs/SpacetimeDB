import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SubscriptionState
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SubscriptionHandleExtrasTest {

    @Test
    fun `queries contains the subscribed query`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        val handle = client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
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
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
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
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
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
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
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
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
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
            .onError { _, err -> applied.completeExceptionally(RuntimeException(err)) }
            .subscribe("SELECT * FROM user")

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        // querySetId should be a valid assigned value
        val id = handle.querySetId
        assertTrue(id.id >= 0u, "querySetId should be non-negative: ${id.id}")

        client.conn.disconnect()
    }
}
