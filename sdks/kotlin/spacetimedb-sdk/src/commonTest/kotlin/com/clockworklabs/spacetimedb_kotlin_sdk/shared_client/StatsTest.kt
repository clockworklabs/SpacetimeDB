package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TestTimeSource

class StatsTest {

    // ---- Start / finish tracking ----

    @Test
    fun `start and finish returns true`() {
        val tracker = NetworkRequestTracker()
        val id = tracker.startTrackingRequest("test")
        assertTrue(tracker.finishTrackingRequest(id))
    }

    @Test
    fun `finish unknown id returns false`() {
        val tracker = NetworkRequestTracker()
        assertFalse(tracker.finishTrackingRequest(999u))
    }

    @Test
    fun `sample count increments after finish`() {
        val tracker = NetworkRequestTracker()
        assertEquals(0, tracker.sampleCount)

        val id = tracker.startTrackingRequest()
        tracker.finishTrackingRequest(id)

        assertEquals(1, tracker.sampleCount)
    }

    @Test
    fun `requests awaiting response tracks active requests`() {
        val tracker = NetworkRequestTracker()
        assertEquals(0, tracker.requestsAwaitingResponse)

        val id1 = tracker.startTrackingRequest()
        val id2 = tracker.startTrackingRequest()
        assertEquals(2, tracker.requestsAwaitingResponse)

        tracker.finishTrackingRequest(id1)
        assertEquals(1, tracker.requestsAwaitingResponse)

        tracker.finishTrackingRequest(id2)
        assertEquals(0, tracker.requestsAwaitingResponse)
    }

    // ---- All-time min/max ----

    @Test
    fun `all time min max tracks extremes`() {
        val tracker = NetworkRequestTracker()
        assertNull(tracker.allTimeMinMax)

        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")
        tracker.insertSample(200.milliseconds, "medium")

        val result = assertNotNull(tracker.allTimeMinMax)
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("fast", result.min.metadata)
        assertEquals(500.milliseconds, result.max.duration)
        assertEquals("slow", result.max.metadata)
    }

    @Test
    fun `get all time min max returns null when empty`() {
        val tracker = NetworkRequestTracker()
        assertNull(tracker.allTimeMinMax)
    }

    @Test
    fun `get all time min max returns consistent pair`() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")

        val result = assertNotNull(tracker.allTimeMinMax)
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("fast", result.min.metadata)
        assertEquals(500.milliseconds, result.max.duration)
        assertEquals("slow", result.max.metadata)
    }

    @Test
    fun `get all time min max with single sample returns same for both`() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(250.milliseconds, "only")

        val result = assertNotNull(tracker.allTimeMinMax)
        assertEquals(250.milliseconds, result.min.duration)
        assertEquals(250.milliseconds, result.max.duration)
    }

    // ---- Insert sample ----

    @Test
    fun `insert sample increments sample count`() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(50.milliseconds)
        tracker.insertSample(100.milliseconds)
        assertEquals(2, tracker.sampleCount)
    }

    // ---- Metadata passthrough ----

    @Test
    fun `metadata passes through to sample`() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(10.milliseconds, "reducer:AddPlayer")
        assertEquals("reducer:AddPlayer", tracker.allTimeMinMax?.min?.metadata)
    }

    @Test
    fun `finish tracking with override metadata`() {
        val tracker = NetworkRequestTracker()
        val id = tracker.startTrackingRequest("original")
        tracker.finishTrackingRequest(id, "override")
        assertEquals("override", tracker.allTimeMinMax?.min?.metadata)
    }

    // ---- Windowed min/max ----

    @Test
    fun `get min max times returns null before window elapses`() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(100.milliseconds)
        // The first window hasn't completed yet, so lastWindow is null
        assertNull(tracker.minMaxTimes(10))
    }

    @Test
    fun `multiple window sizes work independently`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // Register two window sizes
        assertNull(tracker.minMaxTimes(1))  // 1-second window
        assertNull(tracker.minMaxTimes(3))  // 3-second window

        // Window 1 (0s–1s): insert 100ms and 200ms
        tracker.insertSample(100.milliseconds)
        tracker.insertSample(200.milliseconds)
        ts += 1.seconds

        // 1s window should have data; 3s window still pending
        val w1 = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(100.milliseconds, w1.min.duration)
        assertEquals(200.milliseconds, w1.max.duration)
        assertNull(tracker.minMaxTimes(3))

        // Window 2 (1s–2s): insert 500ms only
        tracker.insertSample(500.milliseconds)
        ts += 1.seconds

        // 1s window rotated to new data; 3s window still pending
        val w2 = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(500.milliseconds, w2.min.duration)
        assertNull(tracker.minMaxTimes(3))

        // Advance to 3s — now the 3s window should have data from all samples
        ts += 1.seconds
        val w3 = assertNotNull(tracker.minMaxTimes(3))
        assertEquals(100.milliseconds, w3.min.duration)
        assertEquals(500.milliseconds, w3.max.duration)
    }

    @Test
    fun `window rotation returns min max after window elapses`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // Register a 1-second window tracker
        assertNull(tracker.minMaxTimes(1))

        // Insert samples in the first window
        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")
        tracker.insertSample(250.milliseconds, "mid")

        // Still within the first window — lastWindow has no data yet
        assertNull(tracker.minMaxTimes(1))

        // Advance past the 1-second window boundary
        ts += 1.seconds

        // Now the previous window's data should be available
        val result = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("fast", result.min.metadata)
        assertEquals(500.milliseconds, result.max.duration)
        assertEquals("slow", result.max.metadata)
    }

    @Test
    fun `window rotation replaces with new window data`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // First window: samples 100ms and 500ms
        tracker.minMaxTimes(1) // create tracker
        tracker.insertSample(100.milliseconds, "w1-fast")
        tracker.insertSample(500.milliseconds, "w1-slow")

        // Advance to second window
        ts += 1.seconds

        // Insert new samples in the second window
        tracker.insertSample(200.milliseconds, "w2-fast")
        tracker.insertSample(300.milliseconds, "w2-slow")

        // getMinMax should return first window's data (100ms, 500ms)
        val result1 = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(100.milliseconds, result1.min.duration)
        assertEquals(500.milliseconds, result1.max.duration)

        // Advance to third window — now second window becomes lastWindow
        ts += 1.seconds

        val result2 = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(200.milliseconds, result2.min.duration)
        assertEquals("w2-fast", result2.min.metadata)
        assertEquals(300.milliseconds, result2.max.duration)
        assertEquals("w2-slow", result2.max.metadata)
    }

    @Test
    fun `window rotation returns null after two windows with no data`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // Insert samples in the first window
        tracker.minMaxTimes(1)
        tracker.insertSample(100.milliseconds, "data")

        // Advance past one window — data visible
        ts += 1.seconds
        assertNotNull(tracker.minMaxTimes(1))

        // Advance past two full windows with no new data —
        // the immediately preceding window is empty
        ts += 2.seconds
        assertNull(tracker.minMaxTimes(1))
    }

    @Test
    fun `window rotation empty window preserves null min max`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // First window: insert data
        tracker.minMaxTimes(1)
        tracker.insertSample(100.milliseconds)

        // Advance to second window, insert nothing
        ts += 1.seconds

        // First window data is available
        assertNotNull(tracker.minMaxTimes(1))

        // Advance to third window — second window had no data
        ts += 1.seconds

        // lastWindow should be null since second window was empty
        assertNull(tracker.minMaxTimes(1))
    }

    @Test
    fun `window min max tracks extremes within window`() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)
        tracker.minMaxTimes(1)

        // Insert samples that get progressively larger and smaller
        tracker.insertSample(300.milliseconds, "mid")
        tracker.insertSample(100.milliseconds, "smallest")
        tracker.insertSample(900.milliseconds, "largest")
        tracker.insertSample(200.milliseconds, "small")

        ts += 1.seconds

        val result = assertNotNull(tracker.minMaxTimes(1))
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("smallest", result.min.metadata)
        assertEquals(900.milliseconds, result.max.duration)
        assertEquals("largest", result.max.metadata)
    }

    @Test
    fun `max trackers limit enforced`() {
        val tracker = NetworkRequestTracker()
        // Register 16 distinct window sizes (the max)
        for (i in 1..16) {
            tracker.minMaxTimes(i)
        }
        // 17th should throw
        assertFailsWith<IllegalStateException> {
            tracker.minMaxTimes(17)
        }
    }

    // ---- Stats aggregator ----

    @Test
    fun `stats has all trackers`() {
        val stats = Stats()
        // Just verify the trackers are distinct instances
        assertNotNull(stats.reducerRequestTracker)
        assertNotNull(stats.procedureRequestTracker)
        assertNotNull(stats.subscriptionRequestTracker)
        assertNotNull(stats.oneOffRequestTracker)
        assertNotNull(stats.applyMessageTracker)
    }
}
