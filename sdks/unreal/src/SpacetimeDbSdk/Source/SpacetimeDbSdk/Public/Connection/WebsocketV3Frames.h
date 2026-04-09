#pragma once

#include "CoreMinimal.h"
#include "BSATN/UESpacetimeDB.h"

namespace UE::SpacetimeDB::V3
{

// v3 is only a transport envelope. The inner payloads are already-encoded v2
// websocket messages, so these helpers intentionally operate on raw bytes.
constexpr int32 MaxOutboundFrameBytes = 256 * 1024;

enum class EClientFrameTag : uint8
{
	Single = 0,
	Batch = 1,
};

struct FClientFrame
{
	EClientFrameTag Tag = EClientFrameTag::Single;
	TVariant<TArray<uint8>, TArray<TArray<uint8>>> FrameData;

	static FClientFrame Single(const TArray<uint8>& Value)
	{
		FClientFrame Frame;
		Frame.Tag = EClientFrameTag::Single;
		Frame.FrameData.Set<TArray<uint8>>(Value);
		return Frame;
	}

	static FClientFrame Batch(const TArray<TArray<uint8>>& Value)
	{
		FClientFrame Frame;
		Frame.Tag = EClientFrameTag::Batch;
		Frame.FrameData.Set<TArray<TArray<uint8>>>(Value);
		return Frame;
	}

	bool IsSingle() const
	{
		return Tag == EClientFrameTag::Single;
	}

	bool IsBatch() const
	{
		return Tag == EClientFrameTag::Batch;
	}

	TArray<uint8> GetAsSingle() const
	{
		check(IsSingle());
		return FrameData.Get<TArray<uint8>>();
	}

	TArray<TArray<uint8>> GetAsBatch() const
	{
		check(IsBatch());
		return FrameData.Get<TArray<TArray<uint8>>>();
	}

	bool operator==(const FClientFrame& Other) const
	{
		if (Tag != Other.Tag)
		{
			return false;
		}
		return IsSingle() ? GetAsSingle() == Other.GetAsSingle() : GetAsBatch() == Other.GetAsBatch();
	}

	bool operator!=(const FClientFrame& Other) const
	{
		return !(*this == Other);
	}
};

enum class EServerFrameTag : uint8
{
	Single = 0,
	Batch = 1,
};

struct FServerFrame
{
	EServerFrameTag Tag = EServerFrameTag::Single;
	TVariant<TArray<uint8>, TArray<TArray<uint8>>> FrameData;

	static FServerFrame Single(const TArray<uint8>& Value)
	{
		FServerFrame Frame;
		Frame.Tag = EServerFrameTag::Single;
		Frame.FrameData.Set<TArray<uint8>>(Value);
		return Frame;
	}

	static FServerFrame Batch(const TArray<TArray<uint8>>& Value)
	{
		FServerFrame Frame;
		Frame.Tag = EServerFrameTag::Batch;
		Frame.FrameData.Set<TArray<TArray<uint8>>>(Value);
		return Frame;
	}

	bool IsSingle() const
	{
		return Tag == EServerFrameTag::Single;
	}

	bool IsBatch() const
	{
		return Tag == EServerFrameTag::Batch;
	}

	TArray<uint8> GetAsSingle() const
	{
		check(IsSingle());
		return FrameData.Get<TArray<uint8>>();
	}

	TArray<TArray<uint8>> GetAsBatch() const
	{
		check(IsBatch());
		return FrameData.Get<TArray<TArray<uint8>>>();
	}

	bool operator==(const FServerFrame& Other) const
	{
		if (Tag != Other.Tag)
		{
			return false;
		}
		return IsSingle() ? GetAsSingle() == Other.GetAsSingle() : GetAsBatch() == Other.GetAsBatch();
	}

	bool operator!=(const FServerFrame& Other) const
	{
		return !(*this == Other);
	}
};

constexpr int32 BsatnEnumTagBytes = 1;
constexpr int32 BsatnLengthPrefixBytes = 4;

inline int32 EncodedSingleFrameSize(const TArray<uint8>& Message)
{
	return BsatnEnumTagBytes + BsatnLengthPrefixBytes + Message.Num();
}

inline int32 EncodedBatchFrameSizeForFirstMessage(const TArray<uint8>& Message)
{
	return BsatnEnumTagBytes + BsatnLengthPrefixBytes + BsatnLengthPrefixBytes + Message.Num();
}

inline int32 EncodedBatchElementSize(const TArray<uint8>& Message)
{
	return BsatnLengthPrefixBytes + Message.Num();
}

// Compute the largest prefix of already-encoded v2 client messages that fits in
// one v3 transport frame without trial-serializing candidate batches. The
// queue already stores encoded payload bytes, so a length-based fit check is
// enough here.
inline int32 CountClientMessagesForFrame(const TArray<TArray<uint8>>& Messages, int32 MaxFrameBytes)
{
	check(Messages.Num() > 0);

	const TArray<uint8>& FirstMessage = Messages[0];
	if (EncodedSingleFrameSize(FirstMessage) > MaxFrameBytes)
	{
		return 1;
	}

	int32 Count = 1;
	int32 BatchSize = EncodedBatchFrameSizeForFirstMessage(FirstMessage);
	while (Count < Messages.Num())
	{
		const TArray<uint8>& NextMessage = Messages[Count];
		const int32 NextBatchSize = BatchSize + EncodedBatchElementSize(NextMessage);
		if (NextBatchSize > MaxFrameBytes)
		{
			break;
		}
		BatchSize = NextBatchSize;
		++Count;
	}

	return Count;
}

inline TArray<uint8> EncodeClientMessages(const TArray<TArray<uint8>>& Messages)
{
	check(Messages.Num() > 0);
	return UE::SpacetimeDB::Serialize(
		Messages.Num() == 1 ? FClientFrame::Single(Messages[0]) : FClientFrame::Batch(Messages)
	);
}

inline TArray<uint8> EncodeServerMessages(const TArray<TArray<uint8>>& Messages)
{
	check(Messages.Num() > 0);
	return UE::SpacetimeDB::Serialize(
		Messages.Num() == 1 ? FServerFrame::Single(Messages[0]) : FServerFrame::Batch(Messages)
	);
}

inline void DecodeServerMessages(const TArray<uint8>& Data, TArray<TArray<uint8>>& OutMessages)
{
	const FServerFrame Frame = UE::SpacetimeDB::Deserialize<FServerFrame>(Data);
	if (Frame.IsSingle())
	{
		OutMessages.Reset(1);
		OutMessages.Add(Frame.GetAsSingle());
		return;
	}

	OutMessages = Frame.GetAsBatch();
}

} // namespace UE::SpacetimeDB::V3

namespace UE::SpacetimeDB
{

inline void serialize(UEWriter& Writer, const V3::FClientFrame& Value)
{
	Writer.write_u8(static_cast<uint8>(Value.Tag));
	switch (Value.Tag)
	{
	case V3::EClientFrameTag::Single:
		serialize(Writer, Value.FrameData.Get<TArray<uint8>>());
		break;
	case V3::EClientFrameTag::Batch:
		serialize(Writer, Value.FrameData.Get<TArray<TArray<uint8>>>());
		break;
	default:
		ensureMsgf(false, TEXT("Unknown v3 client-frame tag"));
		break;
	}
}

template<>
inline V3::FClientFrame deserialize<V3::FClientFrame>(UEReader& Reader)
{
	const V3::EClientFrameTag Tag = static_cast<V3::EClientFrameTag>(Reader.read_u8());
	switch (Tag)
	{
	case V3::EClientFrameTag::Single:
		return V3::FClientFrame::Single(Reader.read_array_u8());
	case V3::EClientFrameTag::Batch:
		return V3::FClientFrame::Batch(Reader.read_array<TArray<uint8>>());
	default:
		ensureMsgf(false, TEXT("Unknown v3 client-frame tag"));
		return V3::FClientFrame();
	}
}

inline void serialize(UEWriter& Writer, const V3::FServerFrame& Value)
{
	Writer.write_u8(static_cast<uint8>(Value.Tag));
	switch (Value.Tag)
	{
	case V3::EServerFrameTag::Single:
		serialize(Writer, Value.FrameData.Get<TArray<uint8>>());
		break;
	case V3::EServerFrameTag::Batch:
		serialize(Writer, Value.FrameData.Get<TArray<TArray<uint8>>>());
		break;
	default:
		ensureMsgf(false, TEXT("Unknown v3 server-frame tag"));
		break;
	}
}

template<>
inline V3::FServerFrame deserialize<V3::FServerFrame>(UEReader& Reader)
{
	const V3::EServerFrameTag Tag = static_cast<V3::EServerFrameTag>(Reader.read_u8());
	switch (Tag)
	{
	case V3::EServerFrameTag::Single:
		return V3::FServerFrame::Single(Reader.read_array_u8());
	case V3::EServerFrameTag::Batch:
		return V3::FServerFrame::Batch(Reader.read_array<TArray<uint8>>());
	default:
		ensureMsgf(false, TEXT("Unknown v3 server-frame tag"));
		return V3::FServerFrame();
	}
}

} // namespace UE::SpacetimeDB
