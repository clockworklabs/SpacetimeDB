package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.locks.SynchronizedObject
import kotlinx.atomicfu.locks.synchronized
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeMark
import kotlin.time.TimeSource

/** A single latency sample with its associated metadata (e.g. reducer name). */
public data class DurationSample(val duration: Duration, val metadata: String)

/** Min/max pair from a [NetworkRequestTracker] query. */
public data class MinMaxResult(val min: DurationSample, val max: DurationSample)

private class RequestEntry(val startTime: TimeMark, val metadata: String)

/**
 * Tracks request latencies over sliding time windows.
 * Thread-safe — all reads and writes are synchronized.
 *
 * Use [minMaxTimes] to query min/max latency within a recent window,
 * or [allTimeMinMax] for the lifetime extremes.
 */
public class NetworkRequestTracker internal constructor(
    private val timeSource: TimeSource = TimeSource.Monotonic,
) : SynchronizedObject() {
    internal constructor() : this(TimeSource.Monotonic)

    public companion object {
        private const val MAX_TRACKERS = 16
    }

    private var allTimeMin: DurationSample? = null
    private var allTimeMax: DurationSample? = null

    private val trackers = mutableMapOf<Int, WindowTracker>()
    private var totalSamples = 0
    private var nextRequestId = 0u
    private val requests = mutableMapOf<UInt, RequestEntry>()

    /** All-time min/max latency, or `null` if no samples recorded yet. */
    public val allTimeMinMax: MinMaxResult?
        get() = synchronized(this) {
            val min = allTimeMin ?: return null
            val max = allTimeMax ?: return null
            MinMaxResult(min, max)
        }

    /** Min/max latency within the last [lastSeconds] seconds, or `null` if no samples in that window. */
    public fun minMaxTimes(lastSeconds: Int): MinMaxResult? = synchronized(this) {
        val tracker = trackers.getOrPut(lastSeconds) {
            check(trackers.size < MAX_TRACKERS) {
                "Cannot track more than $MAX_TRACKERS distinct window sizes"
            }
            WindowTracker(lastSeconds, timeSource)
        }
        tracker.getMinMax()
    }

    /** Total number of latency samples recorded. */
    public val sampleCount: Int get() = synchronized(this) { totalSamples }

    /** Number of requests that have been started but not yet completed. */
    public val requestsAwaitingResponse: Int get() = synchronized(this) { requests.size }

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

/** Aggregated latency trackers for each category of SpacetimeDB operation. */
public class Stats {
    /** Tracks round-trip latency for reducer calls. */
    public val reducerRequestTracker: NetworkRequestTracker = NetworkRequestTracker()

    /** Tracks round-trip latency for procedure calls. */
    public val procedureRequestTracker: NetworkRequestTracker = NetworkRequestTracker()

    /** Tracks round-trip latency for subscription requests. */
    public val subscriptionRequestTracker: NetworkRequestTracker = NetworkRequestTracker()

    /** Tracks round-trip latency for one-off query requests. */
    public val oneOffRequestTracker: NetworkRequestTracker = NetworkRequestTracker()

    /** Tracks time spent applying incoming server messages to the client cache. */
    public val applyMessageTracker: NetworkRequestTracker = NetworkRequestTracker()
}
