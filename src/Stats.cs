using System;
using System.Collections.Generic;
using System.Linq;

namespace SpacetimeDB
{
    /// <summary>
    /// Class to track information about network requests and other internal statistics.
    /// </summary>
    public class NetworkRequestTracker
    {
        public NetworkRequestTracker()
        {
        }

        /// <summary>
        /// The fastest request OF ALL TIME.
        /// We keep data for less time than we used to -- having this around catches outliers that may be problematic.
        /// </summary>
        public (TimeSpan Duration, string Metadata)? AllTimeMin
        {
            get; private set;
        }

        /// <summary>
        /// The slowest request OF ALL TIME.
        /// We keep data for less time than we used to -- having this around catches outliers that may be problematic.
        /// </summary>
        public (TimeSpan Duration, string Metadata)? AllTimeMax
        {
            get; private set;
        }

        private int _totalSamples = 0;

        /// <summary>
        /// The maximum number of windows we are willing to track data in.
        /// </summary>
        public static readonly int MAX_TRACKERS = 16;

        /// <summary>
        /// A tracker that tracks the minimum and maximum sample in a time window,
        /// resetting after <c>windowSeconds</c> seconds.
        /// </summary>
        private struct Tracker
        {
            public Tracker(int windowSeconds)
            {
                LastReset = DateTime.UtcNow;
                Window = new TimeSpan(0, 0, windowSeconds);
                LastWindowMin = null;
                LastWindowMax = null;
                ThisWindowMin = null;
                ThisWindowMax = null;
            }

            private DateTime LastReset;
            private TimeSpan Window;

            // The min and max for the previous window.
            private (TimeSpan Duration, string Metadata)? LastWindowMin;
            private (TimeSpan Duration, string Metadata)? LastWindowMax;

            // The min and max for the current window.
            private (TimeSpan Duration, string Metadata)? ThisWindowMin;
            private (TimeSpan Duration, string Metadata)? ThisWindowMax;

            public void InsertRequest(TimeSpan duration, string metadata)
            {
                var sample = (duration, metadata);

                if (ThisWindowMin == null || ThisWindowMin.Value.Duration > duration)
                {
                    ThisWindowMin = sample;
                }
                if (ThisWindowMax == null || ThisWindowMax.Value.Duration < duration)
                {
                    ThisWindowMax = sample;
                }

                if (LastReset < DateTime.UtcNow - Window)
                {
                    LastReset = DateTime.UtcNow;
                    LastWindowMax = ThisWindowMax;
                    LastWindowMin = ThisWindowMin;
                    ThisWindowMax = null;
                    ThisWindowMin = null;
                }
            }

            public ((TimeSpan Duration, string Metadata) Min, (TimeSpan Duration, string Metadata) Max)? GetMinMaxTimes()
            {
                if (LastWindowMin != null && LastWindowMax != null)
                {
                    return (LastWindowMin.Value, LastWindowMax.Value);
                }

                return null;
            }
        }

        /// <summary>
        /// Maps (requested window time in seconds) -> (the tracker for that time window).
        /// </summary>
        private readonly Dictionary<int, Tracker> Trackers = new();

        /// <summary>
        /// To allow modifying Trackers in a loop.
        /// This is needed because we made Tracker a struct.
        /// </summary>
        private readonly HashSet<int> TrackerWindows = new();

        /// <summary>
        /// ID for the next in-flight request.
        /// </summary>
        private uint _nextRequestId;

        /// <summary>
        /// In-flight requests that have not yet finished running.
        /// </summary>
        private readonly Dictionary<uint, (DateTime Start, string Metadata)> _requests = new();

        internal uint StartTrackingRequest(string metadata = "")
        {
            // This method is called when the user submits a new request.
            // It's possible the user was naughty and did this off the main thread.
            // So, be a little paranoid and lock ourselves. Uncontended this will be pretty fast.
            lock (this)
            {
                // Get a new request ID.
                // Note: C# wraps by default, rather than throwing exception on overflow.
                // So, this class should work forever.
                var newRequestId = ++_nextRequestId;
                // Record the start time of the request.
                _requests[newRequestId] = (DateTime.UtcNow, metadata);
                return newRequestId;
            }
        }

        // The remaining methods in this class do not need to lock, since they are only called from OnProcessMessageComplete.

        internal bool FinishTrackingRequest(uint requestId)
        {
            if (!_requests.Remove(requestId, out var entry))
            {
                // TODO: When we implement requestId json support for SpacetimeDB this shouldn't happen anymore!
                // var minKey = _requests.Keys.Min();
                // entry = _requests[minKey];
                //
                // if (!_requests.Remove(minKey))
                // {
                //     return false;
                // }
                return false;
            }

            // Calculate the duration and add it to the queue
            InsertRequest(entry.Start, entry.Metadata);
            return true;
        }

        internal void InsertRequest(TimeSpan duration, string metadata)
        {
            var sample = (duration, metadata);

            if (AllTimeMin == null || AllTimeMin.Value.Duration > duration)
            {
                AllTimeMin = sample;
            }
            if (AllTimeMax == null || AllTimeMax.Value.Duration < duration)
            {
                AllTimeMax = sample;
            }
            _totalSamples += 1;

            foreach (var window in TrackerWindows)
            {
                var tracker = Trackers[window];
                tracker.InsertRequest(duration, metadata);
                Trackers[window] = tracker; // Needed because struct.
            }
        }

        internal void InsertRequest(DateTime start, string metadata)
        {
            InsertRequest(DateTime.UtcNow - start, metadata);
        }

        /// <summary>
        /// Get the the minimum- and maximum-duration events in lastSeconds.
        /// When first called, this will return null until `lastSeconds` have passed.
        /// After this, the value will update every `lastSeconds`.
        /// 
        /// This class allocates an internal data structure for every distinct value of `lastSeconds` passed.
        /// After `NetworkRequestTracker.MAX_TRACKERS` distinct values have been passed, it will stop allocating internal data structures
        /// and always return null.
        /// This should be fine as long as you don't request a large number of distinct windows.
        /// </summary>
        /// <param name="_deprecated">Present for backwards-compatibility, does nothing.</param>
        public ((TimeSpan Duration, string Metadata) Min, (TimeSpan Duration, string Metadata) Max)? GetMinMaxTimes(int lastSeconds = 0)
        {
            if (lastSeconds <= 0) return null;

            if (Trackers.TryGetValue(lastSeconds, out var tracker))
            {
                return tracker.GetMinMaxTimes();
            }
            else if (TrackerWindows.Count < MAX_TRACKERS)
            {
                TrackerWindows.Add(lastSeconds);
                Trackers.Add(lastSeconds, new Tracker(lastSeconds));
            }

            return null;
        }

        /// <summary>
        /// Get the number of samples in the window.
        /// </summary>
        /// <returns></returns>
        public int GetSampleCount() => _totalSamples;

        /// <summary>
        /// Get the number of outstanding tracked requests.
        /// </summary>
        /// <returns></returns>
        public int GetRequestsAwaitingResponse() => _requests.Count;
    }

    public class Stats
    {
        public readonly NetworkRequestTracker ReducerRequestTracker = new();
        public readonly NetworkRequestTracker OneOffRequestTracker = new();
        public readonly NetworkRequestTracker SubscriptionRequestTracker = new();
        public readonly NetworkRequestTracker AllReducersTracker = new();
        public readonly NetworkRequestTracker ParseMessageTracker = new();
    }
}
