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
