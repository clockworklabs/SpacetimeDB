#pragma once
#include "CoreMinimal.h"
#include "Misc/DateTime.h"
#include "Misc/Timespan.h"
#include "Misc/Parse.h"
#include "BSATN/UESpacetimeDB.h"
#include "LargeIntegers.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "Builtins.generated.h"



/**Compression algorithms supported by SpacetimeDB for data storage and transmission. */
UENUM(BlueprintType)
enum class ESpacetimeDBCompression : uint8
{
    None,
    Brotli,
    Gzip
};

/** 
 *  SpacetimeDB value types.
 */


 /**
  * 128-bit identifier used for active connections.
  * Internally uses a FSpacetimeDBUInt128 for the value.
  */
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBConnectionId
{
    GENERATED_BODY()

    /** The 128-bit value of the identifier. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite)
    FSpacetimeDBUInt128 Value;

    /** Default constructor initializes to zero. */
    FSpacetimeDBConnectionId() = default;

    /**
     * Construct from a 128-bit unsigned integer.
     * @param InValue The value to initialize with.
     */
    explicit FSpacetimeDBConnectionId(const FSpacetimeDBUInt128& InValue)
        : Value(InValue)
    {
    }

    /**
     * Compare two connection IDs for equality.
     * @param Other The other connection ID to compare against.
     * @return True if the values are equal.
     */
    bool operator==(const FSpacetimeDBConnectionId& Other) const
    {
        return Value == Other.Value;
    }

    /**
     * Compare two connection IDs for inequality.
     * @param Other The other connection ID to compare against.
     * @return True if the values are not equal.
     */
    bool operator!=(const FSpacetimeDBConnectionId& Other) const
    {
        return !(*this == Other);
    }

    /**
     * Compare two connection IDs for ordering.
     */
    bool operator<(const FSpacetimeDBConnectionId& Other) const
    {
        return Value < Other.Value;
    }

    bool operator>(const FSpacetimeDBConnectionId& Other) const
    {
        return Value > Other.Value;
    }

    bool operator<=(const FSpacetimeDBConnectionId& Other) const
    {
        return !(*this > Other);
    }

    bool operator>=(const FSpacetimeDBConnectionId& Other) const
    {
        return !(*this < Other);
    }

    /**
     * Construct from a little-endian byte array.
     * @param InBytes A 16-element byte array in little-endian order.
     * @return A new FSpacetimeDBConnectionId instance.
     */
    static FSpacetimeDBConnectionId FromLittleEndian(const TArray<uint8>& InBytes)
    {
        if (InBytes.Num() != 16)
        {
            return FSpacetimeDBConnectionId();
        }
        TArray<uint8> BigEndianBytes;
        BigEndianBytes.SetNumUninitialized(16);
        for (int32 i = 0; i < 16; ++i)
        {
            BigEndianBytes[i] = InBytes[15 - i];
        }
        return FSpacetimeDBConnectionId(FSpacetimeDBUInt128::FromBytesArray(BigEndianBytes));
    }

    /**
     * Construct from a big-endian byte array.
     * @param InBytes A 16-element byte array in big-endian order.
     * @return A new FSpacetimeDBConnectionId instance.
     */
    static FSpacetimeDBConnectionId FromBigEndian(const TArray<uint8>& InBytes)
    {
        if (InBytes.Num() != 16)
        {
            return FSpacetimeDBConnectionId();
        }
        return FSpacetimeDBConnectionId(FSpacetimeDBUInt128::FromBytesArray(InBytes));
    }

    /**
     * Construct from hex string (assumes big-endian).
     * @param Hex The hex string (e.g., "0x...").
     * @return A new FSpacetimeDBConnectionId instance.
     */
    static FSpacetimeDBConnectionId FromHex(const FString& Hex)
    {
        FString Clean = Hex.StartsWith(TEXT("0x")) ? Hex.Mid(2) : Hex;
        if (Clean.Len() != 32)
        {
            return FSpacetimeDBConnectionId();
        }
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(16);
        for (int32 i = 0; i < 16; ++i)
        {
            TCHAR High = Clean[i * 2];
            TCHAR Low = Clean[i * 2 + 1];
            uint8 ValueHigh = FParse::HexDigit(High);
            uint8 ValueLow = FParse::HexDigit(Low);
            Bytes[i] = (ValueHigh << 4) | ValueLow;
        }
        return FromBigEndian(Bytes);
    }

    /**
     * Convert to hex string.
     * @return The hex string representation of the value.
     */
    FString ToHex() const
    {
        return Value.ToHexString();
    }
};

/**
 * Get the hash of a Connection ID, required for use as a TMap key.
 * @param Id The Connection ID to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBConnectionId& Id)
{
    return HashCombine(GetTypeHash(Id.Value.GetUpper()), GetTypeHash(Id.Value.GetLower()));
}

namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBConnectionId);
    UE_SPACETIMEDB_STRUCT(FSpacetimeDBConnectionId, Value);
}



/**
 * 256-bit persistent identity for a user.
 * Internally uses a FSpacetimeDBUInt256 for the value.
 */
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBIdentity
{
    GENERATED_BODY()

    /** The 256-bit value of the identity. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite)
    FSpacetimeDBUInt256 Value;

    /** Default constructor initializes to zero. */
    FSpacetimeDBIdentity() = default;

    /**
     * Construct from a 256-bit unsigned integer.
     * @param InValue The value to initialize with.
     */
    explicit FSpacetimeDBIdentity(const FSpacetimeDBUInt256& InValue)
        : Value(InValue)
    {
    }

    /**
     * Compare two identities for equality. Required for TMap key.
     * @param Other The other identity to compare against.
     * @return True if the values are equal.
     */
    bool operator==(const FSpacetimeDBIdentity& Other) const
    {
        return Value == Other.Value;
    }

    /**
     * Compare two identities for inequality.
     * @param Other The other identity to compare against.
     * @return True if the values are not equal.
     */
    bool operator!=(const FSpacetimeDBIdentity& Other) const
    {
        return !(*this == Other);
    }

    /**
     * Compare two identities for ordering. Required for certain TMap/TSet internal operations.
     * @param Other The other identity to compare against.
     * @return True if this identity is less than the other.
     */
    bool operator<(const FSpacetimeDBIdentity& Other) const 
    {
        return Value < Other.Value;
    }

    /**
     * Compare two identities for ordering. Required for certain TMap/TSet internal operations.
     * @param Other The other identity to compare against.
     * @return True if this identity is less than the other.
     */
    bool operator>(const FSpacetimeDBIdentity& Other) const 
    {
        return Value > Other.Value;
    }

    bool operator<=(const FSpacetimeDBIdentity& Other) const
    {
        return !(*this > Other);
    }

    bool operator>=(const FSpacetimeDBIdentity& Other) const
    {
        return !(*this < Other);
    }


    /**
     * Construct from a little-endian byte array.
     * @param InBytes A 32-element byte array in little-endian order.
     * @return A new FSpacetimeDBIdentity instance.
     */
    static FSpacetimeDBIdentity FromLittleEndian(const TArray<uint8>& InBytes)
    {
        if (InBytes.Num() != 32)
        {
            return FSpacetimeDBIdentity();
        }
        TArray<uint8> BigEndianBytes;
        BigEndianBytes.SetNumUninitialized(32);
        for (int32 i = 0; i < 32; ++i)
        {
            BigEndianBytes[i] = InBytes[31 - i];
        }
        return FSpacetimeDBIdentity(FSpacetimeDBUInt256::FromBytesArray(BigEndianBytes));
    }

    /**
     * Construct from a big-endian byte array.
     * @param InBytes A 32-element byte array in big-endian order.
     * @return A new FSpacetimeDBIdentity instance.
     */
    static FSpacetimeDBIdentity FromBigEndian(const TArray<uint8>& InBytes)
    {
        if (InBytes.Num() != 32)
        {
            return FSpacetimeDBIdentity();
        }
        return FSpacetimeDBIdentity(FSpacetimeDBUInt256::FromBytesArray(InBytes));
    }

    /**
     * Construct from hex string (assumes big-endian).
     * @param Hex The hex string (e.g., "0x...").
     * @return A new FSpacetimeDBIdentity instance.
     */
    static FSpacetimeDBIdentity FromHex(const FString& Hex)
    {
        FString Clean = Hex.StartsWith(TEXT("0x")) ? Hex.Mid(2) : Hex;
        if (Clean.Len() != 64)
        {
            return FSpacetimeDBIdentity();
        }
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(32);
        for (int32 i = 0; i < 32; ++i)
        {
            TCHAR High = Clean[i * 2];
            TCHAR Low = Clean[i * 2 + 1];
            uint8 ValueHigh = FParse::HexDigit(High);
            uint8 ValueLow = FParse::HexDigit(Low);
            Bytes[i] = (ValueHigh << 4) | ValueLow;
        }
        return FromBigEndian(Bytes);
    }

    /**
     * Convert to hex string.
     * @return The hex string representation of the value.
     */
    FString ToHex() const
    {
        return Value.ToHexString();
    }
};

/**
 * Get the hash of an Identity, required for use as a TMap key.
 * @param Identity The identity to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBIdentity& Identity)
{
    // Hash the upper and lower 128-bit parts of the 256-bit integer.
    return HashCombine(GetTypeHash(Identity.Value.GetUpper()), GetTypeHash(Identity.Value.GetLower()));
}

namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBIdentity)
    UE_SPACETIMEDB_ENABLE_TOPTIONAL(FSpacetimeDBIdentity)

    UE_SPACETIMEDB_STRUCT(FSpacetimeDBIdentity, Value);
}

/**
* Represents a point in time as microseconds since the Unix epoch (1970-01-01 00:00:00 UTC).
* This corresponds to SpacetimeDB's Timestamp type.
*/
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBTimestamp
{
    GENERATED_BODY()

public:
    /** Microseconds since the Unix epoch (1970-01-01 00:00:00 UTC). */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
    int64 MicrosecondsSinceEpoch = 0;

public:
    /** Default constructor. */
    FSpacetimeDBTimestamp() = default;

    /**
    * Constructor from microseconds.
    * @param InMicroseconds The number of microseconds since the Unix epoch.
    */
    explicit FSpacetimeDBTimestamp(int64 InMicroseconds)
        : MicrosecondsSinceEpoch(InMicroseconds)
    {
    }

    /**
    * Creates a timestamp from a native FDateTime.
    * @param DateTime The FDateTime to convert.
    * @return A new FSpacetimeDBTimestamp instance.
    */
    static FSpacetimeDBTimestamp FromFDateTime(const FDateTime& DateTime)
    {
        constexpr int64 UnixEpochTicks = 621355968000000000LL;
        const int64 Ticks = DateTime.GetTicks();
        if (Ticks < UnixEpochTicks)
        {
            return FSpacetimeDBTimestamp(0);
        }
        return FSpacetimeDBTimestamp((Ticks - UnixEpochTicks) / ETimespan::TicksPerMicrosecond);
    }

    /**
    * Converts this timestamp to a native FDateTime.
    * @return The FDateTime representation.
    */
    FDateTime ToFDateTime() const
    {
        constexpr int64 UnixEpochTicks = 621355968000000000LL;
        int64 Ticks = UnixEpochTicks + MicrosecondsSinceEpoch * ETimespan::TicksPerMicrosecond;
        return FDateTime(Ticks);
    }

    /** Gets the raw microsecond value. */
    int64 GetMicroseconds() const { return MicrosecondsSinceEpoch; }

    /**
    * Converts the timestamp to a string in the ISO 8601 format: YYYY-MM-DDTHH:MM:SS.ffffffZ
    */
    FString ToString() const
    {
        const FDateTime AsDateTime = ToFDateTime();
        const int32 Microseconds = (AsDateTime.GetTicks() % ETimespan::TicksPerSecond) / ETimespan::TicksPerMicrosecond;
        return FString::Printf(TEXT("%04d-%02d-%02dT%02d:%02d:%02d.%06dZ"),
            AsDateTime.GetYear(), AsDateTime.GetMonth(), AsDateTime.GetDay(),
            AsDateTime.GetHour(), AsDateTime.GetMinute(), AsDateTime.GetSecond(),
            Microseconds
        );
    }

    /** Comparison operators */
    bool operator==(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch == Other.MicrosecondsSinceEpoch; }
    bool operator!=(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch != Other.MicrosecondsSinceEpoch; }
    bool operator<(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch < Other.MicrosecondsSinceEpoch; }
    bool operator<=(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch <= Other.MicrosecondsSinceEpoch; }
    bool operator>(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch > Other.MicrosecondsSinceEpoch; }
    bool operator>=(const FSpacetimeDBTimestamp& Other) const { return MicrosecondsSinceEpoch >= Other.MicrosecondsSinceEpoch; }

    /** Arithmetic operators */
    FSpacetimeDBTimestamp operator+(const FSpacetimeDBTimeDuration& Duration) const;
    FSpacetimeDBTimestamp operator-(const FSpacetimeDBTimeDuration& Duration) const;
    FSpacetimeDBTimeDuration operator-(const FSpacetimeDBTimestamp& Other) const;
};

FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBTimestamp& Timestamp)
{
    // Directly hash the int64 value. Unreal's GetTypeHash for int64 will handle this.
    return GetTypeHash(Timestamp.MicrosecondsSinceEpoch);
}

namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBTimestamp)
    UE_SPACETIMEDB_ENABLE_TOPTIONAL(FSpacetimeDBTimestamp)


    UE_SPACETIMEDB_STRUCT(FSpacetimeDBTimestamp, MicrosecondsSinceEpoch);
}

/**
* Represents a duration of time with microsecond precision.
* This corresponds to SpacetimeDB's TimeDuration type.
*/
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBTimeDuration
{
    GENERATED_BODY()

public:
    /** Total duration in microseconds. */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
    int64 TotalMicroseconds = 0;

public:
    /** Default constructor. */
    FSpacetimeDBTimeDuration() = default;

    /**
    * Constructor from microseconds.
    * @param InMicroseconds The total number of microseconds for this duration.
    */
    explicit FSpacetimeDBTimeDuration(int64 InMicroseconds)
        : TotalMicroseconds(InMicroseconds)
    {
    }

    /**
    * Creates a duration from a native FTimespan.
    * @param Timespan The FTimespan to convert.
    * @return A new FSpacetimeDBTimeDuration instance.
    */
    static FSpacetimeDBTimeDuration FromFTimespan(const FTimespan& Timespan)
    {
        return FSpacetimeDBTimeDuration(Timespan.GetTotalMicroseconds());
    }

    /**
    * Converts this duration to a native FTimespan.
    * @return The FTimespan representation.
    */
    FTimespan ToFTimespan() const
    {
        return FTimespan::FromMicroseconds(TotalMicroseconds);
    }

    /** Gets the raw microsecond value. */
    int64 GetMicroseconds() const { return TotalMicroseconds; }

    /**
    * Converts the duration to a string in the format: [-]d.hh:mm:ss.ffffff
    */
    FString ToString() const
    {
        const FTimespan AsTimespan = ToFTimespan();
        const bool bIsNegative = AsTimespan.GetTicks() < 0;
        const FTimespan AbsoluteTimespan(FMath::Abs(AsTimespan.GetTicks()));
        const FString FormattedString = FString::Printf(TEXT("%d.%02d:%02d:%02d.%06d"),
            AbsoluteTimespan.GetDays(),
            AbsoluteTimespan.GetHours(),
            AbsoluteTimespan.GetMinutes(),
            AbsoluteTimespan.GetSeconds(),
            AbsoluteTimespan.GetFractionMicro()
        );
        return bIsNegative ? TEXT("-") + FormattedString : FormattedString;
    }

    /** Comparison operators */
    bool operator==(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds == Other.TotalMicroseconds; }
    bool operator!=(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds != Other.TotalMicroseconds; }
    bool operator<(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds < Other.TotalMicroseconds; }
    bool operator<=(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds <= Other.TotalMicroseconds; }
    bool operator>(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds > Other.TotalMicroseconds; }
    bool operator>=(const FSpacetimeDBTimeDuration& Other) const { return TotalMicroseconds >= Other.TotalMicroseconds; }

    /** Arithmetic operators */
    FSpacetimeDBTimeDuration operator+(const FSpacetimeDBTimeDuration& Other) const { return FSpacetimeDBTimeDuration(TotalMicroseconds + Other.TotalMicroseconds); }
    FSpacetimeDBTimeDuration operator-(const FSpacetimeDBTimeDuration& Other) const { return FSpacetimeDBTimeDuration(TotalMicroseconds - Other.TotalMicroseconds); }
};

 //--- GetTypeHash for FSpacetimeDBTimeDuration ---
 //This should be placed in a .h file where FSpacetimeDBTimeDuration is defined.
 //It must be in the global namespace or a namespace where ADL can find it.
inline uint32 GetTypeHash(const FSpacetimeDBTimeDuration& Thing)
{
    // Since FSpacetimeDBTimeDuration is just a wrapper around int64,
    // we can simply use the hash of its TotalMicroseconds member.
    // GetTypeHash is already defined for int64.
    return GetTypeHash(Thing.TotalMicroseconds);
}


namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBTimeDuration)
    UE_SPACETIMEDB_ENABLE_TOPTIONAL(FSpacetimeDBTimeDuration)

    UE_SPACETIMEDB_STRUCT(FSpacetimeDBTimeDuration, TotalMicroseconds);
}


UENUM(BlueprintType, Category = "SpacetimeDB")
enum class EScheduleAtTag : uint8
{
    Interval,
    Time,
};

// New: value type with TVariant payload
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBScheduleAt
{
    GENERATED_BODY()

public:
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
    EScheduleAtTag Tag = EScheduleAtTag::Interval;

    // Payload
    TVariant<FSpacetimeDBTimeDuration, FSpacetimeDBTimestamp> Data;

    // ---- C++ convenience constructors (mirroring old static makers) ----
    static FSpacetimeDBScheduleAt Interval(const FSpacetimeDBTimeDuration& Value)
    {
        FSpacetimeDBScheduleAt Out;
        Out.Tag = EScheduleAtTag::Interval;
        Out.Data.Set<FSpacetimeDBTimeDuration>(Value);
        return Out;
    }
    static FSpacetimeDBScheduleAt Time(const FSpacetimeDBTimestamp& Value)
    {
        FSpacetimeDBScheduleAt Out;
        Out.Tag = EScheduleAtTag::Time;
        Out.Data.Set<FSpacetimeDBTimestamp>(Value);
        return Out;
    }

    // ---- Query helpers (C++) ----
    FORCEINLINE bool IsInterval() const { return Tag == EScheduleAtTag::Interval; }
    FORCEINLINE bool IsTime()     const { return Tag == EScheduleAtTag::Time; }

    FORCEINLINE FSpacetimeDBTimeDuration GetAsInterval() const
    {
        ensureMsgf(IsInterval(), TEXT("MessageData does not hold Interval!"));
        return Data.Get<FSpacetimeDBTimeDuration>();
    }
    FORCEINLINE FSpacetimeDBTimestamp GetAsTime() const
    {
        ensureMsgf(IsTime(), TEXT("MessageData does not hold Time!"));
        return Data.Get<FSpacetimeDBTimestamp>();
    }

    // Equality
    FORCEINLINE bool operator==(const FSpacetimeDBScheduleAt& Other) const
    {
        if (Tag != Other.Tag) return false;
        switch (Tag)
        {
        case EScheduleAtTag::Interval:
            return GetAsInterval() == Other.GetAsInterval();
        case EScheduleAtTag::Time:
            return GetAsTime() == Other.GetAsTime();
        default: return false;
        }
    }
    FORCEINLINE bool operator!=(const FSpacetimeDBScheduleAt& Other) const { return !(*this == Other); }
};

FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBScheduleAt& ScheduleAt)
{
    const uint32 TagHash = ::GetTypeHash(static_cast<uint8>(ScheduleAt.Tag));
    switch (ScheduleAt.Tag)
    {
    case EScheduleAtTag::Interval:
        return HashCombine(TagHash, ::GetTypeHash(ScheduleAt.GetAsInterval()));
    case EScheduleAtTag::Time:
        return HashCombine(TagHash, ::GetTypeHash(ScheduleAt.GetAsTime()));
    default: return TagHash;
    }
}

namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBScheduleAt);

    UE_SPACETIMEDB_TAGGED_ENUM(
        FSpacetimeDBScheduleAt,
        EScheduleAtTag,
        Data,
        // Tag        // TVariant alternative type
        Interval, FSpacetimeDBTimeDuration,
        Time, FSpacetimeDBTimestamp
    );
}

// ---- Blueprint Function Library ----
UCLASS()
class SPACETIMEDBSDK_API USpacetimeDBScheduleAtBpLib : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|ScheduleAt")
    static FSpacetimeDBScheduleAt Interval(const FSpacetimeDBTimeDuration& Interval)
    {
        return FSpacetimeDBScheduleAt::Interval(Interval);
    }

    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|ScheduleAt")
    static FSpacetimeDBScheduleAt Time(const FSpacetimeDBTimestamp& Timestamp)
    {
        return FSpacetimeDBScheduleAt::Time(Timestamp);
    }

    // Predicates
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ScheduleAt")
    static bool IsInterval(const FSpacetimeDBScheduleAt& InValue) { return InValue.IsInterval(); }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ScheduleAt")
    static bool IsTime(const FSpacetimeDBScheduleAt& InValue) { return InValue.IsTime(); }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ScheduleAt")
    static FSpacetimeDBTimeDuration GetAsInterval(const FSpacetimeDBScheduleAt& InValue)
    {
        return InValue.GetAsInterval();
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ScheduleAt")
    static FSpacetimeDBTimestamp GetTime(const FSpacetimeDBScheduleAt& InValue)
    {
        return InValue.GetAsTime();
    }
};

/**
* Blueprint helpers that turn SpacetimeDB value types into strings
*/
UCLASS()
class SPACETIMEDBSDK_API USpacetimeDBBuiltinLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:

    /* ───────── 128-bit ConnectionId → FString ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Conversion",
        meta = (DisplayName = "To String (ConnectionId)",
            CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_ConnectionIdToString(const FSpacetimeDBConnectionId& InValue)
    {
        return InValue.ToHex();
    }

    /* ───────── 256-bit Identity → FString ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Conversion",
        meta = (DisplayName = "To String (Identity)",
            CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_IdentityToString(const FSpacetimeDBIdentity& InValue)
    {
        return InValue.ToHex();
    }

    /* ───────── Timestamp → FString ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Conversion",
        meta = (DisplayName = "To String (Timestamp)",
            CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_TimestampToString(const FSpacetimeDBTimestamp& InValue)
    {
        return InValue.ToString();
    }

    /* ───────── TimeDuration → FString ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Conversion",
        meta = (DisplayName = "To String (TimeDuration)",
            CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_TimeDurationToString(const FSpacetimeDBTimeDuration& InValue)
    {
        return InValue.ToString();
    }

    /* ───────── ScheduleAt (variant UObject) → FString ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Conversion",
        meta = (DisplayName = "To String (ScheduleAt)",
            CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_ScheduleAtToString(const FSpacetimeDBScheduleAt& InValue)
    {
        switch (InValue.Tag)
        {
        case EScheduleAtTag::Interval:
        {
            const FSpacetimeDBTimeDuration Duration = InValue.GetAsInterval();
            return Duration.ToString();
        }
        case EScheduleAtTag::Time:
        {
            bool bValid = false;
            const FSpacetimeDBTimestamp Time = InValue.GetAsTime();
            return Time.ToString();
        }
        default:
            return TEXT("<invalid-tag>");
        }
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (ConnectionId)",
            CompactNodeTitle = "==",
            Keywords = "== equals equal"))
    static bool Equal_ConnectionId(const FSpacetimeDBConnectionId& A,
        const FSpacetimeDBConnectionId& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (ConnectionId)",
            CompactNodeTitle = "!=",
            Keywords = "!= notequal"))
    static bool NotEqual_ConnectionId(const FSpacetimeDBConnectionId& A,
        const FSpacetimeDBConnectionId& B)
    {
        return A != B;
    }

    /* ───────── Identity compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (Identity)",
            CompactNodeTitle = "==",
            Keywords = "== equals equal"))
    static bool Equal_Identity(const FSpacetimeDBIdentity& A,
        const FSpacetimeDBIdentity& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (Identity)",
            CompactNodeTitle = "!=",
            Keywords = "!= notequal"))
    static bool NotEqual_Identity(const FSpacetimeDBIdentity& A,
        const FSpacetimeDBIdentity& B)
    {
        return A != B;
    }

    /* ───────── Timestamp compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (Timestamp)",
            CompactNodeTitle = "==",
            Keywords = "== equals equal"))
    static bool Equal_Timestamp(const FSpacetimeDBTimestamp& A,
        const FSpacetimeDBTimestamp& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (Timestamp)",
            CompactNodeTitle = "!=",
            Keywords = "!= notequal"))
    static bool NotEqual_Timestamp(const FSpacetimeDBTimestamp& A,
        const FSpacetimeDBTimestamp& B)
    {
        return A != B;
    }

    /* ───────── TimeDuration compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (TimeDuration)",
            CompactNodeTitle = "==",
            Keywords = "== equals equal"))
    static bool Equal_TimeDuration(const FSpacetimeDBTimeDuration& A,
        const FSpacetimeDBTimeDuration& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (TimeDuration)",
            CompactNodeTitle = "!=",
            Keywords = "!= notequal"))
    static bool NotEqual_TimeDuration(const FSpacetimeDBTimeDuration& A,
        const FSpacetimeDBTimeDuration& B)
    {
        return A != B;
    }

    /* ───────── ScheduleAt compare (UObject ptr) ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (ScheduleAt)",
            CompactNodeTitle = "==",
            Keywords = "== equals equal",
            DeterminesOutputType = "A"))
    static bool Equal_ScheduleAt(const FSpacetimeDBScheduleAt& A,
        const FSpacetimeDBScheduleAt& B)
    {
        return A == B; // deep compare
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (ScheduleAt)",
            CompactNodeTitle = "!=",
            Keywords = "!= notequal",
            DeterminesOutputType = "A"))
    static bool NotEqual_ScheduleAt(const FSpacetimeDBScheduleAt& A,
        const FSpacetimeDBScheduleAt& B)
    {
        return !Equal_ScheduleAt(A, B);
    }
};