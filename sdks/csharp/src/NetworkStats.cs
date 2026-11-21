using System;
using System.Collections.Generic;
using System.Diagnostics;

namespace SpacetimeDB
{
    /// <summary>
    /// A snapshot of a connection's network statistics, derived from Ping/Pong round trips.
    /// </summary>
    public readonly struct NetworkStats
    {
        /// <summary>Smoothed round-trip time (RFC 6298 SRTT).</summary>
        public TimeSpan Rtt { get; }
        /// <summary>The most recent round-trip time sample.</summary>
        public TimeSpan RttLatest { get; }
        /// <summary>The lowest round-trip time over the recent sample window.</summary>
        public TimeSpan RttMin { get; }
        /// <summary>Round-trip time variation (RFC 6298 RTTVAR).</summary>
        public TimeSpan Jitter { get; }
        /// <summary>
        /// The estimated current time on the server, on the same clock as a reducer's <c>ctx.Timestamp</c>.
        /// Adjusts gradually and never goes backwards.
        /// </summary>
        public Timestamp ServerNow { get; }
        /// <summary>
        /// When the server received the most recent Ping, as reported in its Pong.
        /// This is the raw measurement <see cref="ServerNow"/> is derived from: it doesn't advance between Pongs.
        /// </summary>
        public Timestamp ServerTimeLatest { get; }
        /// <summary>
        /// How long the oldest unanswered Ping has been waiting for its Pong,
        /// or zero when every Ping has been answered.
        ///
        /// On a healthy connection this stays below <see cref="Rtt"/>. The other properties only
        /// change when a Pong arrives, so they keep showing the last good values while the network
        /// is silently down; this is the one that keeps rising.
        /// </summary>
        public TimeSpan PongWait { get; }

        internal NetworkStats(TimeSpan rtt, TimeSpan rttLatest, TimeSpan rttMin, TimeSpan jitter,
            Timestamp serverNow, Timestamp serverTimeLatest, TimeSpan pongWait)
        {
            Rtt = rtt;
            RttLatest = rttLatest;
            RttMin = rttMin;
            Jitter = jitter;
            ServerNow = serverNow;
            ServerTimeLatest = serverTimeLatest;
            PongWait = pongWait;
        }
    }

    /// <summary>
    /// The result of a single Ping sent with <c>DbConnection.Ping()</c>.
    /// </summary>
    public readonly struct PingResult
    {
        /// <summary>
        /// The round-trip time for this Ping, excluding the time the server held it:
        /// <see cref="RoundTrip"/> minus <see cref="ServerHold"/>, floored at zero.
        /// This is the sample that feeds <see cref="NetworkStats"/>.
        /// </summary>
        public TimeSpan Rtt { get; }
        /// <summary>The time from sending the Ping to receiving its Pong.</summary>
        public TimeSpan RoundTrip { get; }
        /// <summary>How long the server held the Ping before replying.</summary>
        public TimeSpan ServerHold { get; }
        /// <summary>When the server received the Ping, on the same clock as a reducer's <c>ctx.Timestamp</c>.</summary>
        public Timestamp ServerReceiveTime { get; }
        /// <summary>
        /// When the Ping was sent, as a <see cref="Stopwatch.GetTimestamp"/> reading, to the microsecond.
        /// This is when it left the client's send queue, not when <c>Ping()</c> was called.
        /// </summary>
        public long SentAt { get; }
        /// <summary>When the Pong arrived, as a <see cref="Stopwatch.GetTimestamp"/> reading.</summary>
        public long ReceivedAt { get; }

        internal PingResult(TimeSpan rtt, TimeSpan roundTrip, TimeSpan serverHold, Timestamp serverReceiveTime,
            long sentAt, long receivedAt)
        {
            Rtt = rtt;
            RoundTrip = roundTrip;
            ServerHold = serverHold;
            ServerReceiveTime = serverReceiveTime;
            SentAt = sentAt;
            ReceivedAt = receivedAt;
        }
    }

    /// <summary>
    /// Tracks round-trip time and server clock offset from Pong replies.
    ///
    /// Pure: every time value is in microseconds and supplied by the caller, so the maths can be
    /// tested deterministically. <c>now</c> values are on the client's monotonic clock
    /// (see <see cref="Micros"/>); server times are microseconds since the Unix epoch.
    /// Not thread-safe: callers lock on the tracker.
    /// </summary>
    internal sealed class NetworkStatsTracker
    {
        /// <summary>The default interval between periodic Pings.</summary>
        internal static readonly TimeSpan DefaultPingInterval = TimeSpan.FromSeconds(1);

        /// <summary>The default for how long Pings may go unanswered before a WebGL connection is treated as lost.</summary>
        internal static readonly TimeSpan DefaultPingTimeout = TimeSpan.FromSeconds(10);

        /// <summary>How many recent samples feed <c>RttMin</c> and the clock offset estimate.</summary>
        internal const int Window = 10;

        /// <summary>
        /// The fastest the applied clock offset may move towards a new estimate,
        /// as a fraction of the monotonic time that has elapsed.
        /// </summary>
        internal const double MaxSlew = 0.05;

        /// <summary>
        /// How many RTOs a Ping may go unanswered before the connection is considered unresponsive,
        /// when that is longer than the configured Ping timeout.
        /// </summary>
        internal const int PingTimeoutRtoMultiple = 4;

        // RFC 6298 smoothing factors and clock granularity.
        private const double Alpha = 1.0 / 8;
        private const double Beta = 1.0 / 4;
        private const double K = 4;
        private const double GranularityUs = 1_000;

        private double srtt;
        private double rttvar;
        private double latestR;
        private long latestServerReceiveUs;
        private readonly List<(double R, double Theta)> samples = new();
        private double estimate;
        private double? appliedOffset;
        private double lastSlewAt;
        private double lastServerNowUs = double.NegativeInfinity;
        // Send times of Pings with no Pong yet, oldest first.
        private readonly List<ulong> unansweredSendTimesUs = new();

        /// <summary>A <see cref="Stopwatch.GetTimestamp"/> reading in microseconds, the unit of a Ping's <c>client_send_time</c>.</summary>
        internal static ulong Micros(long stopwatchTimestamp) =>
            (ulong)(stopwatchTimestamp * (1_000_000.0 / Stopwatch.Frequency));

        /// <summary>
        /// The inverse of <see cref="Micros"/>: a <see cref="Stopwatch.GetTimestamp"/> reading,
        /// at most one microsecond earlier than the one that produced <paramref name="micros"/>.
        /// </summary>
        internal static long StopwatchTimestamp(ulong micros) =>
            (long)(micros * (Stopwatch.Frequency / 1_000_000.0));

        internal static TimeSpan ToTimeSpan(double micros) =>
            TimeSpan.FromTicks((long)Math.Round(micros * (TimeSpan.TicksPerMillisecond / 1000.0)));

        /// <summary>Record that a Ping went out at <paramref name="sentUs"/>, its <c>client_send_time</c>.</summary>
        internal void RecordPing(ulong sentUs) => unansweredSendTimesUs.Add(sentUs);

        /// <summary>
        /// Record a Pong which arrived at <paramref name="nowUs"/>, and return its round-trip sample
        /// in microseconds: the round trip less the server's hold time, floored at zero.
        /// </summary>
        internal double RecordPong(ulong nowUs, ulong clientSendTimeUs, long serverReceiveTimeUs, long serverHoldUs)
        {
            // A Pong proves the link was alive when its Ping arrived, so it answers every Ping sent
            // up to then. Pongs can overtake one another on the server, so this doesn't assume
            // they arrive in order.
            unansweredSendTimesUs.RemoveAll(sentUs => sentUs <= clientSendTimeUs);

            var r = Math.Max(0, (double)nowUs - clientSendTimeUs - serverHoldUs);
            if (samples.Count == 0)
            {
                srtt = r;
                rttvar = r / 2;
            }
            else
            {
                // RFC 6298 requires RTTVAR to be updated using the previous SRTT.
                rttvar = (1 - Beta) * rttvar + Beta * Math.Abs(srtt - r);
                srtt = (1 - Alpha) * srtt + Alpha * r;
            }
            latestR = r;
            latestServerReceiveUs = serverReceiveTimeUs;

            var theta = serverReceiveTimeUs - (clientSendTimeUs + r / 2);
            samples.Add((r, theta));
            if (samples.Count > Window)
            {
                samples.RemoveAt(0);
            }

            if (appliedOffset == null)
            {
                estimate = MinSample().Theta;
                appliedOffset = estimate;
                lastSlewAt = nowUs;
            }
            else
            {
                // Slew up to now towards the old estimate before switching to the new one.
                Advance(nowUs);
                estimate = MinSample().Theta;
            }
            return r;
        }

        /// <summary>
        /// A fresh snapshot of the statistics at monotonic time <paramref name="nowUs"/>,
        /// or null if no Pong has been recorded since construction or <see cref="Reset"/>.
        /// </summary>
        internal NetworkStats? Snapshot(ulong nowUs)
        {
            if (samples.Count == 0)
            {
                return null;
            }
            Advance(nowUs);
            var serverNowUs = Math.Max(nowUs + appliedOffset!.Value, lastServerNowUs);
            lastServerNowUs = serverNowUs;
            return new NetworkStats(
                rtt: ToTimeSpan(srtt),
                rttLatest: ToTimeSpan(latestR),
                rttMin: ToTimeSpan(MinSample().R),
                jitter: ToTimeSpan(rttvar),
                serverNow: new Timestamp((long)Math.Round(serverNowUs)),
                serverTimeLatest: new Timestamp(latestServerReceiveUs),
                pongWait: ToTimeSpan(PongWaitUs(nowUs)));
        }

        /// <summary>
        /// Whether the connection has stopped answering Pings at monotonic time <paramref name="nowUs"/>:
        /// the oldest unanswered Ping has waited longer than <paramref name="timeoutUs"/>
        /// (or than <see cref="PingTimeoutRtoMultiple"/> RTOs, if longer), and at least one more Ping
        /// has gone out since it.
        ///
        /// Requiring a second Ping means a single stale send time, such as one from before the
        /// machine slept, can't trip the timeout on its own: the connection must also miss a Ping sent since.
        /// </summary>
        internal bool IsUnresponsive(ulong nowUs, double timeoutUs)
        {
            if (unansweredSendTimesUs.Count < 2)
            {
                return false;
            }
            var limitUs = Math.Max(timeoutUs, PingTimeoutRtoMultiple * RtoUs());
            return PongWaitUs(nowUs) > limitUs;
        }

        /// <summary>Forget all samples, unanswered Pings, the clock offset and the never-backwards clamp.</summary>
        internal void Reset()
        {
            srtt = 0;
            rttvar = 0;
            latestR = 0;
            latestServerReceiveUs = 0;
            samples.Clear();
            estimate = 0;
            appliedOffset = null;
            lastSlewAt = 0;
            lastServerNowUs = double.NegativeInfinity;
            unansweredSendTimesUs.Clear();
        }

        /// <summary>
        /// How long the oldest unanswered Ping has waited at monotonic time <paramref name="nowUs"/>,
        /// in microseconds, or 0 if every Ping has been answered. Unlike <see cref="Snapshot"/>,
        /// this is available before the first Pong.
        /// </summary>
        internal double PongWaitUs(ulong nowUs) =>
            unansweredSendTimesUs.Count == 0 ? 0 : Math.Max(0, (double)nowUs - unansweredSendTimesUs[0]);

        // The RFC 6298 retransmission timeout, measured from the samples alone.
        // RFC 6298's 1 s initial value and floor are left out, so that the
        // configured timeout governs until a slow link shows it needs longer.
        private double RtoUs() => samples.Count == 0 ? 0 : srtt + Math.Max(GranularityUs, K * rttvar);

        // The lowest-R sample in the window. Ties go to the most recent sample.
        private (double R, double Theta) MinSample()
        {
            var min = samples[0];
            foreach (var sample in samples)
            {
                if (sample.R <= min.R)
                {
                    min = sample;
                }
            }
            return min;
        }

        // Move the applied offset towards the estimate, by at most
        // `MaxSlew` of the monotonic time elapsed since the last move.
        private void Advance(double nowUs)
        {
            if (appliedOffset is not double applied)
            {
                return;
            }
            var step = MaxSlew * Math.Max(0, nowUs - lastSlewAt);
            var delta = estimate - applied;
            appliedOffset = applied + Math.Max(-step, Math.Min(step, delta));
            lastSlewAt = Math.Max(lastSlewAt, nowUs);
        }
    }
}
