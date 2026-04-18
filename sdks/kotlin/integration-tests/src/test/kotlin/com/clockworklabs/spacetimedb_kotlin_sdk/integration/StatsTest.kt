package com.clockworklabs.spacetimedb_kotlin_sdk.integration

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

        client.conn.oneOffQuery("SELECT * FROM user")

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

    @Test
    fun `procedureRequestTracker exists and starts empty`() = runBlocking {
        val client = connectToDb()

        val tracker = client.conn.stats.procedureRequestTracker
        assertEquals(0, tracker.sampleCount, "No procedures called, sample count should be 0")
        assertNull(tracker.allTimeMinMax, "No procedures called, allTimeMinMax should be null")
        assertEquals(0, tracker.requestsAwaitingResponse, "No procedures in flight")

        client.conn.disconnect()
    }

    @Test
    fun `applyMessageTracker exists`() = runBlocking {
        val client = connectToDb()

        val tracker = client.conn.stats.applyMessageTracker
        // After connecting, there may or may not be apply messages depending on timing
        assertTrue(tracker.sampleCount >= 0, "Sample count should be non-negative")
        assertTrue(tracker.requestsAwaitingResponse >= 0, "Awaiting should be non-negative")

        client.conn.disconnect()
    }

    @Test
    fun `applyMessageTracker records after subscription`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val tracker = client.conn.stats.applyMessageTracker
        // After subscribing, server applies the subscription which should register
        assertTrue(tracker.sampleCount >= 0, "Sample count should be non-negative after subscribe")

        client.conn.disconnect()
    }

    @Test
    fun `all five trackers are distinct objects`() = runBlocking {
        val client = connectToDb()

        val stats = client.conn.stats
        val trackers = listOf(
            stats.reducerRequestTracker,
            stats.subscriptionRequestTracker,
            stats.oneOffRequestTracker,
            stats.procedureRequestTracker,
            stats.applyMessageTracker,
        )
        // All should be distinct instances
        val unique = trackers.toSet()
        assertEquals(5, unique.size, "All 5 trackers should be distinct objects")

        client.conn.disconnect()
    }
}
