/**
 * BSATN round-trip test-suite (Simple Automation Test)
 * Put in Source/<YourModule>/Private/Tests/BSATNSerialization.test.cpp
 */

#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"
#include "BSATN/UESpacetimeDB.h"
#include <iomanip>
#include <iostream>
#include <cmath>
#include <climits>
#include "Math/Quat.h"
#include "Math/Transform.h"
#include "StructUtils/InstancedStruct.h"

#include "SpacetimeDBBSATNTestOrg.generated.h"




 // ──────────────────────────────────────────────────────────────────────────────
 // Logging helpers
 // ──────────────────────────────────────────────────────────────────────────────
#if WITH_DEV_AUTOMATION_TESTS

/**
 * Logs the start of a new category .
 * @param CategoryName The name of the test.
 */
#define LOG_Category(CategoryName) \
	do \
	{ \
		const FString LogMessage = FString::Printf(TEXT("[CATEGORY] %s"), TEXT(CategoryName)); \
		UE_LOG(LogTemp, Log, TEXT("\n%s"), *LogMessage); \
		if (this) \
		{ \
			this->AddInfo(LogMessage); \
		} \
	} while (false)

#define LOG_TEST(Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("\n[TEST] %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		if (this) \
		{ \
			this->AddInfo(LogMessage); \
		} \
	} while (false)

 /**
  * Logs a success message to the output log and the automation test results window.
  * @param Format The format string for the message.
  * @param ... The arguments for the format string.
  */
#define LOG_SUCCESS(Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ✓ %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		if (this) \
		{ \
			this->AddInfo(LogMessage); \
		} \
	} while (false)

  /**
   * Logs a failure message to the output log and the automation test results window, and marks the test as failed.
   * @param Format The format string for the message.
   * @param ... The arguments for the format string.
   */
#define LOG_FAIL(Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ✗ %s"), *UserMessage); \
		UE_LOG(LogTemp, Error, TEXT("%s"), *LogMessage); \
		if (this) \
		{ \
			this->AddError(LogMessage); \
		} \
	} while (false)

   /**
	* Logs an informational message to the output log and the automation test results window.
	* @param Format The format string for the message.
	* @param ... The arguments for the format string.
	*/
#define LOG_INFO(Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ℹ %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		if (this) \
		{ \
			this->AddInfo(LogMessage); \
		} \
	} while (false)

	/**
	 * Performs a round-trip serialization/deserialization test for a given type and value.
	 * @param Type The data type to test.
	 * @param ValueLiteral The literal value to use for the test.
	 * @param TestName A descriptive name for the test case.
	 */
#define TEST_ROUNDTRIP(Type, ValueLiteral, TestName) \
	do \
	{ \
		const Type Original = ValueLiteral; \
		const TArray<uint8> Bytes = UE::SpacetimeDB::Serialize(Original); \
		const Type Round = UE::SpacetimeDB::Deserialize<Type>(Bytes); \
		if (TestEq::Same(Original, Round)) \
		{ \
			LOG_SUCCESS(TEXT("%s: Round-trip ok"), TEXT(TestName)); \
		} \
		else \
		{ \
			LOG_FAIL(TEXT("%s: Mismatch after round-trip"), TEXT(TestName)); \
		} \
	} while (false)

#endif // WITH_DEV_AUTOMATION_TESTS

// ──────────────────────────────────────────────────────────────────────────────
// Generic tolerant equality helpers
// ──────────────────────────────────────────────────────────────────────────────
namespace TestEq
{

	inline bool Float(float A, float B, float Epsilon = 1e-4f)
	{
		return FMath::Abs(A - B) < Epsilon;
	}

	template<typename T>
	inline bool Same(const T& A, const T& B)
	{
		return A == B;
	}

	template<>
	inline bool Same<float>(const float& A, const float& B)
	{
		return Float(A, B);
	}

	template<>
	inline bool Same<double>(const double& A, const double& B)
	{
		return Float(static_cast<float>(A), static_cast<float>(B));
	}

	template<>
	inline bool Same<FVector>(const FVector& A, const FVector& B)
	{
		return Float(A.X, B.X) && Float(A.Y, B.Y) && Float(A.Z, B.Z);
	}

	template<>
	inline bool Same<FRotator>(const FRotator& A, const FRotator& B)
	{
		return Float(A.Pitch, B.Pitch) && Float(A.Yaw, B.Yaw) && Float(A.Roll, B.Roll);
	}

	template<>
	inline bool Same<FTransform>(const FTransform& A, const FTransform& B)
	{
		return Same(A.GetLocation(), B.GetLocation()) &&
			Same(A.GetRotation().Rotator(), B.GetRotation().Rotator()) &&
			Same(A.GetScale3D(), B.GetScale3D());
	}

	template<typename T>
	inline bool Same(const TObjectPtr<T>& A, const TObjectPtr<T>& B)
	{
		const T* PtrA = A.Get();
		const T* PtrB = B.Get();

		// If both pointers are null, they are considered the same.
		if (PtrA == nullptr && PtrB == nullptr)
		{
			return true;
		}

		// If one is null but the other isn't, they are different.
		if (PtrA == nullptr || PtrB == nullptr)
		{
			return false;
		}

		// If both are valid, dereference them to call the T::operator==
		// This performs the deep, value-based comparison on the UObject itself.
		return *PtrA == *PtrB;
	}
}


// ──────────────────────────────────────────────────────────────────────────────
// Utility: little hex-dump for debugging
// ──────────────────────────────────────────────────────────────────────────────
static void PrintHex(const TArray<uint8>& Bytes, const FString& Label)
{
	constexpr int32 MaxDisplay = 32;
	std::cout << TCHAR_TO_UTF8(*Label) << " (" << Bytes.Num() << " bytes): ";
	for (int32 i = 0; i < FMath::Min(Bytes.Num(), MaxDisplay); ++i)
	{
		std::cout << std::hex << std::setw(2) << std::setfill('0')
			<< static_cast<int>(Bytes[i]) << " ";
	}
	if (Bytes.Num() > MaxDisplay)
	{
		std::cout << "...";
	}
	std::cout << std::dec << '\n';
}



// ──────────────────────────────────────────────────────────────────────────────
// Test Enum
// ──────────────────────────────────────────────────────────────────────────────
UENUM(BlueprintType)
enum class ESpaceTimeDBTestEnum1 : uint8
{
	First,
	Secound,
	Third
};

UENUM(BlueprintType)
enum class ECharacterTypeTag : uint8
{
	PlayerData,
	Npc,
};

// ──────────────────────────────────────────────────────────────────────────────
// Test Struct
// ──────────────────────────────────────────────────────────────────────────────
USTRUCT(BlueprintType)
struct FPlayerData
{

	GENERATED_BODY()

	/** Player’s display name */
	FString PlayerName;

	/** Current character level */
	int32   Level;


	/** Simple inventory list */
	TArray<FString>  Inventory;

	bool operator==(const FPlayerData& Other) const
	{
		return PlayerName == Other.PlayerName &&
			Level == Other.Level &&
			Inventory == Other.Inventory;
	}
};

namespace UE::SpacetimeDB
{
	UE_SPACETIMEDB_STRUCT(FPlayerData, PlayerName, Level, Inventory);
}


USTRUCT(BlueprintType)
struct FNpc
{
	GENERATED_BODY()

	/** Player’s display name */
	FString Type;


	bool operator==(const FNpc& Other) const
	{
		return Type == Other.Type;
	}
};

namespace UE::SpacetimeDB
{
	UE_SPACETIMEDB_STRUCT(
		FNpc,    //1 The USTRUCT
		Type	 //2-a Variable name
	);
}

// ──────────────────────────────────────────────────────────────────────────────
// Test Variants / Tagged Enum
// ──────────────────────────────────────────────────────────────────────────────

USTRUCT(BlueprintType)
struct FCharacterType
{
	GENERATED_BODY()

public:

	FCharacterType() = default;

	// Keep Data before/after Tag however your codegen/macros expect. Here we use 'Data' like your old field name.
	TVariant<FPlayerData, FNpc> MessageData;

	UPROPERTY(BlueprintReadOnly)
	ECharacterTypeTag Tag = static_cast<ECharacterTypeTag>(0);


public:
	// ----- Static builders (mirror old style; no UObject/Blueprint library needed) -----

	static FCharacterType PlayerData(const FPlayerData& Value)
	{
		FCharacterType Obj;
		Obj.Tag = ECharacterTypeTag::PlayerData;
		Obj.MessageData.Set<FPlayerData>(Value);
		return Obj;
	}

	static FCharacterType Npc(const FNpc& Value)
	{
		FCharacterType Obj;
		Obj.Tag = ECharacterTypeTag::Npc;
		Obj.MessageData.Set<FNpc>(Value);
		return Obj;
	}

	// ----- Query helpers -----

	bool IsPlayerData() const { return Tag == ECharacterTypeTag::PlayerData; }
	bool IsNpc()        const { return Tag == ECharacterTypeTag::Npc; }

	FPlayerData GetAsPlayer() const
	{
		ensureMsgf(IsPlayerData(), TEXT("MessageData does not hold PlayerData!"));
		return MessageData.Get<FPlayerData>();
	}

	FNpc GetAsNpc() const
	{
		ensureMsgf(IsNpc(), TEXT("MessageData does not hold Npc!"));
		return MessageData.Get<FNpc>();
	}

	// ----- Equality -----

	bool operator==(const FCharacterType& Other) const
	{
		if (Tag != Other.Tag) { return false; }

		switch (Tag)
		{
		case ECharacterTypeTag::PlayerData:
			return GetAsPlayer() == Other.GetAsPlayer();
		case ECharacterTypeTag::Npc:
			return GetAsNpc() == Other.GetAsNpc();
		default:
			return false;
		}
	}

	bool operator!=(const FCharacterType& Other) const
	{
		return !(*this == Other);
	}
};

namespace UE::SpacetimeDB
{

	UE_SPACETIMEDB_ENABLE_TARRAY(FCharacterType);


	UE_SPACETIMEDB_TAGGED_ENUM(
		FCharacterType,				// 1 The USTRUCT type
		ECharacterTypeTag,			// 2 The UENUM tag type
		MessageData,				// 3 The TVariant field name in Struct
		PlayerData, FPlayerData,	// 4-a First (Tag, Type) pair
		Npc, FNpc					// 4-b Second (Tag, Type) pair
	);					
}

// ──────────────────────────────────────────────────────────────────────────────
// Test Struct with Variant
// ──────────────────────────────────────────────────────────────────────────────

USTRUCT(BlueprintType)
struct FCharacterThing
{
	GENERATED_BODY()

	/** Player’s display name */
	UPROPERTY()
	FCharacterType Type;

	/** Current Activation */
	bool Active;

	bool operator==(const FCharacterThing& Other) const
	{
		return Type == Other.Type && Active == Other.Active;
	}
};

namespace UE::SpacetimeDB
{
	UE_SPACETIMEDB_STRUCT(FCharacterThing, Type, Active);
}



// ──────────────────────────────────────────────────────────────────────────────
// Test custom optional type
// ──────────────────────────────────────────────────────────────────────────────

USTRUCT(BlueprintType)
struct FManaOptional
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	bool bHasMana = false;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB", meta = (EditCondition = "bHasName"))
	int Mana = 0;

	FORCEINLINE bool operator==(const FManaOptional& Other) const
	{
		return Mana == Other.Mana && bHasMana == Other.bHasMana;
	}

	FORCEINLINE bool operator!=(const FManaOptional& Other) const
	{
		return !(*this == Other);
	}
};

namespace UE::SpacetimeDB
{
	UE_SPACETIMEDB_OPTIONAL(FManaOptional, bHasMana, Mana);
}
