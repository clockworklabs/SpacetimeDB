package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.locks.SynchronizedObject
import kotlinx.atomicfu.locks.synchronized
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeMark
import kotlin.time.TimeSource

public data class DurationSample(val duration: Duration, val metadata: String)

public data class MinMaxResult(val min: DurationSample, val max: DurationSample)

private class RequestEntry(val startTime: TimeMark, val metadata: String)

public class NetworkRequestTracker internal constructor(
    private val timeSource: TimeSource = TimeSource.Monotonic,
) : SynchronizedObject() {
    public constructor() : this(TimeSource.Monotonic)

    public companion object {
        private const val MAX_TRACKERS = 16
    }

    public var allTimeMin: DurationSample? = null
        get() = synchronized(this) { field }
        private set
    public var allTimeMax: DurationSample? = null
        get() = synchronized(this) { field }
        private set

    private val trackers = mutableMapOf<Int, WindowTracker>()
    private var totalSamples = 0
    private var nextRequestId = 0u
    private val requests = mutableMapOf<UInt, RequestEntry>()

    public fun getAllTimeMinMax(): MinMaxResult? = synchronized(this) {
        val min = allTimeMin ?: return null
        val max = allTimeMax ?: return null
        MinMaxResult(min, max)
    }

    public fun getMinMaxTimes(lastSeconds: Int): MinMaxResult? = synchronized(this) {
        val tracker = trackers.getOrPut(lastSeconds) {
            check(trackers.size < MAX_TRACKERS) {
                "Cannot track more than $MAX_TRACKERS distinct window sizes"
            }
            WindowTracker(lastSeconds, timeSource)
        }
        tracker.getMinMax()
    }

    public fun getSampleCount(): Int = synchronized(this) { totalSamples }

    public fun getRequestsAwaitingResponse(): Int = synchronized(this) { requests.size }

    internal fun startTrackingRequest(metadata: String = ""): UInt {
        synchronized(this) {
            val requestId = nextRequestId++
            requests[requestId] = RequestEntry(
                startTime = timeSource.markNow(),
                metadata = metadata,
            )
            return requestId
        }
    }

    internal fun finishTrackingRequest(requestId: UInt, metadata: String? = null): Boolean {
        synchronized(this) {
            val entry = requests.remove(requestId) ?: return false
            val duration = entry.startTime.elapsedNow()
            val resolvedMetadata = metadata ?: entry.metadata
            insertSampleLocked(duration, resolvedMetadata)
            return true
        }
    }

    internal fun insertSample(duration: Duration, metadata: String = "") {
        synchronized(this) {
            insertSampleLocked(duration, metadata)
        }
    }

    private fun insertSampleLocked(duration: Duration, metadata: String) {
        totalSamples++
        val sample = DurationSample(duration, metadata)

        val currentMin = allTimeMin
        if (currentMin == null || duration < currentMin.duration) {
            allTimeMin = sample
        }
        val currentMax = allTimeMax
        if (currentMax == null || duration > currentMax.duration) {
            allTimeMax = sample
        }

        for (tracker in trackers.values) {
            tracker.insertSample(duration, metadata)
        }
    }

    private class WindowTracker(windowSeconds: Int, private val timeSource: TimeSource) {
        val window: Duration = windowSeconds.seconds
        var lastReset: TimeMark = timeSource.markNow()

        var lastWindowMin: DurationSample? = null
            private set
        var lastWindowMax: DurationSample? = null
            private set
        var thisWindowMin: DurationSample? = null
            private set
        var thisWindowMax: DurationSample? = null
            private set

        fun insertSample(duration: Duration, metadata: String) {
            maybeRotate()
            val sample = DurationSample(duration, metadata)

            val currentMin = thisWindowMin
            if (currentMin == null || duration < currentMin.duration) {
                thisWindowMin = sample
            }
            val currentMax = thisWindowMax
            if (currentMax == null || duration > currentMax.duration) {
                thisWindowMax = sample
            }
        }

        fun getMinMax(): MinMaxResult? {
            maybeRotate()
            val min = lastWindowMin ?: return null
            val max = lastWindowMax ?: return null
            return MinMaxResult(min, max)
        }

        private fun maybeRotate() {
            val elapsed = lastReset.elapsedNow()
            if (elapsed >= window) {
                if (elapsed >= window * 2) {
                    // More than one full window passed — no data in the immediately
                    // preceding window, so lastWindow should be empty.
                    lastWindowMin = null
                    lastWindowMax = null
                } else {
                    lastWindowMin = thisWindowMin
                    lastWindowMax = thisWindowMax
                }
                thisWindowMin = null
                thisWindowMax = null
                lastReset = timeSource.markNow()
            }
        }
    }
}

public class Stats {
    public val reducerRequestTracker: NetworkRequestTracker = NetworkRequestTracker()
    public val procedureRequestTracker: NetworkRequestTracker = NetworkRequestTracker()
    public val subscriptionRequestTracker: NetworkRequestTracker = NetworkRequestTracker()
    public val oneOffRequestTracker: NetworkRequestTracker = NetworkRequestTracker()

    public val applyMessageTracker: NetworkRequestTracker = NetworkRequestTracker()
}
