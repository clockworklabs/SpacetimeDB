// Tests of Ping/Pong on a connection, driven through the reconnect tests' fake socket.

#include "ReconnectTestConnection.h"
#include "Misc/AutomationTest.h"

#if WITH_DEV_AUTOMATION_TESTS

namespace SpacetimeDBPingTests
{
	FServerMessageType PongFor(const FPingType& Ping, int64 ServerReceiveUs, int64 ServerHoldUs)
	{
		FPongType Pong;
		Pong.RequestId = Ping.RequestId;
		Pong.ClientSendTime = Ping.ClientSendTime;
		Pong.ServerReceiveTime = FSpacetimeDBTimestamp(ServerReceiveUs);
		Pong.ServerHoldDuration = FSpacetimeDBTimeDuration(ServerHoldUs);
		return FServerMessageType::Pong(Pong);
	}

	/** When Ping was sent, on the FPlatformTime::Seconds() clock its send time is measured on. */
	double SentAt(const FPingType& Ping)
	{
		return double(Ping.ClientSendTime) / 1e6;
	}

	/** Whether Actual is within 2 microseconds of Expected milliseconds, allowing for clock rounding. */
	bool NearMs(const FTimespan& Actual, double ExpectedMs)
	{
		return FMath::Abs(Actual.GetTicks() - int64(ExpectedMs * ETimespan::TicksPerMillisecond)) <=
		       2 * ETimespan::TicksPerMicrosecond;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FPingConnectionTest, "SpacetimeDB.TestClient.PingConnection",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FPingConnectionTest::RunTest(const FString&)
{
	using namespace SpacetimeDBPingTests;
	constexpr int64 ServerTime = 1'700'000'000'000'000;

	auto* Conn = NewObject<UReconnectTestConnection>();
	Conn->Initialize();
	// Long enough that no periodic Ping falls due by itself during the test.
	Conn->SetPingInterval(FTimespan::FromHours(1.0));
	FSpacetimeDBNetworkStats Stats;

	Conn->PingAndRecord();
	TestTrue(TEXT("Ping before InitialConnection fails at once"),
	         Conn->PingOutcomes.Num() == 1 && !Conn->PingOutcomes[0].bSuccess);
	TestEqual(TEXT("nothing sent before InitialConnection"), Conn->Sent.Num(), 0);
	TestFalse(TEXT("no stats before connecting"), Conn->TryGetNetworkStats(Stats));

	Conn->ConnectIdentity();
	TArray<FPingType> Pings = Conn->SentPings();
	TestEqual(TEXT("one Ping straight after InitialConnection"), Pings.Num(), 1);
	TestFalse(TEXT("no stats before the first Pong"), Conn->TryGetNetworkStats(Stats));
	if (Pings.Num() != 1)
	{
		return false;
	}

	// The Pong arrives 50 ms after its Ping, 10 ms of which the server held it.
	Conn->InjectAt(PongFor(Pings[0], ServerTime, 10'000), SentAt(Pings[0]) + 0.050);
	TestTrue(TEXT("stats after the first Pong"), Conn->TryGetNetworkStats(Stats));
	TestTrue(TEXT("RTT excludes the server's hold"), NearMs(Stats.RttLatest, 40.0));
	TestTrue(TEXT("first sample seeds SRTT"), NearMs(Stats.Rtt, 40.0));
	TestEqual(TEXT("ServerTimeLatest is the Pong's"), Stats.ServerTimeLatest.GetMicroseconds(), ServerTime);
	TestTrue(TEXT("GetNetworkStats agrees"), Conn->GetNetworkStats().IsSet());
	TestEqual(TEXT("periodic Pong calls no Ping callback"), Conn->PingOutcomes.Num(), 1);

	Conn->PingAndRecord();
	Pings = Conn->SentPings();
	TestEqual(TEXT("Ping sends at once"), Pings.Num(), 2);
	if (Pings.Num() != 2)
	{
		return false;
	}
	TestTrue(TEXT("each Ping has its own request id"), Pings[0].RequestId != Pings[1].RequestId);
	Conn->InjectAt(PongFor(Pings[1], ServerTime + 1'000'000, 5'000), SentAt(Pings[1]) + 0.030);
	TestTrue(TEXT("Ping callback succeeds"), Conn->PingOutcomes.Num() == 2 && Conn->PingOutcomes[1].bSuccess);
	if (Conn->PingOutcomes.Num() == 2)
	{
		const FSpacetimeDBPingResult& Result = Conn->PingOutcomes[1].Result;
		TestTrue(TEXT("RoundTrip"), NearMs(Result.RoundTrip, 30.0));
		TestEqual(TEXT("ServerHold"), Result.ServerHold.GetTicks(), 5 * ETimespan::TicksPerMillisecond);
		TestTrue(TEXT("Rtt is RoundTrip less ServerHold"), NearMs(Result.Rtt, 25.0));
		TestEqual(TEXT("ServerReceiveTime"), Result.ServerReceiveTime.GetMicroseconds(), ServerTime + 1'000'000);
		TestTrue(TEXT("ReceivedAt - SentAt"), FMath::Abs(Result.ReceivedAt - Result.SentAt - 0.030) < 2e-6);
	}
	TestTrue(TEXT("on-demand Ping feeds the stats"),
	         Conn->TryGetNetworkStats(Stats) && NearMs(Stats.RttLatest, 25.0));

	Conn->PingDueNow();
	TestEqual(TEXT("periodic Ping once due"), Conn->SentPings().Num(), 3);
	Conn->SetPingInterval(FTimespan::Zero());
	Conn->PingDueNow();
	TestEqual(TEXT("no periodic Ping when disabled"), Conn->SentPings().Num(), 3);

	Conn->PingAndRecord();
	TestEqual(TEXT("Ping works with periodic pinging disabled"), Conn->SentPings().Num(), 4);
	Conn->Drop();
	TestTrue(TEXT("connection is recovering"), Conn->IsReconnecting());
	TestTrue(TEXT("pending Ping fails when the connection is lost"),
	         Conn->PingOutcomes.Num() == 3 && !Conn->PingOutcomes[2].bSuccess);
	TestFalse(TEXT("stats reset when the connection is lost"), Conn->TryGetNetworkStats(Stats));
	Conn->PingAndRecord();
	TestTrue(TEXT("Ping while reconnecting fails at once"),
	         Conn->PingOutcomes.Num() == 4 && !Conn->PingOutcomes[3].bSuccess);
	Conn->Disconnect();
	return true;
}

#endif
