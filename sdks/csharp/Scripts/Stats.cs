using System.Diagnostics.CodeAnalysis;
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using UnityEngine;

namespace SpacetimeDB
{
    public class NetworkRequestTracker
    {
        private readonly ConcurrentQueue<(DateTime, TimeSpan, string)> _requestDurations =
            new ConcurrentQueue<(DateTime, TimeSpan, string)>();

        private uint _nextRequestId;
        private Dictionary<uint, (DateTime, string)> _requests = new Dictionary<uint, (DateTime, string)>();

        public uint StartTrackingRequest(string metadata = "")
        {
            // Record the start time of the request
            var newRequestId = ++_nextRequestId;
            _requests[newRequestId] = (DateTime.UtcNow, metadata);
            return newRequestId;
        }

        public bool FinishTrackingRequest(uint requestId)
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
            var endTime = DateTime.UtcNow;
            var duration = endTime - entry.Item1;
            _requestDurations.Enqueue((endTime, duration, entry.Item2));
            return true;
        }

        public void InsertRequest(DateTime timestamp, TimeSpan duration, string metadata)
        {
            _requestDurations.Enqueue((timestamp, duration, metadata));
        }

        public ((TimeSpan, string), (TimeSpan, string)) GetMinMaxTimes(int lastSeconds)
        {
            var cutoff = DateTime.UtcNow.AddSeconds(-lastSeconds);

            if (!_requestDurations.Where(x => x.Item1 >= cutoff).Select(x => (x.Item2, x.Item3)).Any())
            {
                return ((TimeSpan.Zero, ""), (TimeSpan.Zero, ""));
            }

            var min = _requestDurations.Where(x => x.Item1 >= cutoff).Select(x => (x.Item2, x.Item3)).Min();
            var max = _requestDurations.Where(x => x.Item1 >= cutoff).Select(x => (x.Item2, x.Item3)).Max();

            return (min, max);
        }

        public int GetSampleCount() => _requestDurations.Count;
        public int GetRequestsAwaitingResponse() => _requests.Count;
    }


    public class Stats
    {
        public NetworkRequestTracker ReducerRequestTracker = new NetworkRequestTracker();
        public NetworkRequestTracker OneOffRequestTracker = new NetworkRequestTracker();
        public NetworkRequestTracker SubscriptionRequestTracker = new NetworkRequestTracker();
        public NetworkRequestTracker AllReducersTracker = new NetworkRequestTracker();
        public NetworkRequestTracker ParseMessageTracker = new NetworkRequestTracker();
    }
}