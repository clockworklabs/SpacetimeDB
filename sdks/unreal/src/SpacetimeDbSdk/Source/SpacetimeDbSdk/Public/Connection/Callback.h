#pragma once

#include "CoreMinimal.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/TransactionUpdateType.g.h"
#include "ModuleBindings/Types/ReducerCallInfoType.g.h"
#include "ModuleBindings/Types/UpdateStatusType.g.h"
#include "ModuleBindings/Types/EnergyQuantaType.g.h"
#include "Types/Builtins.h"
#include "Types/UnitType.h"
#include <atomic>
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

USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FProcedureEvent
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FProcedureStatusType Status;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBTimestamp Timestamp;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	FSpacetimeDBTimeDuration TotalHostExecutionDuration;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	bool Success = false;

	FORCEINLINE bool operator==(const FProcedureEvent& Other) const
	{
		return Status == Other.Status && Timestamp == Other.Timestamp && TotalHostExecutionDuration == Other.TotalHostExecutionDuration && Success == Other.Success;
	}

	FORCEINLINE bool operator!=(const FProcedureEvent& Other) const
	{
		return !(*this == Other);
	}
};
FORCEINLINE uint32 GetTypeHash(const FProcedureEvent& ProcedureResult)
{
	uint32 Hash = GetTypeHash(ProcedureResult.Status);
	Hash = HashCombine(Hash, GetTypeHash(ProcedureResult.Timestamp));
	Hash = HashCombine(Hash, GetTypeHash(ProcedureResult.TotalHostExecutionDuration));
	Hash = HashCombine(Hash, GetTypeHash(ProcedureResult.Success));
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
	UnknownTransaction,
	/** A procedure event */
	Procedure
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
		FString,             // SubscribeError
		FProcedureEvent		 // Procedure
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

	static FSpacetimeDBEvent Procedure(const FProcedureEvent& Value)
	{
		FSpacetimeDBEvent Obj;
		Obj.Tag = ESpacetimeDBEventTag::Procedure;
		Obj.MessageData.Set<FProcedureEvent>(Value);
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

	FORCEINLINE bool IsProcedure() const { return Tag == ESpacetimeDBEventTag::Procedure; }
	FORCEINLINE FProcedureEvent GetAsProcedure() const
	{
		ensureMsgf(IsProcedure(), TEXT("MessageData does not hold Procedure!"));
		return MessageData.Get<FProcedureEvent>();
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
		case ESpacetimeDBEventTag::Procedure:
			return GetAsProcedure() == Other.GetAsProcedure();
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
	case ESpacetimeDBEventTag::Procedure: return HashCombine(TagHash, ::GetTypeHash(Event.GetAsProcedure()));
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

DECLARE_DELEGATE_ThreeParams(
	FOnProcedureCompleteDelegate,
	const FSpacetimeDBEvent& /*EventContext*/,
	const TArray<uint8>& /*ResultData*/,
	bool /*bSuccess*/);

/** Simple procedure callback management - game thread only for callbacks, atomic for request IDs */
UCLASS()
class SPACETIMEDBSDK_API UProcedureCallbacks : public UObject
{
	GENERATED_BODY()
public:
    /** Register a callback for a procedure call */
    uint32 RegisterCallback(const FOnProcedureCompleteDelegate& Callback);
    
    /** Resolve a procedure callback with results */
    bool ResolveCallback(uint32 RequestId, const FSpacetimeDBEvent& EventContext, 
                        const TArray<uint8>& ResultData, bool bSuccess);
    
    /** Remove a callback (for explicit cleanup) */
    bool RemoveCallback(uint32 RequestId);
    
    /** Clear all pending callbacks (on disconnect) */
    void ClearAllCallbacks();

    /** Get the next available request ID - thread safe */
    uint32 GetNextRequestId();

private:
    /** Map of request ID to callback - game thread only, no locking needed */
    TMap<uint32, FOnProcedureCompleteDelegate> PendingCallbacks;
    
    /** Counter for generating unique request IDs - atomic for thread safety */
    std::atomic<uint32> NextRequestIdCounter{1};
};


USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBProcedureStatus
{
	GENERATED_BODY()

public:
	FSpacetimeDBProcedureStatus() = default;

	// NOTE: order matches ESpacetimeDBStatusTag: Committed, Failed, OutOfEnergy
	// Payloads:
	//   Returned      -> FSpacetimeDBUnit
	//   OutOfEnergy   -> FSpacetimeDBUnit
	//   InternalError -> FString
	TVariant<FSpacetimeDBUnit, FString> MessageData;

	UPROPERTY(BlueprintReadOnly)
	EProcedureStatusTag Tag = EProcedureStatusTag::Returned;

	// -- Static constructors ----------------------
	static FSpacetimeDBProcedureStatus Returned( const FSpacetimeDBUnit& SpacetimeDBUnit)
	{
		FSpacetimeDBProcedureStatus Obj;
		Obj.Tag = EProcedureStatusTag::Returned;
		Obj.MessageData.Set<FSpacetimeDBUnit>(SpacetimeDBUnit);
		return Obj;
	}

	static FSpacetimeDBProcedureStatus InternalError(const FString& Error)
	{
		FSpacetimeDBProcedureStatus Obj;
		Obj.Tag = EProcedureStatusTag::InternalError;
		Obj.MessageData.Set<FString>(Error);
		return Obj;
	}

	static FSpacetimeDBProcedureStatus OutOfEnergy(const FSpacetimeDBUnit& Value)
	{
		FSpacetimeDBProcedureStatus Obj;
		Obj.Tag = EProcedureStatusTag::OutOfEnergy;
		Obj.MessageData.Set<FSpacetimeDBUnit>(Value);
		return Obj;
	}

	static FSpacetimeDBProcedureStatus FromStatus(const FProcedureStatusType& Value)
	{
		switch (Value.Tag)
		{
		case EProcedureStatusTag::Returned:
			return Returned(FSpacetimeDBUnit());
		case EProcedureStatusTag::OutOfEnergy:
			return OutOfEnergy(Value.GetAsOutOfEnergy());
		case EProcedureStatusTag::InternalError:
			return InternalError(Value.GetAsInternalError());
		default:
			return Returned(FSpacetimeDBUnit());
		}
	}
	// -- Query helpers ----------------------
	FORCEINLINE bool IsReturned() const { return Tag == EProcedureStatusTag::Returned; }
	FORCEINLINE bool IsOutOfEnergy() const { return Tag == EProcedureStatusTag::OutOfEnergy; }
	FORCEINLINE bool IsInternalError() const { return Tag == EProcedureStatusTag::InternalError; }

	FORCEINLINE FSpacetimeDBUnit GetAsReturned() const
	{
		ensureMsgf(IsReturned(), TEXT("MessageData does not hold Returned!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE FSpacetimeDBUnit GetAsOutOfEnergy() const
	{
		ensureMsgf(IsOutOfEnergy(), TEXT("MessageData does not hold OutOfEnergy!"));
		return MessageData.Get<FSpacetimeDBUnit>();
	}

	FORCEINLINE FString GetAsInternalError() const
	{
		ensureMsgf(IsInternalError(), TEXT("MessageData does not hold InternalError!"));
		return MessageData.Get<FString>();
	}

	// -- Equality ----------------------
	FORCEINLINE bool operator==(const FSpacetimeDBProcedureStatus& Other) const
	{
		if (Tag != Other.Tag) return false;

		switch (Tag)
		{
		case EProcedureStatusTag::Returned:
			return GetAsReturned() == Other.GetAsReturned();
		case EProcedureStatusTag::OutOfEnergy:
			return GetAsOutOfEnergy() == Other.GetAsOutOfEnergy();
		case EProcedureStatusTag::InternalError:
			return GetAsInternalError() == Other.GetAsInternalError();
		default:
			return false;
		}
	}
	FORCEINLINE bool operator!=(const FSpacetimeDBProcedureStatus& Other) const { return !(*this == Other); }
};

FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBProcedureStatus& Status)
{
	const uint32 TagHash = ::GetTypeHash(static_cast<uint8>(Status.Tag));

	switch (Status.Tag)
	{
	case EProcedureStatusTag::Returned:
		return HashCombine(TagHash, ::GetTypeHash(Status.GetAsReturned()));
	case EProcedureStatusTag::OutOfEnergy:
		return HashCombine(TagHash, ::GetTypeHash(Status.GetAsOutOfEnergy()));
	case EProcedureStatusTag::InternalError:
		return HashCombine(TagHash, GetTypeHash(Status.GetAsInternalError()));
	default:
		return TagHash;
	}
}

UCLASS()
class SPACETIMEDBSDK_API USpacetimeDBProcedureStatusBpLib : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

private:
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|ProcedureStatus")
	static FSpacetimeDBProcedureStatus Returned(const FSpacetimeDBUnit& InValue)
	{
		return FSpacetimeDBProcedureStatus::Returned(InValue);
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static bool IsReturned(const FProcedureStatusType& InValue) { return InValue.IsReturned(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static TArray<uint8> GetAsReturned(const FProcedureStatusType& InValue)
	{
		return InValue.GetAsReturned();
	}

	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|ProcedureStatus")
	static FProcedureStatusType OutOfEnergy(const FSpacetimeDBUnit& InValue)
	{
		return FProcedureStatusType::OutOfEnergy(InValue);
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static bool IsOutOfEnergy(const FProcedureStatusType& InValue) { return InValue.IsOutOfEnergy(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static FSpacetimeDBUnit GetAsOutOfEnergy(const FProcedureStatusType& InValue)
	{
		return InValue.GetAsOutOfEnergy();
	}

	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|ProcedureStatus")
	static FProcedureStatusType InternalError(const FString& InValue)
	{
		return FProcedureStatusType::InternalError(InValue);
	}

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static bool IsInternalError(const FProcedureStatusType& InValue) { return InValue.IsInternalError(); }

	UFUNCTION(BlueprintPure, Category = "SpacetimeDB|ProcedureStatus")
	static FString GetAsInternalError(const FProcedureStatusType& InValue)
	{
		return InValue.GetAsInternalError();
	}
};

