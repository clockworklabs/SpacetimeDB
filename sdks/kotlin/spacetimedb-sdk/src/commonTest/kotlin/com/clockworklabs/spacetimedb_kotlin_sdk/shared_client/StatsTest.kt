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
    fun startAndFinishReturnsTrue() {
        val tracker = NetworkRequestTracker()
        val id = tracker.startTrackingRequest("test")
        assertTrue(tracker.finishTrackingRequest(id))
    }

    @Test
    fun finishUnknownIdReturnsFalse() {
        val tracker = NetworkRequestTracker()
        assertFalse(tracker.finishTrackingRequest(999u))
    }

    @Test
    fun sampleCountIncrementsAfterFinish() {
        val tracker = NetworkRequestTracker()
        assertEquals(0, tracker.getSampleCount())

        val id = tracker.startTrackingRequest()
        tracker.finishTrackingRequest(id)

        assertEquals(1, tracker.getSampleCount())
    }

    @Test
    fun requestsAwaitingResponseTracksActiveRequests() {
        val tracker = NetworkRequestTracker()
        assertEquals(0, tracker.getRequestsAwaitingResponse())

        val id1 = tracker.startTrackingRequest()
        val id2 = tracker.startTrackingRequest()
        assertEquals(2, tracker.getRequestsAwaitingResponse())

        tracker.finishTrackingRequest(id1)
        assertEquals(1, tracker.getRequestsAwaitingResponse())

        tracker.finishTrackingRequest(id2)
        assertEquals(0, tracker.getRequestsAwaitingResponse())
    }

    // ---- All-time min/max ----

    @Test
    fun allTimeMinMaxTracksExtremes() {
        val tracker = NetworkRequestTracker()
        assertNull(tracker.allTimeMin)
        assertNull(tracker.allTimeMax)

        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")
        tracker.insertSample(200.milliseconds, "medium")

        val min = assertNotNull(tracker.allTimeMin)
        assertEquals(100.milliseconds, min.duration)
        assertEquals("fast", min.metadata)

        val max = assertNotNull(tracker.allTimeMax)
        assertEquals(500.milliseconds, max.duration)
        assertEquals("slow", max.metadata)
    }

    @Test
    fun getAllTimeMinMaxReturnsNullWhenEmpty() {
        val tracker = NetworkRequestTracker()
        assertNull(tracker.getAllTimeMinMax())
    }

    @Test
    fun getAllTimeMinMaxReturnsConsistentPair() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")

        val result = assertNotNull(tracker.getAllTimeMinMax())
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("fast", result.min.metadata)
        assertEquals(500.milliseconds, result.max.duration)
        assertEquals("slow", result.max.metadata)
    }

    @Test
    fun getAllTimeMinMaxWithSingleSampleReturnsSameForBoth() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(250.milliseconds, "only")

        val result = assertNotNull(tracker.getAllTimeMinMax())
        assertEquals(250.milliseconds, result.min.duration)
        assertEquals(250.milliseconds, result.max.duration)
    }

    // ---- Insert sample ----

    @Test
    fun insertSampleIncrementsSampleCount() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(50.milliseconds)
        tracker.insertSample(100.milliseconds)
        assertEquals(2, tracker.getSampleCount())
    }

    // ---- Metadata passthrough ----

    @Test
    fun metadataPassesThroughToSample() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(10.milliseconds, "reducer:AddPlayer")
        assertEquals("reducer:AddPlayer", tracker.allTimeMin?.metadata)
    }

    @Test
    fun finishTrackingWithOverrideMetadata() {
        val tracker = NetworkRequestTracker()
        val id = tracker.startTrackingRequest("original")
        tracker.finishTrackingRequest(id, "override")
        assertEquals("override", tracker.allTimeMin?.metadata)
    }

    // ---- Windowed min/max ----

    @Test
    fun getMinMaxTimesReturnsNullBeforeWindowElapses() {
        val tracker = NetworkRequestTracker()
        tracker.insertSample(100.milliseconds)
        // The first window hasn't completed yet, so lastWindow is null
        assertNull(tracker.getMinMaxTimes(10))
    }

    @Test
    fun multipleWindowSizesWorkIndependently() {
        val tracker = NetworkRequestTracker()
        // Just verify we can request multiple window sizes without error
        tracker.insertSample(100.milliseconds)
        tracker.getMinMaxTimes(5)
        tracker.getMinMaxTimes(10)
        tracker.getMinMaxTimes(30)
        // All return null initially (no completed window)
        assertNull(tracker.getMinMaxTimes(5))
        assertNull(tracker.getMinMaxTimes(10))
        assertNull(tracker.getMinMaxTimes(30))
    }

    @Test
    fun windowRotationReturnsMinMaxAfterWindowElapses() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // Register a 1-second window tracker
        assertNull(tracker.getMinMaxTimes(1))

        // Insert samples in the first window
        tracker.insertSample(100.milliseconds, "fast")
        tracker.insertSample(500.milliseconds, "slow")
        tracker.insertSample(250.milliseconds, "mid")

        // Still within the first window — lastWindow has no data yet
        assertNull(tracker.getMinMaxTimes(1))

        // Advance past the 1-second window boundary
        ts += 1.seconds

        // Now the previous window's data should be available
        val result = assertNotNull(tracker.getMinMaxTimes(1))
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("fast", result.min.metadata)
        assertEquals(500.milliseconds, result.max.duration)
        assertEquals("slow", result.max.metadata)
    }

    @Test
    fun windowRotationReplacesWithNewWindowData() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // First window: samples 100ms and 500ms
        tracker.getMinMaxTimes(1) // create tracker
        tracker.insertSample(100.milliseconds, "w1-fast")
        tracker.insertSample(500.milliseconds, "w1-slow")

        // Advance to second window
        ts += 1.seconds

        // Insert new samples in the second window
        tracker.insertSample(200.milliseconds, "w2-fast")
        tracker.insertSample(300.milliseconds, "w2-slow")

        // getMinMax should return first window's data (100ms, 500ms)
        val result1 = assertNotNull(tracker.getMinMaxTimes(1))
        assertEquals(100.milliseconds, result1.min.duration)
        assertEquals(500.milliseconds, result1.max.duration)

        // Advance to third window — now second window becomes lastWindow
        ts += 1.seconds

        val result2 = assertNotNull(tracker.getMinMaxTimes(1))
        assertEquals(200.milliseconds, result2.min.duration)
        assertEquals("w2-fast", result2.min.metadata)
        assertEquals(300.milliseconds, result2.max.duration)
        assertEquals("w2-slow", result2.max.metadata)
    }

    @Test
    fun windowRotationReturnsNullAfterTwoWindowsWithNoData() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // Insert samples in the first window
        tracker.getMinMaxTimes(1)
        tracker.insertSample(100.milliseconds, "data")

        // Advance past one window — data visible
        ts += 1.seconds
        assertNotNull(tracker.getMinMaxTimes(1))

        // Advance past two full windows with no new data —
        // the immediately preceding window is empty
        ts += 2.seconds
        assertNull(tracker.getMinMaxTimes(1))
    }

    @Test
    fun windowRotationEmptyWindowPreservesNullMinMax() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)

        // First window: insert data
        tracker.getMinMaxTimes(1)
        tracker.insertSample(100.milliseconds)

        // Advance to second window, insert nothing
        ts += 1.seconds

        // First window data is available
        assertNotNull(tracker.getMinMaxTimes(1))

        // Advance to third window — second window had no data
        ts += 1.seconds

        // lastWindow should be null since second window was empty
        assertNull(tracker.getMinMaxTimes(1))
    }

    @Test
    fun windowMinMaxTracksExtremesWithinWindow() {
        val ts = TestTimeSource()
        val tracker = NetworkRequestTracker(ts)
        tracker.getMinMaxTimes(1)

        // Insert samples that get progressively larger and smaller
        tracker.insertSample(300.milliseconds, "mid")
        tracker.insertSample(100.milliseconds, "smallest")
        tracker.insertSample(900.milliseconds, "largest")
        tracker.insertSample(200.milliseconds, "small")

        ts += 1.seconds

        val result = assertNotNull(tracker.getMinMaxTimes(1))
        assertEquals(100.milliseconds, result.min.duration)
        assertEquals("smallest", result.min.metadata)
        assertEquals(900.milliseconds, result.max.duration)
        assertEquals("largest", result.max.metadata)
    }

    @Test
    fun maxTrackersLimitEnforced() {
        val tracker = NetworkRequestTracker()
        // Register 16 distinct window sizes (the max)
        for (i in 1..16) {
            tracker.getMinMaxTimes(i)
        }
        // 17th should throw
        assertFailsWith<IllegalStateException> {
            tracker.getMinMaxTimes(17)
        }
    }

    // ---- Stats aggregator ----

    @Test
    fun statsHasAllTrackers() {
        val stats = Stats()
        // Just verify the trackers are distinct instances
        assertNotNull(stats.reducerRequestTracker)
        assertNotNull(stats.procedureRequestTracker)
        assertNotNull(stats.subscriptionRequestTracker)
        assertNotNull(stats.oneOffRequestTracker)
        assertNotNull(stats.applyMessageTracker)
    }
}
