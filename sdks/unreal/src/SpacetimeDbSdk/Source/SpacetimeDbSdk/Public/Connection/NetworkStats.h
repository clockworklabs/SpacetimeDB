#pragma once

#include "CoreMinimal.h"
#include "Misc/Optional.h"
#include "Types/Builtins.h"
#include "NetworkStats.generated.h"

/**
 * A snapshot of a connection's network statistics, derived from Ping/Pong round trips.
 *
 * Get one from UDbConnectionBase::TryGetNetworkStats.
 */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBNetworkStats
{
	GENERATED_BODY()

	/** Smoothed round-trip time (RFC 6298 SRTT). */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan Rtt;

	/** The most recent round-trip time sample. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan RttLatest;

	/** The lowest round-trip time over the recent sample window. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan RttMin;

	/** Round-trip time variation (RFC 6298 RTTVAR). */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan Jitter;

	/**
	 * The estimated current time on the server, on the same clock as a reducer's timestamp.
	 *
	 * Adjusts gradually and never goes backwards. This is the estimate at the moment the
	 * snapshot was taken; read the stats again for a fresh one.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FSpacetimeDBTimestamp ServerNow;

	/**
	 * When the server received the most recent Ping, as reported in its Pong.
	 *
	 * This is the raw measurement ServerNow is derived from: it doesn't advance between Pongs.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FSpacetimeDBTimestamp ServerTimeLatest;

	/**
	 * How long the oldest unanswered Ping has been waiting for its Pong, or zero when every
	 * Ping has been answered.
	 *
	 * On a healthy connection this stays below Rtt. The other fields only change when a Pong
	 * arrives, so they keep showing the last good values while the network is silently down;
	 * this is the field that keeps rising.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan PongWait;
};

/** The result of a single Ping sent with UDbConnectionBase::Ping. */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBPingResult
{
	GENERATED_BODY()

	/**
	 * The round-trip time for this Ping, excluding the time the server held it:
	 * RoundTrip - ServerHold, floored at zero. This is the sample that feeds the network stats.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan Rtt;

	/** The time from sending the Ping to receiving its Pong. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan RoundTrip;

	/** How long the server held the Ping before replying. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FTimespan ServerHold;

	/** When the server received the Ping, on the same clock as a reducer's timestamp. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FSpacetimeDBTimestamp ServerReceiveTime;

	/** When the Ping was sent, as an FPlatformTime::Seconds() reading. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	double SentAt = 0.0;

	/** When the Pong arrived, as an FPlatformTime::Seconds() reading. */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	double ReceivedAt = 0.0;
};

/**
 * Called when a Ping sent with UDbConnectionBase::Ping completes.
 *
 * On success, bSuccess is true and Result holds the timings. Otherwise Error says why: the
 * connection wasn't established when Ping was called, or it closed before the Pong arrived.
 */
DECLARE_DYNAMIC_DELEGATE_ThreeParams(FOnPingCompleteDelegate, bool, bSuccess, const FSpacetimeDBPingResult&, Result,
                                     const FString&, Error);

/**
 * Tracks round-trip time and server clock offset from Pong replies.
 *
 * Pure: every time value is supplied by the caller in microseconds, so it can be tested
 * deterministically. Monotonic times are on the client's monotonic clock, from an arbitrary
 * epoch; server times are microseconds since the Unix epoch.
 */
class SPACETIMEDBSDK_API FSpacetimeDBNetworkStatsTracker
{
public:
	/** How many recent samples feed RttMin and the clock offset estimate. */
	static constexpr int32 Window = 10;

	/** Record that a Ping went out, with SentUs as its client_send_time. */
	void RecordPing(uint64 SentUs);

	/**
	 * Record a Pong which arrived at monotonic time NowUs.
	 *
	 * ClientSendTimeUs is the Ping's send time echoed back by the server, ServerReceiveTimeUs
	 * when the server received the Ping, and ServerHoldUs how long the server held it.
	 */
	void RecordPong(uint64 NowUs, uint64 ClientSendTimeUs, int64 ServerReceiveTimeUs, int64 ServerHoldUs);

	/**
	 * A fresh snapshot of the statistics at monotonic time NowUs, or unset if no Pong has been
	 * recorded since construction or Reset.
	 */
	TOptional<FSpacetimeDBNetworkStats> Snapshot(uint64 NowUs);

	/**
	 * How long the oldest unanswered Ping has waited at monotonic time NowUs, in microseconds,
	 * or 0 if every Ping has been answered. Unlike Snapshot, this is available before the first Pong.
	 */
	uint64 PongWaitUs(uint64 NowUs) const;

	/** Forget all samples, unanswered Pings, the clock offset and the never-backwards clamp. */
	void Reset();

private:
	struct FSample
	{
		double R = 0.0;
		double Theta = 0.0;
	};

	/** The lowest-R sample in the window. Ties go to the most recent sample. */
	FSample MinSample() const;
	/** Move the applied offset towards the estimate, by a bounded fraction of the elapsed time. */
	void Advance(uint64 NowUs);

	double Srtt = 0.0;
	double RttVar = 0.0;
	double LatestR = 0.0;
	int64 LatestServerReceiveUs = 0;
	/** The most recent samples, oldest first, at most Window of them. */
	TArray<FSample> Samples;
	double Estimate = 0.0;
	/** Unset until the first Pong. */
	TOptional<double> AppliedOffset;
	double LastSlewAt = 0.0;
	/** The never-backwards clamp on ServerNow: unset until the first snapshot. */
	TOptional<double> LastServerNowUs;
	/** Send times of Pings with no Pong yet, oldest first. */
	TArray<uint64> UnansweredSendTimesUs;
};
