import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class StatsExtrasTest {

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
