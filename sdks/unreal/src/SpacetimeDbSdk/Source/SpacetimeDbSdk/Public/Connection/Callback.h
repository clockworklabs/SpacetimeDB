#pragma once

#include "CoreMinimal.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/TransactionUpdateType.g.h"
#include "ModuleBindings/Types/ReducerCallInfoType.g.h"
#include "ModuleBindings/Types/UpdateStatusType.g.h"
#include "ModuleBindings/Types/EnergyQuantaType.g.h"
#include "Types/Builtins.h"
#include "Types/UnitType.h"

#include "Callback.generated.h"

/**
 * Types and helper utilities used by connection callbacks.
 */


//Forward declare
class UDbConnectionBase;


/** Termination status for a reducer event. */
UENUM(BlueprintType)
enum class ESpacetimeDBStatusTag : uint8
{
	/** Reducer committed successfully */
	Committed,
	/** Reducer execution failed */
	Failed,
	/** Reducer aborted due to energy limits */
	OutOfEnergy
};

USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBStatus
{
	GENERATED_BODY()

public:
	FSpacetimeDBStatus() = default;

	// NOTE: order matches ESpacetimeDBStatusTag: Committed, Failed, OutOfEnergy
	// Payloads:
	//   Committed     -> FSpacetimeDBUnit
	//   Failed        -> FString
	//   OutOfEnergy   -> FSpacetimeDBUnit
	TVariant<FSpacetimeDBUnit, FString> MessageData;

	UPROPERTY(BlueprintReadOnly)
	ESpacetimeDBStatusTag Tag = ESpacetimeDBStatusTag::Committed;

	// -- Static constructors ----------------------
	static FSpacetimeDBStatus Committed( const FSpacetimeDBUnit& SpacetimeDBUnit)
	{
		FSpacetimeDBStatus Obj;
		Obj.Tag = ESpacetimeDBStatusTag::Committed;
		Obj.MessageData.Set<FSpacetimeDBUnit>(SpacetimeDBUnit);
		return Obj;
	}

	static FSpacetimeDBStatus Failed(const FString& Error)
	{
		FSpacetimeDBStatus Obj;
		Obj.Tag = ESpacetimeDBStatusTag::Failed;
		Obj.MessageData.Set<FString>(Error);
		return Obj;
	}

	static FSpacetimeDBStatus OutOfEnergy(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBStatus Obj;
		Obj.Tag = ESpacetimeDBStatusTag::OutOfEnergy;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	// -- Query helpers ----------------------
	FORCEINLINE bool IsCommitted() const { return Tag == ESpacetimeDBStatusTag::Committed; }
	FORCEINLINE bool IsFailed() const { return Tag == ESpacetimeDBStatusTag::Failed; }
	FORCEINLINE bool IsOutOfEnergy() const { return Tag == ESpacetimeDBStatusTag::OutOfEnergy; }

	FORCEINLINE FSpacetimeDBUnit GetAsCommitted() const
	{
		ensureMsgf(IsCommitted(), TEXT("MessageData does not hold Committed!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE FString GetAsFailed() const
	{
		ensureMsgf(IsFailed(), TEXT("MessageData does not hold Failed!"));
		return MessageData.Get<FString>();
	}

	FORCEINLINE FSpacetimeDBUnit GetAsOutOfEnergy() const
	{
		ensureMsgf(IsOutOfEnergy(), TEXT("MessageData does not hold OutOfEnergy!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	// -- Equality ----------------------
	FORCEINLINE bool operator==(const FSpacetimeDBStatus& Other) const
	{
		if (Tag != Other.Tag) return false;

		switch (Tag)
		{
		case ESpacetimeDBStatusTag::Committed:
			return GetAsCommitted() == Other.GetAsCommitted();
		case ESpacetimeDBStatusTag::Failed:
			return GetAsFailed() == Other.GetAsFailed();
		case ESpacetimeDBStatusTag::OutOfEnergy:
			return GetAsOutOfEnergy() == Other.GetAsOutOfEnergy();
		default:
			return false;
		}
	}
	FORCEINLINE bool operator!=(const FSpacetimeDBStatus& Other) const { return !(*this == Other); }
};

FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBStatus& Status)
{
	const uint32 TagHash = ::GetTypeHash(static_cast<uint8>(Status.Tag));

	switch (Status.Tag)
	{
	case ESpacetimeDBStatusTag::Committed:
		return HashCombine(TagHash, ::GetTypeHash(Status.GetAsCommitted()));
	case ESpacetimeDBStatusTag::Failed:
		return HashCombine(TagHash, GetTypeHash(Status.GetAsFailed()));
	case ESpacetimeDBStatusTag::OutOfEnergy:
		return HashCombine(TagHash, ::GetTypeHash(Status.GetAsOutOfEnergy()));
	default:
		return TagHash;
	}
}

UCLASS()
class SPACETIMEDBSDK_API USpacetimeDBStatusBpLib : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FSpacetimeDBStatus Committed(const FSpacetimeDBUnit& InValue)
	{
		return FSpacetimeDBStatus::Committed(InValue);
	}

	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FSpacetimeDBStatus Failed(const FString& InValue)
	{
		return FSpacetimeDBStatus::Failed(InValue);
	}

	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FSpacetimeDBStatus OutOfEnergy(const FSpacetimeDBUnit& InValue)
	{
		return FSpacetimeDBStatus::OutOfEnergy(InValue);
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static bool IsCommitted(const FSpacetimeDBStatus& Status) { return Status.IsCommitted(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static bool IsFailed(const FSpacetimeDBStatus& Status) { return Status.IsFailed(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static bool IsOutOfEnergy(const FSpacetimeDBStatus& Status) { return Status.IsOutOfEnergy(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FSpacetimeDBUnit GetAsCommitted(const FSpacetimeDBStatus& Status)
	{
		return Status.GetAsCommitted();
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FString GetAsFailed(const FSpacetimeDBStatus& Status)
	{
		return Status.GetAsFailed();
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|SpacetimeDBStatus")
	static FSpacetimeDBUnit GetAsOutOfEnergy(const FSpacetimeDBStatus& Status)
	{
		return Status.GetAsOutOfEnergy();
	}
};


/** Metadata describing a reducer run. */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FReducerEvent
{
	GENERATED_BODY()

	/** Timestamp for when the reducer executed */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBTimestamp Timestamp;

	/** Result status of the reducer */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBStatus Status;

	/** Identity that initiated the call */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBIdentity CallerIdentity;

	/** Connection ID for the caller */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBConnectionId CallerConnectionId;

	/** Energy consumed while executing */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FEnergyQuantaType EnergyConsumed;

	/** Detailed call information */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FReducerCallInfoType ReducerCall;

	FORCEINLINE bool operator==(const FReducerEvent& Other) const
	{
		return Status == Other.Status && Timestamp == Other.Timestamp && CallerIdentity == Other.CallerIdentity &&
			CallerConnectionId == Other.CallerConnectionId && EnergyConsumed == Other.EnergyConsumed &&
			ReducerCall == Other.ReducerCall;
	}
	FORCEINLINE bool operator!=(const FReducerEvent& Other) const
	{
		return !(*this == Other);
	}
};

FORCEINLINE uint32 GetTypeHash(const FReducerEvent& ReducerEvent)
{
	uint32 Hash = GetTypeHash(ReducerEvent.Status);
	Hash = HashCombine(Hash, GetTypeHash(ReducerEvent.Timestamp));
	Hash = HashCombine(Hash, GetTypeHash(ReducerEvent.CallerIdentity));
	Hash = HashCombine(Hash, GetTypeHash(ReducerEvent.CallerConnectionId));
	Hash = HashCombine(Hash, GetTypeHash(ReducerEvent.EnergyConsumed));
	Hash = HashCombine(Hash, GetTypeHash(ReducerEvent.ReducerCall));
	return Hash;
}

/** High level event description used in callback contexts. */
UENUM(BlueprintType)
enum class ESpacetimeDBEventTag : uint8
{
	/** A reducer event */
	Reducer,
	/** Subscription applied */
	SubscribeApplied,
	/** Subscription removed */
	UnsubscribeApplied,
	/** Connection lost */
	Disconnected,
	/** Subscription error */
	SubscribeError,
	/** Unknown transaction type */
	UnknownTransaction
};

USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBEvent
{
	GENERATED_BODY()

public:
	FSpacetimeDBEvent() = default;

	TVariant<
		FReducerEvent,       // Reducer
		FSpacetimeDBUnit,    // SubscribeApplied, UnsubscribeApplied, Disconnected, UnknownTransaction
		FString              // SubscribeError
	> MessageData;

	UPROPERTY(BlueprintReadOnly)
	ESpacetimeDBEventTag Tag = ESpacetimeDBEventTag::UnknownTransaction;

	// Static factory methods
	static FSpacetimeDBEvent Reducer(const FReducerEvent& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::Reducer;
		Obj.MessageData.Set<FReducerEvent>(Value);
		return Obj;
	}

	static FSpacetimeDBEvent SubscribeApplied(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::SubscribeApplied;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	static FSpacetimeDBEvent UnsubscribeApplied(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::UnsubscribeApplied;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	static FSpacetimeDBEvent Disconnected(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::Disconnected;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	static FSpacetimeDBEvent SubscribeError(const FString& InError)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::SubscribeError;
		Obj.MessageData.Set<FString>(InError);
		return Obj;
	}

	static FSpacetimeDBEvent UnknownTransaction(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::UnknownTransaction;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	// Tag checks + GetAs methods
	FORCEINLINE bool IsReducer() const { return Tag == ESpacetimeDBEventTag::Reducer; }
	FORCEINLINE FReducerEvent GetAsReducer() const
	{
		ensureMsgf(IsReducer(), TEXT("MessageData does not hold Reducer!"));
		return MessageData.Get<FReducerEvent>();
	}

	FORCEINLINE bool IsSubscribeApplied() const { return Tag == ESpacetimeDBEventTag::SubscribeApplied; }
	FORCEINLINE FSpacetimeDBUnit GetAsSubscribeApplied() const
	{
		ensureMsgf(IsSubscribeApplied(), TEXT("MessageData does not hold SubscribeApplied!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE bool IsUnsubscribeApplied() const { return Tag == ESpacetimeDBEventTag::UnsubscribeApplied; }
	FORCEINLINE FSpacetimeDBUnit GetAsUnsubscribeApplied() const
	{
		ensureMsgf(IsUnsubscribeApplied(), TEXT("MessageData does not hold UnsubscribeApplied!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE bool IsDisconnected() const { return Tag == ESpacetimeDBEventTag::Disconnected; }
	FORCEINLINE FSpacetimeDBUnit GetAsDisconnected() const
	{
		ensureMsgf(IsDisconnected(), TEXT("MessageData does not hold Disconnected!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE bool IsSubscribeError() const { return Tag == ESpacetimeDBEventTag::SubscribeError; }
	FORCEINLINE FString GetAsSubscribeError() const
	{
		ensureMsgf(IsSubscribeError(), TEXT("MessageData does not hold SubscribeError!"));
		return MessageData.Get<FString>();
	}

	FORCEINLINE bool IsUnknownTransaction() const { return Tag == ESpacetimeDBEventTag::UnknownTransaction; }
	FORCEINLINE FSpacetimeDBUnit GetAsUnknownTransaction() const
	{
		ensureMsgf(IsUnknownTransaction(), TEXT("MessageData does not hold UnknownTransaction!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	// Equality operators
	FORCEINLINE bool operator==(const FSpacetimeDBEvent& Other) const
	{
		if (Tag != Other.Tag) return false;

		switch (Tag)
		{
		case ESpacetimeDBEventTag::Reducer:
			return GetAsReducer() == Other.GetAsReducer();
		case ESpacetimeDBEventTag::SubscribeApplied:
			return GetAsSubscribeApplied() == Other.GetAsSubscribeApplied();
		case ESpacetimeDBEventTag::UnsubscribeApplied:
			return GetAsUnsubscribeApplied() == Other.GetAsUnsubscribeApplied();
		case ESpacetimeDBEventTag::Disconnected:
			return GetAsDisconnected() == Other.GetAsDisconnected();
		case ESpacetimeDBEventTag::SubscribeError:
			return GetAsSubscribeError() == Other.GetAsSubscribeError();
		case ESpacetimeDBEventTag::UnknownTransaction:
			return GetAsUnknownTransaction() == Other.GetAsUnknownTransaction();
		default:
			return false;
		}
	}

	FORCEINLINE bool operator!=(const FSpacetimeDBEvent& Other) const
	{
		return !(*this == Other);
	}
};

// Hash function
FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBEvent& Event)
{
	const uint32 TagHash = GetTypeHash(static_cast<uint8>(Event.Tag));
	switch (Event.Tag)
	{
	case ESpacetimeDBEventTag::Reducer: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsReducer()));
	case ESpacetimeDBEventTag::SubscribeApplied: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsSubscribeApplied()));
	case ESpacetimeDBEventTag::UnsubscribeApplied: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsUnsubscribeApplied()));
	case ESpacetimeDBEventTag::Disconnected: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsDisconnected()));
	case ESpacetimeDBEventTag::SubscribeError: return HashCombine(TagHash, GetTypeHash(Event.GetAsSubscribeError()));
	case ESpacetimeDBEventTag::UnknownTransaction: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsUnknownTransaction()));
	default: return TagHash;
	}
}

/**
 * Context passed to callbacks triggered by SpacetimeDB events.
 * Contains a pointer back to the connection that produced the event
 * and the raw server message that caused the callback.
 */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FEventContextBase
{
	GENERATED_BODY()

	/** Description of the event that triggered this callback */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FSpacetimeDBEvent Event;
};


/**
 * Context used for subscription lifecycle callbacks (apply/unapply).
 */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSubscriptionEventContextBase
{
	GENERATED_BODY()

	/** Description of the subscription event */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FSpacetimeDBEvent Event;
};

/**
 * Context used when reporting errors back to user callbacks.
 */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FErrorContextBase
{
	GENERATED_BODY()

	/** Text describing the error */
	UPROPERTY(BlueprintReadOnly, Category = "SpacetimeDB")
	FString Error;
};