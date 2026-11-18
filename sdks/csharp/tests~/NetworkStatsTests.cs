using System.Collections.Concurrent;
using System.Diagnostics;
using SpacetimeDB;
using SpacetimeDB.ClientApi;
using Xunit;

namespace SpacetimeDB.Tests;

public class NetworkStatsTrackerTests
{
    // A plausible server clock offset: monotonic client time vs. Unix epoch micros.
    private const long T0 = 1_700_000_000_000_000;

    // Record a Pong arriving at `now` with round-trip `r` and clock offset `theta`, with no server hold time.
    private static void Pong(NetworkStatsTracker tracker, ulong now, ulong r, long theta)
    {
        var clientSendTime = now - r;
        tracker.RecordPong(now, clientSendTime, (long)clientSendTime + (long)r / 2 + theta, 0);
    }

    private static long ServerNowUs(NetworkStats? stats) => stats!.Value.ServerNow.MicrosecondsSinceUnixEpoch;

    private static long OffsetAt(NetworkStatsTracker tracker, ulong now) => ServerNowUs(tracker.Snapshot(now)) - (long)now;

    [Fact]
    public void IsNullBeforeAnyPong() => Assert.Null(new NetworkStatsTracker().Snapshot(1_000_000));

    [Fact]
    public void FirstSampleSeedsSrttAndRttvar()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPong(1_000_000, 900_000, T0, 20_000);
        var stats = tracker.Snapshot(1_000_000)!.Value;
        Assert.Equal(80, stats.Rtt.TotalMilliseconds);
        Assert.Equal(40, stats.Jitter.TotalMilliseconds);
        Assert.Equal(80, stats.RttLatest.TotalMilliseconds);
        Assert.Equal(80, stats.RttMin.TotalMilliseconds);
    }

    [Fact]
    public void ClampsANegativeRoundTripToZero()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPong(1_000_000, 990_000, T0, 50_000);
        var stats = tracker.Snapshot(1_000_000)!.Value;
        Assert.Equal(TimeSpan.Zero, stats.RttLatest);
        Assert.Equal(TimeSpan.Zero, stats.Rtt);
        Assert.Equal(TimeSpan.Zero, stats.Jitter);
    }

    [Fact]
    public void UpdatesRttvarUsingThePreviousSrttThenSrtt()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 100_000, T0);
        Pong(tracker, 2_000_000, 200_000, T0);
        var stats = tracker.Snapshot(2_000_000)!.Value;
        Assert.Equal(62.5, stats.Jitter.TotalMilliseconds);
        Assert.Equal(112.5, stats.Rtt.TotalMilliseconds);
        Assert.Equal(200, stats.RttLatest.TotalMilliseconds);
    }

    [Fact]
    public void RttMinCoversOnlyTheRecentWindow()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        for (ulong i = 1; i < NetworkStatsTracker.Window; i++)
        {
            Pong(tracker, 1_000_000 + i * 1_000_000, 50_000, T0);
            Assert.Equal(10, tracker.Snapshot(1_000_000 + i * 1_000_000)!.Value.RttMin.TotalMilliseconds);
        }
        // The 10_000 sample is now the oldest of a full window; one more evicts it.
        Pong(tracker, 20_000_000, 50_000, T0);
        Assert.Equal(50, tracker.Snapshot(20_000_000)!.Value.RttMin.TotalMilliseconds);
    }

    [Fact]
    public void UsesTheOffsetOfTheLowestRSampleNotTheLatest()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0 + 5_000);
        Pong(tracker, 2_000_000, 50_000, T0 + 99_999);
        // Long enough for any slew to converge.
        Assert.Equal(T0 + 5_000, OffsetAt(tracker, 1_000_000_000));

        // Evict the low-R sample; ties then go to the most recent sample.
        for (ulong i = 0; i < NetworkStatsTracker.Window - 1; i++)
        {
            Pong(tracker, 1_000_000_001 + i, 50_000, T0 + 7_000);
        }
        Assert.Equal(T0 + 7_000, OffsetAt(tracker, 2_000_000_000));
    }

    [Fact]
    public void AppliesTheFirstOffsetImmediately()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        Assert.Equal(1_000_000 + T0, ServerNowUs(tracker.Snapshot(1_000_000)));
    }

    [Fact]
    public void ServerTimeLatestIsTheLatestPongsServerReceiveTime()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPong(1_000_000, 900_000, T0, 0);
        tracker.RecordPong(2_000_000, 1_900_000, T0 + 1_000_000, 0);
        // It's the raw measurement, so it doesn't advance between Pongs.
        Assert.Equal(new Timestamp(T0 + 1_000_000), tracker.Snapshot(2_500_000)!.Value.ServerTimeLatest);
    }

    [Fact]
    public void SlewsTowardsANewOffsetAtFivePercentOfElapsedTimeThenStops()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        // Same R, so the new sample wins the tie and the estimate jumps by +1 s.
        Pong(tracker, 2_000_000, 10_000, T0 + 1_000_000);
        Assert.Equal(T0, OffsetAt(tracker, 2_000_000));
        Assert.Equal(T0 + 50_000, OffsetAt(tracker, 3_000_000));
        Assert.Equal(T0 + 1_000_000, OffsetAt(tracker, 22_000_000));
        Assert.Equal(T0 + 1_000_000, OffsetAt(tracker, 30_000_000));
    }

    [Fact]
    public void ServerNowNeverGoesBackwards()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        var first = ServerNowUs(tracker.Snapshot(5_000_000));
        Assert.Equal(first, ServerNowUs(tracker.Snapshot(5_000_000 - 5_000)));

        // A large negative estimate pulls the offset down, but never the clock.
        Pong(tracker, 6_000_000, 10_000, T0 - 1_000_000_000);
        var previous = first;
        foreach (var now in new ulong[] { 6_000_000, 6_000_001, 7_000_000, 6_500_000, 60_000_000 })
        {
            var current = ServerNowUs(tracker.Snapshot(now));
            Assert.True(current >= previous, $"serverNow went backwards at {now}: {current} < {previous}");
            previous = current;
        }
    }

    [Fact]
    public void ResetClearsTheStatsAndTheNeverBackwardsClamp()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        var before = ServerNowUs(tracker.Snapshot(5_000_000));

        tracker.Reset();
        Assert.Null(tracker.Snapshot(5_000_000));

        Pong(tracker, 1_000_000, 30_000, T0 - 1_000_000);
        var stats = tracker.Snapshot(1_000_000)!.Value;
        Assert.Equal(1_000_000 + T0 - 1_000_000, ServerNowUs(stats));
        Assert.True(ServerNowUs(stats) < before);
        Assert.Equal(30, stats.Rtt.TotalMilliseconds);
        Assert.Equal(30, stats.RttMin.TotalMilliseconds);
    }

    [Fact]
    public void ASnapshotDoesNotChangeWhenLaterPongsArrive()
    {
        var tracker = new NetworkStatsTracker();
        Pong(tracker, 1_000_000, 10_000, T0);
        var a = tracker.Snapshot(1_000_000)!.Value;
        Assert.Equal((a.Rtt, a.ServerNow), (tracker.Snapshot(1_000_000)!.Value.Rtt, tracker.Snapshot(1_000_000)!.Value.ServerNow));

        Pong(tracker, 2_000_000, 50_000, T0);
        Assert.Equal(10, a.RttLatest.TotalMilliseconds);
        Assert.Equal(50, tracker.Snapshot(2_000_000)!.Value.RttLatest.TotalMilliseconds);
    }

    // Answer the Ping sent at `sentUs`, arriving `r` later.
    private static void Answer(NetworkStatsTracker tracker, ulong sentUs, ulong r) =>
        tracker.RecordPong(sentUs + r, sentUs, T0 + (long)sentUs, 0);

    [Fact]
    public void PongWaitIsZeroWhenEveryPingHasBeenAnswered()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        Answer(tracker, 1_000_000, 20_000);
        Assert.Equal(TimeSpan.Zero, tracker.Snapshot(5_000_000)!.Value.PongWait);
    }

    [Fact]
    public void PongWaitKeepsRisingWhilePongsStopArriving()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        Answer(tracker, 1_000_000, 20_000);
        tracker.RecordPing(2_000_000);
        tracker.RecordPing(3_000_000);

        // The other fields are frozen at their last good values...
        var early = tracker.Snapshot(2_500_000)!.Value;
        var late = tracker.Snapshot(6_000_000)!.Value;
        Assert.Equal(early.Rtt, late.Rtt);
        // ...but PongWait measures from the oldest unanswered Ping.
        Assert.Equal(500, early.PongWait.TotalMilliseconds);
        Assert.Equal(4000, late.PongWait.TotalMilliseconds);
    }

    [Fact]
    public void APongAnswersEveryPingSentUpToItsOwn()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        tracker.RecordPing(2_000_000);
        tracker.RecordPing(3_000_000);

        // The Pong for the second Ping overtakes the first's.
        Answer(tracker, 2_000_000, 20_000);
        Assert.Equal(500_000, tracker.PongWaitUs(3_500_000));

        // The first Pong arriving late changes nothing.
        Answer(tracker, 1_000_000, 2_100_000);
        Assert.Equal(500_000, tracker.PongWaitUs(3_500_000));
    }

    [Fact]
    public void PongWaitIsAvailableBeforeTheFirstPong()
    {
        var tracker = new NetworkStatsTracker();
        Assert.Equal(0, tracker.PongWaitUs(1_000_000));
        tracker.RecordPing(1_000_000);
        Assert.Null(tracker.Snapshot(4_000_000));
        Assert.Equal(3_000_000, tracker.PongWaitUs(4_000_000));
    }

    [Fact]
    public void IsUnresponsiveOnceTwoPingsAreUnansweredPastTheTimeout()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        tracker.RecordPing(2_000_000);
        Assert.False(tracker.IsUnresponsive(11_000_000, 10_000_000));
        Assert.True(tracker.IsUnresponsive(11_000_001, 10_000_000));
    }

    [Fact]
    public void ASingleUnansweredPingIsNeverUnresponsive()
    {
        // As after a long sleep: only one Ping went out, and the next tick will send another before deciding.
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        Assert.False(tracker.IsUnresponsive(1_000_000_000, 10_000_000));
    }

    [Fact]
    public void AllowsASlowLinkSeveralRtosBeforeGivingUp()
    {
        var tracker = new NetworkStatsTracker();
        // A steady 3 s round trip: RTO = SRTT + 4 * RTTVAR = 3 s + 4 * 1.5 s.
        tracker.RecordPing(0);
        Answer(tracker, 0, 3_000_000);
        const ulong rtoUs = 9_000_000;
        tracker.RecordPing(10_000_000);
        tracker.RecordPing(11_000_000);

        const ulong limitUs = NetworkStatsTracker.PingTimeoutRtoMultiple * rtoUs;
        Assert.False(tracker.IsUnresponsive(10_000_000 + limitUs, 10_000_000));
        Assert.True(tracker.IsUnresponsive(10_000_001 + limitUs, 10_000_000));
    }

    [Fact]
    public void ResetForgetsUnansweredPings()
    {
        var tracker = new NetworkStatsTracker();
        tracker.RecordPing(1_000_000);
        tracker.RecordPing(2_000_000);
        tracker.Reset();
        Assert.Equal(0, tracker.PongWaitUs(60_000_000));
        Assert.False(tracker.IsUnresponsive(60_000_000, 10_000_000));
    }
}

// The connection-level Ping tests share ReconnectTests' fake connection and socket.
public partial class ReconnectTests
{
    private static readonly Timestamp ServerReceiveTime = new(1_700_000_000_000_000);

    private static ServerMessage PongFor(Ping ping, TimeSpan serverHold = default) =>
        new ServerMessage.Pong(new(ping.RequestId, ping.ClientSendTime, ServerReceiveTime,
            new TimeDuration(serverHold.Ticks / 10)));

    private static Connection CreateWithoutPeriodicPings(bool automatic = true) =>
        Create(automatic, configure: builder => builder.WithPingInterval(TimeSpan.Zero));

    [Fact]
    public void PingsStartAtInitialConnectionAndRepeat()
    {
        using var conn = Create(configure: builder => builder.WithPingInterval(TimeSpan.FromMilliseconds(10)));
        conn.FrameTick();
        Assert.Empty(conn.Socket.Pings);
        Assert.IsType<InvalidOperationException>(conn.Ping().Exception!.InnerException);

        Establish(conn);
        Assert.NotEmpty(conn.Socket.Pings);
        Pump(conn, () => conn.Socket.Pings.Count >= 3);
        var pings = conn.Socket.Pings.ToArray();
        for (var i = 1; i < pings.Length; i++)
        {
            Assert.True(pings[i].RequestId > pings[i - 1].RequestId);
            Assert.True(pings[i].ClientSendTime >= pings[i - 1].ClientSendTime);
        }
        // No Pong yet, so no stats, but the oldest Ping is waiting.
        Assert.Null(conn.stats.Network);
        // Pings are kept out of the request trackers.
        Assert.Empty(conn.Socket.Sent);
        Assert.Equal(0, conn.stats.ReducerRequestTracker.GetRequestsAwaitingResponse());
        Assert.Equal(0, conn.stats.OneOffRequestTracker.GetRequestsAwaitingResponse());
    }

    [Fact]
    public async Task PingResolvesFromItsOwnPongWithoutFrameTick()
    {
        using var conn = CreateWithoutPeriodicPings();
        Establish(conn);
        Assert.Empty(conn.Socket.Pings);
        Assert.Null(conn.stats.Network);
        var parseSamples = conn.stats.ParseMessageTracker.GetSampleCount();
        var applySamples = conn.stats.ApplyMessageTracker.GetSampleCount();

        var task = conn.Ping();
        var ping = Assert.Single(conn.Socket.Pings);
        var receivedAt = Stopwatch.GetTimestamp() + Stopwatch.Frequency / 20;
        conn.Socket.Receive(PongFor(ping, TimeSpan.FromMilliseconds(20)), receivedAt);
        Assert.True(SpinWait.SpinUntil(() => task.IsCompleted, TimeSpan.FromSeconds(5)));

        var result = await task;
        Assert.Equal(TimeSpan.FromMilliseconds(20), result.ServerHold);
        Assert.Equal(result.RoundTrip - result.ServerHold, result.Rtt);
        Assert.True(result.RoundTrip >= TimeSpan.FromMilliseconds(50));
        Assert.Equal(ServerReceiveTime, result.ServerReceiveTime);
        Assert.Equal(receivedAt, result.ReceivedAt);
        Assert.True(result.SentAt < result.ReceivedAt);

        // The sample feeds the stats too, and the Pong stays out of the message trackers.
        var stats = conn.stats.Network!.Value;
        Assert.Equal(result.Rtt, stats.RttLatest);
        Assert.Equal(TimeSpan.Zero, stats.PongWait);
        Assert.Equal(ServerReceiveTime, stats.ServerTimeLatest);
        conn.FrameTick();
        Assert.Equal(parseSamples, conn.stats.ParseMessageTracker.GetSampleCount());
        Assert.Equal(applySamples, conn.stats.ApplyMessageTracker.GetSampleCount());
        Assert.Equal(0, conn.stats.ParseMessageQueueTracker.GetRequestsAwaitingResponse());
        Assert.Empty(conn.Events);
    }

    [Fact]
    public void StampPingGivesAPingTheTimeItLeavesTheSendQueue()
    {
        var now = Stopwatch.GetTimestamp();
        var ping = new Ping(7, 1);
        SpacetimeDB.WebSocket.StampPing(new ClientMessage.Ping(ping), now);
        Assert.Equal(7u, ping.RequestId);
        Assert.Equal(NetworkStatsTracker.Micros(now), ping.ClientSendTime);

        var query = new OneOffQuery { RequestId = 1, QueryString = "SELECT 1" };
        SpacetimeDB.WebSocket.StampPing(new ClientMessage.OneOffQuery(query), now);
        Assert.Equal(1u, query.RequestId);
        Assert.Equal("SELECT 1", query.QueryString);
    }

    [Fact]
    public void StampPingDoesNotAllocate()
    {
        var message = new ClientMessage.Ping(new Ping(7, 1));
        var other = new ClientMessage.OneOffQuery(new OneOffQuery { RequestId = 1, QueryString = "SELECT 1" });
        var now = Stopwatch.GetTimestamp();
        // Warm up, so that JIT compilation isn't counted.
        SpacetimeDB.WebSocket.StampPing(message, now);
        SpacetimeDB.WebSocket.StampPing(other, now);

        var before = GC.GetAllocatedBytesForCurrentThread();
        SpacetimeDB.WebSocket.StampPing(message, now);
        SpacetimeDB.WebSocket.StampPing(other, now);
        Assert.Equal(before, GC.GetAllocatedBytesForCurrentThread());
    }

    // The native socket stamps a Ping as it leaves the send queue, so the Pong echoes a later send time
    // than the Ping() call's. Every measurement must start from the echoed time.
    [Fact]
    public async Task PingIsMeasuredFromTheSendTimeItsPongEchoes()
    {
        using var conn = CreateWithoutPeriodicPings();
        Establish(conn);
        var task = conn.Ping();
        var queued = Assert.Single(conn.Socket.Pings);

        // Spend 30 ms in the send queue, then 50 ms on the network.
        var sentUs = queued.ClientSendTime + 30_000;
        var receivedAt = NetworkStatsTracker.StopwatchTimestamp(sentUs + 50_000);
        conn.Socket.Receive(PongFor(new Ping(queued.RequestId, sentUs)), receivedAt);
        var result = await task.WaitAsync(TimeSpan.FromSeconds(5));

        // Converting between microseconds and Stopwatch ticks may lose up to a microsecond.
        Assert.InRange(result.RoundTrip, TimeSpan.FromMilliseconds(50) - TimeSpan.FromTicks(10), TimeSpan.FromMilliseconds(50));
        Assert.Equal(result.RoundTrip, result.Rtt);
        Assert.Equal(NetworkStatsTracker.StopwatchTimestamp(sentUs), result.SentAt);
        var stats = conn.stats.Network!.Value;
        Assert.Equal(result.Rtt, stats.RttLatest);
        Assert.Equal(TimeSpan.Zero, stats.PongWait);
    }

    [Fact]
    public void LosingTheConnectionFailsPendingPingsAndResetsTheStats()
    {
        using var conn = CreateWithoutPeriodicPings();
        Establish(conn);
        var answered = conn.Ping();
        conn.Socket.Receive(PongFor(Assert.Single(conn.Socket.Pings)));
        Assert.True(SpinWait.SpinUntil(() => answered.IsCompletedSuccessfully, TimeSpan.FromSeconds(5)));
        Assert.NotNull(conn.stats.Network);

        var pending = conn.Ping();
        var lostPing = conn.Socket.Pings.Last();
        var oldSocket = conn.Socket;
        Drop(conn);
        Assert.IsType<OperationCanceledException>(pending.Exception!.InnerException);
        Assert.Null(conn.stats.Network);
        Assert.IsType<InvalidOperationException>(conn.Ping().Exception!.InnerException);

        Retry(conn);
        Establish(conn);
        // A late Pong from the replaced socket doesn't reach the new connection's stats.
        oldSocket.Receive(PongFor(lostPing));
        conn.FrameTick();
        Assert.Null(conn.stats.Network);
    }

    [Fact]
    public async Task ConcurrentPingsGetDistinctRequestIds()
    {
        using var conn = Create(configure: builder => builder.WithPingInterval(TimeSpan.FromMilliseconds(1)));
        Establish(conn);
        var callers = Enumerable.Range(0, 4).Select(_ => Task.Run(() =>
        {
            for (var i = 0; i < 500; i++)
            {
                Assert.False(conn.Ping().IsFaulted);
            }
        })).ToArray();
        Pump(conn, () => callers.All(caller => caller.IsCompleted));
        await Task.WhenAll(callers);

        var ids = conn.Socket.Pings.Select(ping => ping.RequestId).ToList();
        Assert.True(ids.Count > 4 * 500);
        Assert.Equal(ids.Count, ids.Distinct().Count());
    }

    [Fact]
    public async Task PingsRacingAConnectionLossAllComplete()
    {
        using var conn = CreateWithoutPeriodicPings();
        Establish(conn);
        var stop = false;
        var tasks = new ConcurrentQueue<Task<PingResult>>();
        var callers = Enumerable.Range(0, 4).Select(_ => Task.Run(() =>
        {
            while (!Volatile.Read(ref stop))
            {
                tasks.Enqueue(conn.Ping());
            }
        })).ToArray();
        Assert.True(SpinWait.SpinUntil(() => conn.Socket.Pings.Count > 100, TimeSpan.FromSeconds(5)));

        Drop(conn);
        // Keep pinging past the loss, so that some calls race the failure handling.
        Thread.Sleep(10);
        Volatile.Write(ref stop, true);
        await Task.WhenAll(callers);

        Assert.All(tasks, task => Assert.True(task.IsCompleted));
        Assert.Contains(tasks, task => task.Exception?.InnerException is OperationCanceledException);
        Assert.Contains(tasks, task => task.Exception?.InnerException is InvalidOperationException);
        // No send time from the lost socket survives the reset to inflate the next connection's PongWait.
        lock (conn.stats.NetworkTracker)
        {
            Assert.Equal(0, conn.stats.NetworkTracker.PongWaitUs(ulong.MaxValue));
        }
    }

    [Fact]
    public void DisconnectFailsPendingPingsAndResetsTheStats()
    {
        using var conn = CreateWithoutPeriodicPings(automatic: false);
        Establish(conn);
        var answered = conn.Ping();
        conn.Socket.Receive(PongFor(Assert.Single(conn.Socket.Pings)));
        Assert.True(SpinWait.SpinUntil(() => answered.IsCompletedSuccessfully, TimeSpan.FromSeconds(5)));
        Assert.NotNull(conn.stats.Network);

        var pending = conn.Ping();
        conn.Disconnect();
        Assert.IsType<OperationCanceledException>(pending.Exception!.InnerException);
        Assert.Null(conn.stats.Network);
        Assert.IsType<InvalidOperationException>(conn.Ping().Exception!.InnerException);
    }
}
