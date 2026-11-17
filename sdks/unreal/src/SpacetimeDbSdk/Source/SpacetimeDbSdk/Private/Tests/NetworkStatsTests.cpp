// Deterministic tests of the network stats maths, ported from the Rust SDK's network_stats.rs.

#include "Connection/NetworkStats.h"
#include "Misc/AutomationTest.h"

#if WITH_DEV_AUTOMATION_TESTS

// Named rather than anonymous, so unity builds can't merge these helpers with another test file's.
namespace SpacetimeDBNetworkStatsTests
{
	// A plausible server clock offset: monotonic client time vs. Unix epoch micros.
	constexpr int64 T0 = 1'700'000'000'000'000;

	int64 MsTicks(double Ms)
	{
		return static_cast<int64>(Ms * ETimespan::TicksPerMillisecond);
	}

	// Record a Pong arriving at Now with round trip R and clock offset Theta, with no server hold time.
	void RecordPong(FSpacetimeDBNetworkStatsTracker& Tracker, uint64 Now, uint64 R, int64 Theta)
	{
		const uint64 ClientSendTime = Now - R;
		Tracker.RecordPong(Now, ClientSendTime, int64(ClientSendTime) + int64(R / 2) + Theta, 0);
	}

	// Answer the Ping sent at SentUs, arriving R later.
	void Answer(FSpacetimeDBNetworkStatsTracker& Tracker, uint64 SentUs, uint64 R)
	{
		Tracker.RecordPong(SentUs + R, SentUs, T0 + int64(SentUs), 0);
	}

	int64 ServerNowUs(FSpacetimeDBNetworkStatsTracker& Tracker, uint64 Now)
	{
		return Tracker.Snapshot(Now).GetValue().ServerNow.GetMicroseconds();
	}

	int64 OffsetAt(FSpacetimeDBNetworkStatsTracker& Tracker, uint64 Now)
	{
		return ServerNowUs(Tracker, Now) - int64(Now);
	}
}

using namespace SpacetimeDBNetworkStatsTests;

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FNetworkStatsTrackerTest, "SpacetimeDB.TestClient.NetworkStatsTracker",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FNetworkStatsTrackerTest::RunTest(const FString&)
{
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		TestFalse(TEXT("unset before any Pong"), Tracker.Snapshot(1'000'000).IsSet());
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPong(1'000'000, 900'000, T0, 20'000);
		const FSpacetimeDBNetworkStats Stats = Tracker.Snapshot(1'000'000).GetValue();
		TestEqual(TEXT("first sample seeds SRTT"), Stats.Rtt.GetTicks(), MsTicks(80));
		TestEqual(TEXT("first sample seeds RTTVAR as R/2"), Stats.Jitter.GetTicks(), MsTicks(40));
		TestEqual(TEXT("first sample is the latest"), Stats.RttLatest.GetTicks(), MsTicks(80));
		TestEqual(TEXT("first sample is the minimum"), Stats.RttMin.GetTicks(), MsTicks(80));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPong(1'000'000, 990'000, T0, 50'000);
		const FSpacetimeDBNetworkStats Stats = Tracker.Snapshot(1'000'000).GetValue();
		TestEqual(TEXT("negative round trip clamps to zero"), Stats.RttLatest.GetTicks(), int64(0));
		TestEqual(TEXT("negative round trip gives zero SRTT"), Stats.Rtt.GetTicks(), int64(0));
		TestEqual(TEXT("negative round trip gives zero RTTVAR"), Stats.Jitter.GetTicks(), int64(0));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 100'000, T0);
		RecordPong(Tracker, 2'000'000, 200'000, T0);
		const FSpacetimeDBNetworkStats Stats = Tracker.Snapshot(2'000'000).GetValue();
		TestEqual(TEXT("RTTVAR uses the previous SRTT"), Stats.Jitter.GetTicks(), MsTicks(62.5));
		TestEqual(TEXT("SRTT updated after RTTVAR"), Stats.Rtt.GetTicks(), MsTicks(112.5));
		TestEqual(TEXT("latest sample"), Stats.RttLatest.GetTicks(), MsTicks(200));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		for (uint64 I = 1; I < FSpacetimeDBNetworkStatsTracker::Window; ++I)
		{
			const uint64 Now = 1'000'000 + I * 1'000'000;
			RecordPong(Tracker, Now, 50'000, T0);
			TestEqual(TEXT("RttMin within the window"), Tracker.Snapshot(Now).GetValue().RttMin.GetTicks(), MsTicks(10));
		}
		// The 10 ms sample is now the oldest of a full window; one more evicts it.
		RecordPong(Tracker, 20'000'000, 50'000, T0);
		TestEqual(TEXT("RttMin covers only the recent window"),
		          Tracker.Snapshot(20'000'000).GetValue().RttMin.GetTicks(), MsTicks(50));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0 + 5'000);
		RecordPong(Tracker, 2'000'000, 50'000, T0 + 99'999);
		// Long enough for any slew to converge.
		TestEqual(TEXT("offset of the lowest-R sample"), OffsetAt(Tracker, 1'000'000'000), T0 + 5'000);
		// Evict the low-R sample; ties then go to the most recent sample.
		for (uint64 I = 0; I < FSpacetimeDBNetworkStatsTracker::Window - 1; ++I)
		{
			RecordPong(Tracker, 1'000'000'001 + I, 50'000, T0 + 7'000);
		}
		TestEqual(TEXT("ties go to the most recent sample"), OffsetAt(Tracker, 2'000'000'000), T0 + 7'000);
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		TestEqual(TEXT("first offset applies immediately"), ServerNowUs(Tracker, 1'000'000), 1'000'000 + T0);
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPong(1'000'000, 900'000, T0, 0);
		Tracker.RecordPong(2'000'000, 1'900'000, T0 + 1'000'000, 0);
		// It's the raw measurement, so it doesn't advance between Pongs.
		TestEqual(TEXT("ServerTimeLatest is the latest Pong's receive time"),
		          Tracker.Snapshot(2'500'000).GetValue().ServerTimeLatest.GetMicroseconds(), T0 + 1'000'000);
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		// Same R, so the new sample wins the tie and the estimate jumps by +1 s.
		RecordPong(Tracker, 2'000'000, 10'000, T0 + 1'000'000);
		TestEqual(TEXT("slew starts from the old offset"), OffsetAt(Tracker, 2'000'000), T0);
		TestEqual(TEXT("slews at 5% of elapsed time"), OffsetAt(Tracker, 3'000'000), T0 + 50'000);
		TestEqual(TEXT("slew reaches the estimate"), OffsetAt(Tracker, 22'000'000), T0 + 1'000'000);
		TestEqual(TEXT("slew stops at the estimate"), OffsetAt(Tracker, 30'000'000), T0 + 1'000'000);
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		const int64 First = ServerNowUs(Tracker, 5'000'000);
		TestEqual(TEXT("an earlier read doesn't go backwards"), ServerNowUs(Tracker, 5'000'000 - 5'000), First);

		// A large negative estimate pulls the offset down, but never the clock.
		RecordPong(Tracker, 6'000'000, 10'000, T0 - 1'000'000'000);
		int64 Previous = First;
		for (const uint64 Now : {6'000'000ull, 6'000'001ull, 7'000'000ull, 6'500'000ull, 60'000'000ull})
		{
			const int64 Current = ServerNowUs(Tracker, Now);
			TestTrue(FString::Printf(TEXT("ServerNow never goes backwards at %llu"), Now), Current >= Previous);
			Previous = Current;
		}
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		const int64 Before = ServerNowUs(Tracker, 5'000'000);

		Tracker.Reset();
		TestFalse(TEXT("reset clears the stats"), Tracker.Snapshot(5'000'000).IsSet());

		RecordPong(Tracker, 1'000'000, 30'000, T0 - 1'000'000);
		const FSpacetimeDBNetworkStats Stats = Tracker.Snapshot(1'000'000).GetValue();
		TestEqual(TEXT("reset applies the new offset at once"), Stats.ServerNow.GetMicroseconds(),
		          1'000'000 + T0 - 1'000'000);
		TestTrue(TEXT("reset clears the never-backwards clamp"), Stats.ServerNow.GetMicroseconds() < Before);
		TestEqual(TEXT("reset clears SRTT"), Stats.Rtt.GetTicks(), MsTicks(30));
		TestEqual(TEXT("reset clears RttMin"), Stats.RttMin.GetTicks(), MsTicks(30));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		RecordPong(Tracker, 1'000'000, 10'000, T0);
		const FSpacetimeDBNetworkStats A = Tracker.Snapshot(1'000'000).GetValue();
		const FSpacetimeDBNetworkStats B = Tracker.Snapshot(1'000'000).GetValue();
		TestEqual(TEXT("repeat snapshots agree on ServerNow"), A.ServerNow.GetMicroseconds(), B.ServerNow.GetMicroseconds());
		TestEqual(TEXT("repeat snapshots agree on Rtt"), A.Rtt.GetTicks(), B.Rtt.GetTicks());
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FNetworkStatsPongWaitTest, "SpacetimeDB.TestClient.NetworkStatsPongWait",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FNetworkStatsPongWaitTest::RunTest(const FString&)
{
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPing(1'000'000);
		Answer(Tracker, 1'000'000, 20'000);
		TestEqual(TEXT("zero when every Ping is answered"), Tracker.Snapshot(5'000'000).GetValue().PongWait.GetTicks(),
		          int64(0));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPing(1'000'000);
		Answer(Tracker, 1'000'000, 20'000);
		Tracker.RecordPing(2'000'000);
		Tracker.RecordPing(3'000'000);
		// The other fields are frozen at their last good values...
		const FSpacetimeDBNetworkStats Early = Tracker.Snapshot(2'500'000).GetValue();
		const FSpacetimeDBNetworkStats Late = Tracker.Snapshot(6'000'000).GetValue();
		TestEqual(TEXT("Rtt frozen while Pongs stop"), Late.Rtt.GetTicks(), Early.Rtt.GetTicks());
		// ...but PongWait measures from the oldest unanswered Ping.
		TestEqual(TEXT("PongWait early"), Early.PongWait.GetTicks(), MsTicks(500));
		TestEqual(TEXT("PongWait keeps rising"), Late.PongWait.GetTicks(), MsTicks(4'000));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPing(1'000'000);
		Tracker.RecordPing(2'000'000);
		Tracker.RecordPing(3'000'000);
		// The Pong for the second Ping overtakes the first's.
		Answer(Tracker, 2'000'000, 20'000);
		TestEqual(TEXT("a Pong answers every Ping sent up to its own"), int64(Tracker.PongWaitUs(3'500'000)), int64(500'000));
		// The first Pong arriving late changes nothing.
		Answer(Tracker, 1'000'000, 2'100'000);
		TestEqual(TEXT("a late Pong changes nothing"), int64(Tracker.PongWaitUs(3'500'000)), int64(500'000));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		TestEqual(TEXT("zero with no Pings"), int64(Tracker.PongWaitUs(1'000'000)), int64(0));
		Tracker.RecordPing(1'000'000);
		TestFalse(TEXT("no stats before the first Pong"), Tracker.Snapshot(4'000'000).IsSet());
		TestEqual(TEXT("PongWait available before the first Pong"), int64(Tracker.PongWaitUs(4'000'000)), int64(3'000'000));
	}
	{
		FSpacetimeDBNetworkStatsTracker Tracker;
		Tracker.RecordPing(1'000'000);
		Tracker.RecordPing(2'000'000);
		Tracker.Reset();
		TestEqual(TEXT("reset forgets unanswered Pings"), int64(Tracker.PongWaitUs(60'000'000)), int64(0));
	}
	return true;
}

#endif
