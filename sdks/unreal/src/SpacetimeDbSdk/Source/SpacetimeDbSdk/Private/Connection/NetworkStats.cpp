#include "Connection/NetworkStats.h"

namespace
{
	/** The fastest the applied clock offset may move towards a new estimate, as a fraction of elapsed time. */
	constexpr double MaxSlew = 0.05;

	// RFC 6298 smoothing factors.
	constexpr double Alpha = 1.0 / 8.0;
	constexpr double Beta = 1.0 / 4.0;

	/** Convert a number of microseconds to an FTimespan, floored at zero and rounded to the tick. */
	FTimespan TimespanFromUs(double Us)
	{
		return FTimespan(static_cast<int64>(FMath::RoundToDouble(FMath::Max(Us, 0.0) * ETimespan::TicksPerMicrosecond)));
	}
}

void FSpacetimeDBNetworkStatsTracker::RecordPing(uint64 SentUs)
{
	UnansweredSendTimesUs.Add(SentUs);
}

void FSpacetimeDBNetworkStatsTracker::RecordPong(uint64 NowUs, uint64 ClientSendTimeUs, int64 ServerReceiveTimeUs,
                                                 int64 ServerHoldUs)
{
	// A Pong proves the link was alive when its Ping arrived, so it answers every Ping sent up
	// to then. Pongs can overtake one another on the server, so this doesn't assume they arrive
	// in order.
	UnansweredSendTimesUs.RemoveAll([ClientSendTimeUs](uint64 SentUs) { return SentUs <= ClientSendTimeUs; });

	const double R = FMath::Max(0.0, double(NowUs) - double(ClientSendTimeUs) - double(ServerHoldUs));
	if (Samples.IsEmpty())
	{
		Srtt = R;
		RttVar = R / 2.0;
	}
	else
	{
		// RFC 6298 requires RTTVAR to be updated using the previous SRTT.
		RttVar = (1.0 - Beta) * RttVar + Beta * FMath::Abs(Srtt - R);
		Srtt = (1.0 - Alpha) * Srtt + Alpha * R;
	}
	LatestR = R;
	LatestServerReceiveUs = ServerReceiveTimeUs;

	FSample Sample;
	Sample.R = R;
	Sample.Theta = double(ServerReceiveTimeUs) - (double(ClientSendTimeUs) + R / 2.0);
	Samples.Add(Sample);
	if (Samples.Num() > Window)
	{
		Samples.RemoveAt(0);
	}

	if (!AppliedOffset.IsSet())
	{
		Estimate = MinSample().Theta;
		AppliedOffset = Estimate;
		LastSlewAt = double(NowUs);
	}
	else
	{
		// Slew up to now towards the old estimate before switching to the new one.
		Advance(NowUs);
		Estimate = MinSample().Theta;
	}
}

TOptional<FSpacetimeDBNetworkStats> FSpacetimeDBNetworkStatsTracker::Snapshot(uint64 NowUs)
{
	if (Samples.IsEmpty() || !AppliedOffset.IsSet())
	{
		return {};
	}
	Advance(NowUs);
	double ServerNowUs = double(NowUs) + AppliedOffset.GetValue();
	if (LastServerNowUs.IsSet())
	{
		ServerNowUs = FMath::Max(ServerNowUs, LastServerNowUs.GetValue());
	}
	LastServerNowUs = ServerNowUs;

	FSpacetimeDBNetworkStats Stats;
	Stats.Rtt = TimespanFromUs(Srtt);
	Stats.RttLatest = TimespanFromUs(LatestR);
	Stats.RttMin = TimespanFromUs(MinSample().R);
	Stats.Jitter = TimespanFromUs(RttVar);
	Stats.ServerNow = FSpacetimeDBTimestamp(static_cast<int64>(FMath::RoundToDouble(ServerNowUs)));
	Stats.ServerTimeLatest = FSpacetimeDBTimestamp(LatestServerReceiveUs);
	Stats.PongWait = TimespanFromUs(double(PongWaitUs(NowUs)));
	return Stats;
}

uint64 FSpacetimeDBNetworkStatsTracker::PongWaitUs(uint64 NowUs) const
{
	if (UnansweredSendTimesUs.IsEmpty())
	{
		return 0;
	}
	const uint64 Oldest = UnansweredSendTimesUs[0];
	return NowUs > Oldest ? NowUs - Oldest : 0;
}

void FSpacetimeDBNetworkStatsTracker::Reset()
{
	*this = FSpacetimeDBNetworkStatsTracker();
}

FSpacetimeDBNetworkStatsTracker::FSample FSpacetimeDBNetworkStatsTracker::MinSample() const
{
	check(!Samples.IsEmpty());
	FSample Min = Samples[0];
	for (const FSample& Sample : Samples)
	{
		if (Sample.R <= Min.R)
		{
			Min = Sample;
		}
	}
	return Min;
}

void FSpacetimeDBNetworkStatsTracker::Advance(uint64 NowUs)
{
	if (!AppliedOffset.IsSet())
	{
		return;
	}
	const double Now = double(NowUs);
	const double Step = MaxSlew * FMath::Max(Now - LastSlewAt, 0.0);
	const double Applied = AppliedOffset.GetValue();
	AppliedOffset = Applied + FMath::Clamp(Estimate - Applied, -Step, Step);
	LastSlewAt = FMath::Max(LastSlewAt, Now);
}
