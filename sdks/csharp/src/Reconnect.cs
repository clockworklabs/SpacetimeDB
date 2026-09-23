using System;
using System.IO;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;

namespace SpacetimeDB
{
    public readonly struct NextReconnect
    {
        public int Attempt { get; }
        public TimeSpan Delay { get; }

        internal NextReconnect(int attempt, TimeSpan delay)
        {
            Attempt = attempt;
            Delay = delay;
        }
    }

    /// <summary>Options for <c>WithAutomaticReconnect</c>.</summary>
    public sealed class AutomaticReconnectOptions
    {
        /// <summary>
        /// The delay before the first reconnect attempt, and the floor for every later one.
        /// Defaults to 1 second; values below 500 ms are raised to 500 ms.
        /// </summary>
        public TimeSpan? MinDelay { get; set; }
        /// <summary>
        /// The cap on the delay between reconnect attempts. Defaults to 30 seconds;
        /// values below 1 second (or below <see cref="MinDelay"/>) are raised to that bound.
        /// </summary>
        public TimeSpan? MaxDelay { get; set; }
    }

    public class UnknownResultException : SpacetimeDBException
    {
        public UnknownResultException() : base("Connection lost before the result was received. The operation may have executed.") { }
    }

    internal class ConnectionProtocolException : SpacetimeDBException
    {
        internal ConnectionProtocolException(string message, Exception? inner = null) : base(message, inner) { }
    }

    internal static class ReconnectPolicy
    {
        internal static readonly TimeSpan DefaultMinDelay = TimeSpan.FromSeconds(1);
        internal static readonly TimeSpan DefaultMaxDelay = TimeSpan.FromSeconds(30);
        internal static readonly TimeSpan MinDelayFloor = TimeSpan.FromMilliseconds(500);
        internal static readonly TimeSpan MaxDelayFloor = TimeSpan.FromSeconds(1);

        /// <summary>Fill in defaults and enforce the floors, warning when a value is raised.</summary>
        internal static (TimeSpan Min, TimeSpan Max) Resolve(AutomaticReconnectOptions? options)
        {
            var min = options?.MinDelay ?? DefaultMinDelay;
            var max = options?.MaxDelay ?? DefaultMaxDelay;
            // Floors protect the database from clients that retry too aggressively.
            if (min < MinDelayFloor)
            {
                Log.Warn($"Reconnect MinDelay {min.TotalMilliseconds} ms is below the {MinDelayFloor.TotalMilliseconds} ms floor; using {MinDelayFloor.TotalMilliseconds} ms.");
                min = MinDelayFloor;
            }
            var maxFloor = min > MaxDelayFloor ? min : MaxDelayFloor;
            if (max < maxFloor)
            {
                Log.Warn($"Reconnect MaxDelay {max.TotalMilliseconds} ms is below the {maxFloor.TotalMilliseconds} ms floor; using {maxFloor.TotalMilliseconds} ms.");
                max = maxFloor;
            }
            return (min, max);
        }

        /// <summary>Exponential backoff with jitter, clamped to the bounds; attempt numbers start at one.</summary>
        internal static TimeSpan Delay(int attempt, double random, TimeSpan minDelay, TimeSpan maxDelay)
        {
            var baseMs = Math.Min(maxDelay.TotalMilliseconds, minDelay.TotalMilliseconds * Math.Pow(2, Math.Min(30, Math.Max(0, attempt - 1))));
            var jittered = baseMs * (0.5 + random);
            return TimeSpan.FromMilliseconds(Math.Max(minDelay.TotalMilliseconds, Math.Min(maxDelay.TotalMilliseconds, jittered)));
        }

        [DataContract]
#if UNITY_5_3_OR_NEWER
        [UnityEngine.Scripting.Preserve]
#endif
        private class Claims
        {
            [DataMember(Name = "exp")]
            public double? Exp { get; set; }
            [DataMember(Name = "iat")]
            public double? Iat { get; set; }
        }

        internal static bool TokenNeedsRefresh(string? token, DateTimeOffset now)
        {
            try
            {
                var payload = token!.Split('.')[1].Replace('-', '+').Replace('_', '/');
                payload = payload.PadRight((payload.Length + 3) / 4 * 4, '=');
                using var stream = new MemoryStream(Convert.FromBase64String(payload));
                var claims = (Claims)new DataContractJsonSerializer(typeof(Claims)).ReadObject(stream)!;
                if (claims.Exp is not double exp || double.IsNaN(exp) || double.IsInfinity(exp))
                {
                    return true;
                }
                var margin = Math.Max(30, claims.Iat is double iat ? (exp - iat) * 0.05 : 0);
                return exp - now.ToUnixTimeMilliseconds() / 1000.0 <= margin;
            }
            catch
            {
                return true;
            }
        }
    }
}
