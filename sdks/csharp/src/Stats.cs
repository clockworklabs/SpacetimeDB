using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;

namespace SpacetimeDB
{
    public class NetworkRequestTracker
    {
        private readonly ConcurrentQueue<(DateTime End, (TimeSpan Duration, string Metadata) Request)> _requestDurations = new();

        private uint _nextRequestId;
        private readonly Dictionary<uint, (DateTime Start, string Metadata)> _requests = new();

        // Limit the number of request durations we store to prevent memory leaks.
        public int KeepLastSeconds = 5 * 60;

        internal uint StartTrackingRequest(string metadata = "")
        {
            // Record the start time of the request
            var newRequestId = ++_nextRequestId;
            _requests[newRequestId] = (DateTime.UtcNow, metadata);
            return newRequestId;
        }

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

        private IEnumerable<(TimeSpan Duration, string Metadata)> GetRequestDurations(int lastSeconds)
        {
            var cutoff = DateTime.UtcNow.AddSeconds(-lastSeconds);
            return _requestDurations.SkipWhile(x => x.End < cutoff).Select(x => x.Request);
        }

        internal void InsertRequest(TimeSpan duration, string metadata)
        {
            lock (_requestDurations)
            {
                // Remove expired entries, we need to do this atomically.
                var cutoff = DateTime.UtcNow.AddSeconds(-KeepLastSeconds);
                var removeCount = _requestDurations.TakeWhile(x => x.End < cutoff).Count();
                for (var i = 0; i < removeCount; i++)
                {
                    _requestDurations.TryDequeue(out _);
                }
                _requestDurations.Enqueue((DateTime.UtcNow, (duration, metadata)));
            }
        }

        internal void InsertRequest(DateTime start, string metadata)
        {
            InsertRequest(DateTime.UtcNow - start, metadata);
        }

        public ((TimeSpan Duration, string Metadata) Min, (TimeSpan Duration, string Metadata) Max)? GetMinMaxTimes(int lastSeconds)
        {
            if (lastSeconds > KeepLastSeconds)
            {
                throw new ArgumentException($"lastSeconds must be less than or equal to KeepLastSeconds = {KeepLastSeconds}", nameof(lastSeconds));
            }

            var cutoff = DateTime.UtcNow.AddSeconds(-lastSeconds);
            var requestDurations = _requestDurations.SkipWhile(x => x.End < cutoff).Select(x => x.Request);

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
        public readonly NetworkRequestTracker ReducerRequestTracker = new();
        public readonly NetworkRequestTracker OneOffRequestTracker = new();
        public readonly NetworkRequestTracker SubscriptionRequestTracker = new();
        public readonly NetworkRequestTracker AllReducersTracker = new();
        public readonly NetworkRequestTracker ParseMessageTracker = new();

        public void KeepLastSeconds(int seconds)
        {
            ReducerRequestTracker.KeepLastSeconds = seconds;
            OneOffRequestTracker.KeepLastSeconds = seconds;
            SubscriptionRequestTracker.KeepLastSeconds = seconds;
            AllReducersTracker.KeepLastSeconds = seconds;
            ParseMessageTracker.KeepLastSeconds = seconds;
        }
    }
}
