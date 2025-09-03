#pragma once
#include "CoreMinimal.h"
#include "Algo/Reverse.h"
#include "LargeIntegers.generated.h"


#define UINT64_HEX TEXT("%016llx") // one 64-bit chunk → 16 hex chars, zero-padded
#define INT64_HEX TEXT("%016llx")

// Reverse an FString in-place (works with UE 5.x)
static void ReverseStringInline(FString& Str)
{
    const int32 Len = Str.Len();
    if (Len <= 1)
    {
        return;
    }

    for (int32 I = 0, J = Len - 1; I < J; ++I, --J)
    {
        const TCHAR Tmp = Str[I];
        Str[I] = Str[J];
        Str[J] = Tmp;
    }
}

// Big-endian bytes -> decimal string (base-10) via repeated division by 10.
static FString BigEndianBytesToDecimalString(const TArray<uint8>& InBigEndian)
{
    // Trim leading zeros
    int32 FirstNZ = 0;
    while (FirstNZ < InBigEndian.Num() && InBigEndian[FirstNZ] == 0)
    {
        ++FirstNZ;
    }
    if (FirstNZ == InBigEndian.Num())
    {
        return TEXT("0");
    }

    // Working buffer (big-endian, no leading zeros)
    TArray<uint8> Work;
    Work.Append(InBigEndian.GetData() + FirstNZ, InBigEndian.Num() - FirstNZ);

    FString Digits;
    Digits.Reserve(Work.Num() * 3); // heuristic

    // Long division in base 256, collecting base-10 remainders
    while (Work.Num() > 0)
    {
        uint32 Remainder = 0;

        TArray<uint8> Quot;
        Quot.SetNumUninitialized(Work.Num());

        for (int32 i = 0; i < Work.Num(); ++i)
        {
            const uint32 Acc = (Remainder << 8) | Work[i]; // base-256 step
            const uint8 Q = static_cast<uint8>(Acc / 10);
            Remainder = Acc - static_cast<uint32>(Q) * 10;
            Quot[i] = Q;
        }

        // Next least-significant decimal digit
        Digits.AppendChar(static_cast<TCHAR>('0' + static_cast<TCHAR>(Remainder)));

        // Trim leading zeros of quotient
        int32 QFirst = 0;
        while (QFirst < Quot.Num() && Quot[QFirst] == 0)
        {
            ++QFirst;
        }

        if (QFirst == Quot.Num())
        {
            Work.Reset();
        }
        else
        {
            TArray<uint8> Next;
            Next.Append(Quot.GetData() + QFirst, Quot.Num() - QFirst);
            Work = MoveTemp(Next);
        }
    }

    // We collected digits least→most significant, so reverse
    ReverseStringInline(Digits);
    return Digits;
}

static void TwosComplementNegateBigEndian(TArray<uint8>& Bytes)
{
    // In-place: value = (~value) + 1  (big-endian)
    for (uint8& B : Bytes) { B = ~B; }
    // add 1 from the least significant end (last byte)
    int32 i = Bytes.Num() - 1;
    while (i >= 0)
    {
        uint16 Sum = static_cast<uint16>(Bytes[i]) + 1u;
        Bytes[i] = static_cast<uint8>(Sum & 0xFFu);
        if ((Sum & 0x100u) == 0) { break; } // no carry -> done
        --i;
    }
}


/**
 * Unsigned 128-bit integer (Upper:High 64 bits, Lower:Low 64 bits).
 * Designed for fast value-type use in C++ and Blueprint.
 */
USTRUCT(BlueprintType)
struct FSpacetimeDBUInt128
{
    GENERATED_BODY()

private:
    /** Low  64 bits (little-endian layout) */
    UPROPERTY()
    uint64 Lower = 0;

    /** High 64 bits */
    UPROPERTY()
    uint64 Upper = 0;

public:
    /** Default-zero constructor (required for Blueprint) */
    FSpacetimeDBUInt128() = default;

    /** Construct from two 64-bit parts */
    FSpacetimeDBUInt128(uint64 InUpper, uint64 InLower)
        : Lower(InLower)
        , Upper(InUpper)
    {
    }

    /** High 64 bits */
    FORCEINLINE uint64 GetUpper() const
    {
        return Upper;
    }

    /** Low 64 bits */
    FORCEINLINE uint64 GetLower() const
    {
        return Lower;
    }

    /** Comparison operators */
    FORCEINLINE bool operator<(const FSpacetimeDBUInt128& Other) const
    {
        return (Upper < Other.Upper) || (Upper == Other.Upper && Lower < Other.Lower);
    }

    FORCEINLINE bool operator>(const FSpacetimeDBUInt128& Other) const
    {
        return (Upper > Other.Upper) || (Upper == Other.Upper && Lower > Other.Lower);
    }

    FORCEINLINE bool operator<=(const FSpacetimeDBUInt128& Other) const
    {
        return !(*this > Other);
    }

    FORCEINLINE bool operator>=(const FSpacetimeDBUInt128& Other) const
    {
        return !(*this < Other);
    }

    FORCEINLINE bool operator==(const FSpacetimeDBUInt128& Other) const
    {
        return Upper == Other.Upper && Lower == Other.Lower;
    }

    FORCEINLINE bool operator!=(const FSpacetimeDBUInt128& Other) const
    {
        return !(*this == Other);
    }


    /** Decimal via simple hex fallback (cheap & predictable). */
    FString ToHexString() const
    {
        // Concatenate the two halves; Upper first for natural reading.
        return FString::Printf(TEXT("0x") UINT64_HEX UINT64_HEX, Upper, Lower);
    }

    /** Hex string “0xHHHH…LLLL” (always 32 hex digits) */
    FString ToString() const { return ToHexString(); }

    /** 16-byte BE array: Upper(63‒0) | Lower(63‒0) */
    TArray<uint8> ToBytesArray() const
    {
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(16);

        for (int32 Index = 0; Index < 8; ++Index)
        {
            Bytes[Index] = static_cast<uint8>((Upper >> ((7 - Index) * 8)) & 0xFF);
            Bytes[8 + Index] = static_cast<uint8>((Lower >> ((7 - Index) * 8)) & 0xFF);
        }
        return Bytes;
    }

    static FSpacetimeDBUInt128 FromBytesArray(const TArray<uint8>& Bytes)
    {
        check(Bytes.Num() == 16);

        uint64 Upper = 0;
        uint64 Lower = 0;

        for (int32 Index = 0; Index < 8; ++Index)
        {
            Upper = (Upper << 8) | Bytes[Index];
            Lower = (Lower << 8) | Bytes[8 + Index];
        }

        return FSpacetimeDBUInt128(Upper, Lower);
    }

    FString ToDecimalString() const
    {
        const TArray<uint8> Bytes = ToBytesArray();
        return BigEndianBytesToDecimalString(Bytes);
    }
};

/**
 * Get the hash of a 128-bit unsigned integer.
 * @param Val The value to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBUInt128& Val)
{
    return HashCombine(GetTypeHash(Val.GetUpper()), GetTypeHash(Val.GetLower()));
}

/**
 * Signed 128-bit integer (two’s-complement).
 * Upper = high 64 bits, Lower = low 64 bits.
 * Fields are *not* UPROPERTY because Blueprint reflection only supports
 * uint8 / int32 / float, not 64-bit ints. :contentReference[oaicite:0]{index=0}
 */
USTRUCT(BlueprintType)
struct FSpacetimeDBInt128
{
    GENERATED_BODY()

private:
    UPROPERTY()
    uint64 Lower = 0;
    UPROPERTY()
    uint64 Upper = 0;

public:
    FSpacetimeDBInt128() = default;

    /** Build from the two 64-bit halves. */
    FSpacetimeDBInt128(uint64 InUpper, uint64 InLower)
        : Lower(InLower), Upper(InUpper)
    {
    }

    /** Sign test (two’s-complement). */
    FORCEINLINE bool IsNegative() const noexcept
    {
        // Bit 127 (sign-bit) lives in the high 64-bits -> index 63.
        return (Upper >> 63) != 0;
    }

    FORCEINLINE uint64 GetUpper() const { return Upper; }
    FORCEINLINE uint64 GetLower() const { return Lower; }

    /* ---------- comparison ---------- */
    FORCEINLINE bool operator<(const FSpacetimeDBInt128& Rhs) const
    {
        const bool bNegA = IsNegative();
        const bool bNegB = Rhs.IsNegative();
        if (bNegA != bNegB)
        {
            return bNegA; // negative < positive
        }
        return (Upper < Rhs.Upper) || (Upper == Rhs.Upper && Lower < Rhs.Lower);
    }

    FORCEINLINE bool operator>(const FSpacetimeDBInt128& Rhs) const
    {
        const bool bNegA = IsNegative();
        const bool bNegB = Rhs.IsNegative();
        if (bNegA != bNegB)
        {
            return bNegB; // positive > negative
        }
        return (Upper > Rhs.Upper) || (Upper == Rhs.Upper && Lower > Rhs.Lower);
    }

    FORCEINLINE bool operator<=(const FSpacetimeDBInt128& Rhs) const
    {
        return !(*this > Rhs);
    }

    FORCEINLINE bool operator>=(const FSpacetimeDBInt128& Rhs) const
    {
        return !(*this < Rhs);
    }

    FORCEINLINE bool operator==(const FSpacetimeDBInt128& Rhs) const
    {
        return Upper == Rhs.Upper && Lower == Rhs.Lower;
    }
    FORCEINLINE bool operator!=(const FSpacetimeDBInt128& Rhs) const
    {
        return !(*this == Rhs);
    }

    /** Hex form ― 32 digits, two’s-complement. */
    FString ToHexString() const
    {
        return FString::Printf(TEXT("0x") INT64_HEX INT64_HEX, Upper, Lower);
    }

    /** Decimal via simple hex fallback (cheap & predictable). */
    FString ToString() const { return ToHexString(); }

    TArray<uint8> ToBytesArray() const
    {
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(16);

        for (int32 Index = 0; Index < 8; ++Index)
        {
            Bytes[Index] = static_cast<uint8>((Upper >> ((7 - Index) * 8)) & 0xFF);
            Bytes[8 + Index] = static_cast<uint8>((Lower >> ((7 - Index) * 8)) & 0xFF);
        }
        return Bytes;
    }

    static FSpacetimeDBInt128 FromBytesArray(const TArray<uint8>& Bytes)
    {
        check(Bytes.Num() == 16);

        uint64 Upper = 0;
        uint64 Lower = 0;

        for (int32 Index = 0; Index < 8; ++Index)
        {
            Upper = (Upper << 8) | Bytes[Index];
            Lower = (Lower << 8) | Bytes[8 + Index];
        }

        return FSpacetimeDBInt128(Upper, Lower);
    }

    FString ToDecimalString() const
    {
        // Get big-endian bytes: Upper | Lower (already provided by your struct) 
        TArray<uint8> Bytes = ToBytesArray();
        const bool bNegative = IsNegative();

        if (bNegative)
        {
            // Convert magnitude = two's-complement negate (on a copy)
            TArray<uint8> Mag = Bytes;
            TwosComplementNegateBigEndian(Mag);
            FString Dec = BigEndianBytesToDecimalString(Mag);
            return FString::Printf(TEXT("-%s"), *Dec);
        }
        else
        {
            return BigEndianBytesToDecimalString(Bytes);
        }
    }
};

/**
 * Get the hash of a 128-bit signed integer.
 * @param Val The value to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBInt128& Val)
{
    return HashCombine(GetTypeHash(Val.GetUpper()), GetTypeHash(Val.GetLower()));
}



/**
 * Unsigned 256-bit integer (Upper = high 128 bits, Lower = low 128 bits).
 * Internal data stay private (Blueprint can’t expose uint64 directly) :contentReference[oaicite:0]{index=0}
 */
USTRUCT(BlueprintType)
struct FSpacetimeDBUInt256
{
    GENERATED_BODY()

private:
    UPROPERTY() FSpacetimeDBUInt128 Lower;   // bits 0–127  (least significant)
    UPROPERTY() FSpacetimeDBUInt128 Upper;   // bits 128–255 (most significant)

public:
    /** Default-zero - required for BP */
    FSpacetimeDBUInt256() = default;

    /** Construct from two 128-bit halves (Upper: high, Lower: low) */
    FSpacetimeDBUInt256(const FSpacetimeDBUInt128& InUpper, const FSpacetimeDBUInt128& InLower)
        : Lower(InLower), Upper(InUpper)
    {
    }

    /* ---------- accessors ---------- */
    FORCEINLINE const FSpacetimeDBUInt128& GetUpper() const { return Upper; }
    FORCEINLINE const FSpacetimeDBUInt128& GetLower() const { return Lower; }

    /* ---------- comparisons (unsigned lexicographic Upper→Lower) ---------- */
    FORCEINLINE bool operator<(const FSpacetimeDBUInt256& Rhs) const
    {
        return (Upper < Rhs.Upper) || (Upper == Rhs.Upper && Lower < Rhs.Lower);
    }

    FORCEINLINE bool operator>(const FSpacetimeDBUInt256& Rhs) const
    {
        return (Upper > Rhs.Upper) || (Upper == Rhs.Upper && Lower > Rhs.Lower);
    }

    FORCEINLINE bool operator<=(const FSpacetimeDBUInt256& Rhs) const { return !(*this > Rhs); }
    FORCEINLINE bool operator>=(const FSpacetimeDBUInt256& Rhs) const { return !(*this < Rhs); }

    FORCEINLINE bool operator==(const FSpacetimeDBUInt256& Rhs) const
    {
        return Upper == Rhs.Upper && Lower == Rhs.Lower;
    }

    FORCEINLINE bool operator!=(const FSpacetimeDBUInt256& Rhs) const { return !(*this == Rhs); }

    /** Fixed-width hex string “0x[64 hex digits]” */
    FString ToHexString() const
    {
        return FString::Printf(
            TEXT("0x") UINT64_HEX UINT64_HEX UINT64_HEX UINT64_HEX,
            Upper.GetUpper(),  /* bits 192–255 */
            Upper.GetLower(),  /* bits 128–191 */
            Lower.GetUpper(),  /* bits  64–127 */
            Lower.GetLower()); /* bits   0–63  */
    }

    FString ToString() const { return ToHexString(); }

    /** 32-byte BE array: Upper(127…0) | Lower(127…0) */
    TArray<uint8> ToBytesArray() const
    {
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(32);

        const uint64 Parts[4] =
        {
            Upper.GetUpper(),   // 192–255
            Upper.GetLower(),   // 128–191
            Lower.GetUpper(),   //  64–127
            Lower.GetLower()    //   0–63
        };

        int32 Offset = 0;
        for (uint64 Part : Parts)
        {
            for (int32 ByteIndex = 0; ByteIndex < 8; ++ByteIndex)
            {
                Bytes[Offset++] = static_cast<uint8>((Part >> ((7 - ByteIndex) * 8)) & 0xFF);
            }
        }
        return Bytes;
    }

    static FSpacetimeDBUInt256 FromBytesArray(const TArray<uint8>& Bytes)
    {
        check(Bytes.Num() == 32);

        uint64 Parts[4] = { 0, 0, 0, 0 };
        int32 Offset = 0;

        for (int32 PartIdx = 0; PartIdx < 4; ++PartIdx)
        {
            for (int32 ByteIdx = 0; ByteIdx < 8; ++ByteIdx)
            {
                Parts[PartIdx] = (Parts[PartIdx] << 8) | Bytes[Offset++];
            }
        }

        const FSpacetimeDBUInt128 NewUpper(Parts[0], Parts[1]);
        const FSpacetimeDBUInt128 NewLower(Parts[2], Parts[3]);
        return FSpacetimeDBUInt256(NewUpper, NewLower);
    }

    /** Decimal (unsigned) via your byte→decimal helper */
    FString ToDecimalString() const
    {
        const TArray<uint8> Bytes = ToBytesArray(); // big-endian
        return BigEndianBytesToDecimalString(Bytes);
    }
};

/**
 * Get the hash of a 256-bit unsigned integer, required for use as a TMap key.
 * @param Val The value to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBUInt256& Val)
{
    // Hash the upper and lower 128-bit parts of the 256-bit integer.
    // Assuming FSpacetimeDBUInt128 has its own GetTypeHash defined,
    // which you provided in the initial prompt.
    return HashCombine(GetTypeHash(Val.GetUpper()), GetTypeHash(Val.GetLower()));
}

/**
 * Signed 256-bit integer (two’s-complement).
 * Upper = high 128 bits, Lower = low 128 bits.
 */
USTRUCT(BlueprintType)
struct FSpacetimeDBInt256
{
    GENERATED_BODY()

private:
    UPROPERTY()
    FSpacetimeDBUInt128 Lower;   /** Bits 0-127  (least-significant) */
    UPROPERTY()
    FSpacetimeDBUInt128 Upper;   /** Bits 128-255 (most-significant) */

public:
    /** Zero constructor (needed for BP) */
    FSpacetimeDBInt256() = default;

    /** Construct from two halves */
    FSpacetimeDBInt256(const FSpacetimeDBUInt128& InUpper, const FSpacetimeDBUInt128& InLower)
        : Lower(InLower)
        , Upper(InUpper)
    {
    }

    /**
    * @return true when the 256-bit value is negative (MS-bit set).
    */
    FORCEINLINE bool IsNegative() const noexcept
    {
        // Bit 255 lives in the upper-halfs top 64 bits -> index 63.
        return (Upper.GetUpper() >> 63) != 0;
    }

    FORCEINLINE const FSpacetimeDBUInt128& GetUpper() const { return Upper; }
    FORCEINLINE const FSpacetimeDBUInt128& GetLower() const { return Lower; }

    /* ---------- comparisons ---------- */
    FORCEINLINE bool operator<(const FSpacetimeDBInt256& Rhs) const
    {
        const bool bNegA = IsNegative();
        const bool bNegB = Rhs.IsNegative();
        if (bNegA != bNegB)
        {
            return bNegA;               // negative < positive
        }
        return (Upper < Rhs.Upper) || (Upper == Rhs.Upper && Lower < Rhs.Lower);
    }

    FORCEINLINE bool operator>(const FSpacetimeDBInt256& Rhs) const
    {
        const bool bNegA = IsNegative();
        const bool bNegB = Rhs.IsNegative();
        if (bNegA != bNegB)
        {
            return bNegB;               // positive > negative
        }
        return (Upper > Rhs.Upper) || (Upper == Rhs.Upper && Lower > Rhs.Lower);
    }

    FORCEINLINE bool operator<=(const FSpacetimeDBInt256& Rhs) const
    {
        return !(*this > Rhs);
    }

    FORCEINLINE bool operator>=(const FSpacetimeDBInt256& Rhs) const
    {
        return !(*this < Rhs);
    }

    FORCEINLINE bool operator==(const FSpacetimeDBInt256& Rhs) const
    {
        return Upper == Rhs.Upper && Lower == Rhs.Lower;
    }

    FORCEINLINE bool operator!=(const FSpacetimeDBInt256& Rhs) const
    {
        return !(*this == Rhs);
    }


    /** Hex string “0x[64 hex digits]”. */
    FString ToHexString() const
    {
        return FString::Printf(
            TEXT("0x") UINT64_HEX UINT64_HEX UINT64_HEX UINT64_HEX,
            Upper.GetUpper(),   // bits 192-255
            Upper.GetLower(),   // bits 128-191
            Lower.GetUpper(),   // bits  64-127
            Lower.GetLower());  // bits   0-63
    }

    FString ToString() const { return ToHexString(); }

    /* ---------- helpers ---------- */
    /** Cast from int64 for convenience (matches C# implicit). */
    static FSpacetimeDBInt256 FromInt64(int64 Src)
    {
        const uint64 Low = static_cast<uint64>(Src);
        const uint64 High = static_cast<uint64>(Src >> 63);  // 0 or 0xFFFF… for sign-extension
        return FSpacetimeDBInt256(FSpacetimeDBUInt128(High, 0), FSpacetimeDBUInt128(0, Low));
    }

    TArray<uint8> ToBytesArray() const
    {
        TArray<uint8> Bytes;
        Bytes.SetNumUninitialized(32);

        const uint64 Parts[4] =
        {
            Upper.GetUpper(), Upper.GetLower(),
            Lower.GetUpper(), Lower.GetLower()
        };

        int32 Offset = 0;
        for (uint64 Part : Parts)
        {
            for (int32 ByteIndex = 0; ByteIndex < 8; ++ByteIndex)
            {
                Bytes[Offset++] = static_cast<uint8>((Part >> ((7 - ByteIndex) * 8)) & 0xFF);
            }
        }
        return Bytes;
    }

    static FSpacetimeDBInt256 FromBytesArray(const TArray<uint8>& Bytes)
    {
        check(Bytes.Num() == 32);

        uint64 Parts[4] = {0, 0, 0, 0};
        int32 Offset = 0;

        for (int32 PartIdx = 0; PartIdx < 4; ++PartIdx)
        {
            for (int32 ByteIdx = 0; ByteIdx < 8; ++ByteIdx)
            {
                Parts[PartIdx] = (Parts[PartIdx] << 8) | Bytes[Offset++];
            }
        }

        const FSpacetimeDBUInt128 Upper(Parts[0], Parts[1]);
        const FSpacetimeDBUInt128 Lower(Parts[2], Parts[3]);
        return FSpacetimeDBInt256(Upper, Lower);
    }

    FString ToDecimalString() const
    {
        TArray<uint8> bytes = ToBytesArray(); // big-endian Upper|Lower (two’s-complement)  
        if (IsNegative())
        {
            TArray<uint8> mag = bytes;              // copy
            TwosComplementNegateBigEndian(mag);     // magnitude = (~x)+1
            const FString dec = BigEndianBytesToDecimalString(mag);
            return FString::Printf(TEXT("-%s"), *dec);
        }
        return BigEndianBytesToDecimalString(bytes);
    }
};


UCLASS()
class USpacetimeDBLargeIntegerLibrary : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

public:

    /** FString ← FSpacetimeDBUInt128 */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|LargeInteger",
        meta = (DisplayName = "To String (UInt128)", CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_UInt128ToString(const FSpacetimeDBUInt128& InValue) { return InValue.ToString(); };

    /** FString ← FSpacetimeDBInt128 */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|LargeInteger",
        meta = (DisplayName = "To String (Int128)", CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_Int128ToString(const FSpacetimeDBInt128& InValue) { return InValue.ToString(); };

    /** FString ← FSpacetimeDBUInt256 */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|LargeInteger",
        meta = (DisplayName = "To String (UInt256)", CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_UInt256ToString(const FSpacetimeDBUInt256& InValue) { return InValue.ToString(); };

    /** FString ← FSpacetimeDBInt256 */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|LargeInteger",
        meta = (DisplayName = "To String (Int256)", CompactNodeTitle = ".",
            BlueprintAutocast))
    static FString Conv_Int256ToString(const FSpacetimeDBInt256& InValue) { return InValue.ToString(); };

    /* ───────── UInt128 compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (UInt128)",
            CompactNodeTitle = "=="))
    static bool Equal_UInt128(const FSpacetimeDBUInt128& A,
        const FSpacetimeDBUInt128& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (UInt128)",
            CompactNodeTitle = "!="))
    static bool NotEqual_UInt128(const FSpacetimeDBUInt128& A,
        const FSpacetimeDBUInt128& B)
    {
        return A != B;
    }

    /* ───────── Int128 compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (Int128)",
            CompactNodeTitle = "=="))
    static bool Equal_Int128(const FSpacetimeDBInt128& A,
        const FSpacetimeDBInt128& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (Int128)",
            CompactNodeTitle = "!="))
    static bool NotEqual_Int128(const FSpacetimeDBInt128& A,
        const FSpacetimeDBInt128& B)
    {
        return A != B;
    }

    /* ───────── UInt256 compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (UInt256)",
            CompactNodeTitle = "=="))
    static bool Equal_UInt256(const FSpacetimeDBUInt256& A,
        const FSpacetimeDBUInt256& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (UInt256)",
            CompactNodeTitle = "!="))
    static bool NotEqual_UInt256(const FSpacetimeDBUInt256& A,
        const FSpacetimeDBUInt256& B)
    {
        return A != B;
    }

    /* ───────── Int256 compare ───────── */
    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Equal (Int256)",
            CompactNodeTitle = "=="))
    static bool Equal_Int256(const FSpacetimeDBInt256& A,
        const FSpacetimeDBInt256& B)
    {
        return A == B;
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|Comparison",
        meta = (DisplayName = "Not Equal (Int256)",
            CompactNodeTitle = "!="))
    static bool NotEqual_Int256(const FSpacetimeDBInt256& A,
        const FSpacetimeDBInt256& B)
    {
        return A != B;
    }
};

/**
 * Get the hash of a 256-bit signed integer.
 * @param Val The value to hash.
 * @return The hash value.
 */
inline uint32 GetTypeHash(const FSpacetimeDBInt256& Val)
{
    return HashCombine(GetTypeHash(Val.GetUpper()), GetTypeHash(Val.GetLower()));
}