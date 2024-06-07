using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;

namespace SpacetimeDB
{
    public class NetworkRequestTracker
    {
        private readonly ConcurrentQueue<(DateTime End, TimeSpan Duration, string Metadata)> _requestDurations = new();

        private uint _nextRequestId;
        private readonly Dictionary<uint, (DateTime Start, string Metadata)> _requests = new();

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
            InsertRequest(entry.Start, entry.Metadata);
            return true;
        }

        public void InsertRequest(TimeSpan duration, string metadata)
        {
            _requestDurations.Enqueue((DateTime.UtcNow, duration, metadata));
        }

        public void InsertRequest(DateTime start, string metadata)
        {
            InsertRequest(DateTime.UtcNow - start, metadata);
        }

        public ((TimeSpan Duration, string Metadata) Min, (TimeSpan Duration, string Metadata) Max)? GetMinMaxTimes(int lastSeconds)
        {
            var cutoff = DateTime.UtcNow.AddSeconds(-lastSeconds);
            var requestDurations = _requestDurations.Where(x => x.End >= cutoff).Select(x => (x.Duration, x.Metadata));

            if (!requestDurations.Any())
            {
                return null;
            }

            return (requestDurations.Min(), requestDurations.Max());
        }

        public int GetSampleCount() => _requestDurations.Count;
        public int GetRequestsAwaitingResponse() => _requests.Count;
    }

    public class Stats
    {
        public NetworkRequestTracker ReducerRequestTracker = new();
        public NetworkRequestTracker OneOffRequestTracker = new();
        public NetworkRequestTracker SubscriptionRequestTracker = new();
        public NetworkRequestTracker AllReducersTracker = new();
        public NetworkRequestTracker ParseMessageTracker = new();
    }
}
