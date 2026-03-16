import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class StatsTest {

    @Test
    fun `stats are zero before any operations`() = runBlocking {
        val client = connectToDb()

        assertEquals(0, client.conn.stats.reducerRequestTracker.sampleCount, "reducer samples should be 0 initially")
        assertEquals(0, client.conn.stats.oneOffRequestTracker.sampleCount, "oneOff samples should be 0 initially")
        assertEquals(0, client.conn.stats.reducerRequestTracker.requestsAwaitingResponse, "no in-flight requests initially")
        assertNull(client.conn.stats.reducerRequestTracker.allTimeMinMax, "allTimeMinMax should be null initially")

        client.conn.disconnect()
    }

    @Test
    fun `subscriptionRequestTracker increments after subscribe`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val subSamples = client.conn.stats.subscriptionRequestTracker.sampleCount
        assertTrue(subSamples > 0, "subscriptionRequestTracker should have samples after subscribe, got $subSamples")

        client.conn.disconnect()
    }

    @Test
    fun `reducerRequestTracker increments after reducer call`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val before = client.conn.stats.reducerRequestTracker.sampleCount

        val reducerDone = CompletableDeferred<Unit>()
        client.conn.reducers.onSendMessage { ctx, _ ->
            if (ctx.callerIdentity == client.identity) reducerDone.complete(Unit)
        }
        client.conn.reducers.sendMessage("stats-reducer-${System.nanoTime()}")
        withTimeout(DEFAULT_TIMEOUT_MS) { reducerDone.await() }

        val after = client.conn.stats.reducerRequestTracker.sampleCount
        assertTrue(after > before, "reducerRequestTracker should increment, before=$before after=$after")

        client.cleanup()
    }

    @Test
    fun `oneOffRequestTracker increments after suspend query`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val before = client.conn.stats.oneOffRequestTracker.sampleCount

        // Use suspend variant — no flaky delay needed
        @Suppress("UNUSED_VARIABLE")
        val result = client.conn.oneOffQuery("SELECT * FROM user")

        val after = client.conn.stats.oneOffRequestTracker.sampleCount
        assertTrue(after > before, "oneOffRequestTracker should increment, before=$before after=$after")

        client.conn.disconnect()
    }

    @Test
    fun `allTimeMinMax is set after reducer call`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val reducerDone = CompletableDeferred<Unit>()
        client.conn.reducers.onSendMessage { ctx, _ ->
            if (ctx.callerIdentity == client.identity) reducerDone.complete(Unit)
        }
        client.conn.reducers.sendMessage("stats-minmax-${System.nanoTime()}")
        withTimeout(DEFAULT_TIMEOUT_MS) { reducerDone.await() }

        val minMax = assertNotNull(client.conn.stats.reducerRequestTracker.allTimeMinMax, "allTimeMinMax should be set")
        assertTrue(
            minMax.min.duration >= kotlin.time.Duration.ZERO,
            "min duration should be non-negative"
        )

        client.cleanup()
    }

    @Test
    fun `minMaxTimes returns null when no window has rotated`() = runBlocking {
        val client = connectToDb()

        // On a fresh tracker, no window has rotated yet
        val minMax = client.conn.stats.reducerRequestTracker.minMaxTimes(60)
        assertNull(minMax, "minMaxTimes should return null before any window rotation")

        client.conn.disconnect()
    }
}
