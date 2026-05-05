#pragma once

#include "CoreMinimal.h"
#include "UObject/NoExportTypes.h"
#include "Types/Builtins.h"
#include "Websocket.h"
#include "Subscription.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "DBCache/TableAppliedDiff.h"
#include "HAL/CriticalSection.h"
#include "HAL/ThreadSafeBool.h"
#include "BSATN/UEBSATNHelpers.h"
#include "Connection/Callback.h"
#include "LogCategory.h"
#include <type_traits>

#include "DbConnectionBase.generated.h"

// Forward declarations
class UDbConnectionBuilder;
class UProcedureCallbacks;
class FSpacetimeDbInboundWorker;

/** Macro for safae way to bind delegate without needing to write Function name as an FName. */
#define BIND_DELEGATE_SAFE(DelegateVar, Object, ClassType, FunctionName) \
	DelegateVar.BindUFunction(Object, GET_FUNCTION_NAME_CHECKED(ClassType, FunctionName))

/** Macro for safe way to unbind delegate without needing to write Function name as an FName. */
#define UNBIND_DELEGATE_SAFE(DelegateVar, Object, ClassType, FunctionName) \
	DelegateVar.Remove(Object, GET_FUNCTION_NAME_CHECKED(ClassType, FunctionName))

/** Delegate called when the connection attempt fails. */
DECLARE_DYNAMIC_DELEGATE_OneParam(
	FOnConnectErrorDelegate,
	const FString&, ErrorMessage);

/** Called when a connection is established. */
DECLARE_DYNAMIC_DELEGATE_ThreeParams(
	FOnConnectBaseDelegate,
	UDbConnectionBase*, Connection,
	FSpacetimeDBIdentity, Identity,
	const FString&, Token);

/** Called when a connection closes. */
DECLARE_DYNAMIC_DELEGATE_TwoParams(
	FOnDisconnectBaseDelegate,
	UDbConnectionBase*, Connection,
	const FString&, Error);

/** Runtime-compatible database update wrapper used by table-update pipeline. */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FDatabaseUpdateType
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	TArray<FTableUpdateType> Tables;

	FORCEINLINE bool operator==(const FDatabaseUpdateType& Other) const
	{
		return Tables == Other.Tables;
	}

	FORCEINLINE bool operator!=(const FDatabaseUpdateType& Other) const
	{
		return !(*this == Other);
	}
};

FORCEINLINE uint32 GetTypeHash(const FDatabaseUpdateType& DatabaseUpdate)
{
	return GetTypeHash(DatabaseUpdate.Tables);
}


/** Key used to index preprocessed table data without relying on row addresses */
struct FPreprocessedTableKey
{
	FString TableName;

	FPreprocessedTableKey() = default;
	explicit FPreprocessedTableKey(const FString& InName)
		: TableName(InName) {
	}

	friend bool operator==(const FPreprocessedTableKey& A, const FPreprocessedTableKey& B)
	{
		return A.TableName == B.TableName;
	}
};

FORCEINLINE uint32 GetTypeHash(const FPreprocessedTableKey& Key)
{
	return GetTypeHash(Key.TableName);
}

using FPreprocessedTableDataMap = TMap<FPreprocessedTableKey, TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>>;

struct FInboundRawMessage
{
	uint64 ConnectionEpoch = 0;
	uint64 SequenceId = 0;
	int32 QueueDepthAtEnqueue = 0;
	int64 QueuedBytesAtEnqueue = 0;
	TArray<uint8> Payload;
};

struct FInboundParsedMessage
{
	uint64 ConnectionEpoch = 0;
	uint64 SequenceId = 0;
	int32 PayloadSizeBytes = 0;
	uint8 CompressionTag = 0;
	int32 QueueDepthAtEnqueue = 0;
	int64 QueuedBytesAtEnqueue = 0;
	bool bProtocolError = false;
	FString ProtocolError;
	FServerMessageType Message;
	FPreprocessedTableDataMap PreprocessedTableData;
};

struct FSpacetimeDBTableApplyStats
{
	FString TableName;
	int32 RowSetCount = 0;
	int32 InsertRowCount = 0;
	int32 DeleteRowCount = 0;
	int64 InsertRowBytes = 0;
	int64 DeleteRowBytes = 0;
	double CacheMicros = 0.0;
	double BroadcastMicros = 0.0;
	bool bProducedDiff = false;
};

struct FSpacetimeDBInboundMessageApplyStats
{
	FString MessageKind;
	FString ReducerName;
	uint32 RequestId = 0;
	uint64 SequenceId = 0;
	int32 PayloadSizeBytes = 0;
	int32 QueueDepthAtEnqueue = 0;
	int64 QueuedBytesAtEnqueue = 0;
	TArray<FSpacetimeDBTableApplyStats> TableStats;
};

USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FSpacetimeDBInboundApplyBudget
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	int32 MaxMessagesPerFrame = 256;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	int64 MaxPayloadBytesPerFrame = 4 * 1024 * 1024;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	int32 MinMessagesPerFrame = 1;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	int64 SoftTimeBudgetMicros = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
	bool bDrainAllPendingMessages = false;

	static FSpacetimeDBInboundApplyBudget MakeDrainAllPendingMessages()
	{
		FSpacetimeDBInboundApplyBudget Budget;
		Budget.bDrainAllPendingMessages = true;
		return Budget;
	}

	void Sanitize()
	{
		MaxMessagesPerFrame = FMath::Max(1, MaxMessagesPerFrame);
		MaxPayloadBytesPerFrame = FMath::Max<int64>(1, MaxPayloadBytesPerFrame);
		MinMessagesPerFrame = FMath::Clamp(MinMessagesPerFrame, 1, MaxMessagesPerFrame);
		SoftTimeBudgetMicros = FMath::Max<int64>(0, SoftTimeBudgetMicros);
	}
};

template<typename T, typename = void>
struct THasOnDeleteDelegate : std::false_type
{
};

template<typename T>
struct THasOnDeleteDelegate<T, std::void_t<decltype(&T::OnDelete)>> : std::true_type
{
};

template<typename T, typename = void>
struct THasOnUpdateDelegate : std::false_type
{
};

template<typename T>
struct THasOnUpdateDelegate<T, std::void_t<decltype(&T::OnUpdate)>> : std::true_type
{
};

template<typename Type>
const void* GetNativeTableListenerTypeId()
{
	static const uint8 TypeId = 0;
	return &TypeId;
}

struct FNativeTableListenerBinding
{
	using FInsertThunk = void(*)(void* Owner, const void* Context, const void* Row);
	using FUpdateThunk = void(*)(void* Owner, const void* Context, const void* OldRow, const void* NewRow);
	using FDeleteThunk = void(*)(void* Owner, const void* Context, const void* Row);
	using FDiffThunk = void(*)(void* Owner, const void* Context, const void* Diff);

	void* Owner = nullptr;
	const void* RowTypeId = nullptr;
	const void* EventContextTypeId = nullptr;
	FInsertThunk InsertThunk = nullptr;
	FUpdateThunk UpdateThunk = nullptr;
	FDeleteThunk DeleteThunk = nullptr;
	FDiffThunk DiffThunk = nullptr;

	bool IsComplete() const
	{
		return Owner != nullptr &&
			RowTypeId != nullptr &&
			EventContextTypeId != nullptr &&
			(DiffThunk != nullptr ||
				(InsertThunk != nullptr &&
					UpdateThunk != nullptr &&
					DeleteThunk != nullptr));
	}
};

UCLASS()
class SPACETIMEDBSDK_API UDbConnectionBase : public UObject, public FTickableGameObject
{
	GENERATED_BODY()

public:

	/** The default constructor is private to prevent instantiation without using the builder. */
	explicit UDbConnectionBase(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());

	virtual void BeginDestroy() override;

	/** Disconnect from the server. */
	UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
	void Disconnect();

	/** Check if the underlying WebSocket is connected. */
	UFUNCTION(BlueprintPure, Category="SpacetimeDB")
	bool IsActive() const;

	UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
	void FrameTick();

	UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
	void SetAutoTicking(bool bAutoTick) { bIsAutoTicking = bAutoTick; }

	UFUNCTION(BlueprintCallable, Category="SpacetimeDB")
	void SetInboundApplyBudget(FSpacetimeDBInboundApplyBudget InBudget)
	{
		InBudget.Sanitize();
		InboundApplyBudget = InBudget;
	}

	/** Send a raw JSON message to the server. */
	bool SendRawMessage(const FString& Message);
	/** Send a raw binary message to the server. */
	bool SendRawMessage(const TArray<uint8>& Message);

	/** Get the current subscription builder. This is used to create subscriptions. */
	UFUNCTION()
	USubscriptionBuilderBase* SubscriptionBuilderBase();

	/** Get the current identity of the SpacetimeDB instance. This is used to identify the connection. */
	UFUNCTION(BlueprintPure, Category = "SpacetimeDB")
	bool TryGetIdentity(FSpacetimeDBIdentity& OutIdentity) const;

	/** Get the current connection id. This is used to identify the connection. */
	UFUNCTION(BlueprintPure, Category = "SpacetimeDB")
	FSpacetimeDBConnectionId GetConnectionId() const;

	// Typed reducer call helper: hides BSATN bytes from callers.
	template<typename ArgsStruct>
	uint32 CallReducerTyped(const FString& Reducer, const ArgsStruct& Args)
	{
		TArray<uint8> Bytes = UE::SpacetimeDB::Serialize(Args);
		return InternalCallReducer(Reducer, MoveTemp(Bytes));
	}

	template<typename ArgsStruct>
	void CallProcedureTyped(const FString& ProcedureName, const ArgsStruct& Args, const FOnProcedureCompleteDelegate& Callback)
	{
		TArray<uint8> Bytes = UE::SpacetimeDB::Serialize(Args);
		InternalCallProcedure(ProcedureName, MoveTemp(Bytes), Callback);
	}

	template<typename RowType>
	void RegisterTable(const FString& TableName)
	{
		FScopeLock Lock(&TableDeserializersMutex);
		TableDeserializers.Add(TableName, MakeShared<UE::SpacetimeDB::TTableRowDeserializer<RowType>>());
	}

	/** Internal interface for applying table updates generically */
	class ITableUpdateHandler
	{
	public:
		virtual ~ITableUpdateHandler() {}

		/** Update the in-memory cache for the table and store the diff */
		virtual bool UpdateCache(UDbConnectionBase* Conn, const FTableUpdateType& Update, void* Context, FSpacetimeDBTableApplyStats* OutStats) = 0;

		/** Broadcast the previously stored diff */
		virtual void BroadcastDiff(UDbConnectionBase* Conn, void* Context) = 0;
		virtual const FString& GetTableName() const = 0;
		virtual void RegisterNativeListener(const FNativeTableListenerBinding& Binding) = 0;
		virtual void UnregisterNativeListener(void* Owner) = 0;
	};

	template<typename RowType, typename TableClass, typename EventContext>
	class TTableUpdateHandler : public ITableUpdateHandler
	{
	public:
		explicit TTableUpdateHandler(const FString& InTableName, TableClass* InTable)
			: TableName(InTableName)
			, Table(InTable)
		{
		}

		//** Update the in-memory cache for the table and store the diff */
		virtual bool UpdateCache(UDbConnectionBase* Conn, const FTableUpdateType& Update, void* Context, FSpacetimeDBTableApplyStats* OutStats) override
		{
			if (PendingDiffReadIndex == PendingDiffs.Num())
			{
				PendingDiffs.Reset();
				PendingDiffReadIndex = 0;
			}

			TSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>> Pre;
			const bool bTookPreprocessedData = Conn->TakePreprocessedTableData<RowType>(Update, Pre);
			checkf(bTookPreprocessedData && Pre.IsValid(), TEXT("Missing message-scoped preprocessed data for table '%s'."), *Update.TableName);
			if (OutStats != nullptr)
			{
				OutStats->TableName = TableName;
				OutStats->RowSetCount = Pre->RowSetCount;
				OutStats->InsertRowCount = Pre->InsertRowCount;
				OutStats->DeleteRowCount = Pre->DeleteRowCount;
				OutStats->InsertRowBytes = Pre->InsertRowBytes;
				OutStats->DeleteRowBytes = Pre->DeleteRowBytes;
			}
			FTableAppliedDiff<RowType> AppliedDiff = Table->Update(MoveTemp(Pre->Inserts), MoveTemp(Pre->Deletes));
			if (AppliedDiff.IsEmpty())
			{
				if (OutStats != nullptr)
				{
					OutStats->bProducedDiff = false;
				}
				return false;
			}
			if (OutStats != nullptr)
			{
				OutStats->bProducedDiff = true;
			}
			PendingDiffs.Add(MoveTemp(AppliedDiff));
			return true;
		}
		//** Broadcast the last stored diff to the table's delegates */
		virtual void BroadcastDiff(UDbConnectionBase* Conn, void* Context) override
		{
			checkf(PendingDiffReadIndex < PendingDiffs.Num(), TEXT("Missing pending SpacetimeDB table diff for broadcast."));
			EventContext& Ctx = *reinterpret_cast<EventContext*>(Context);
			const FTableAppliedDiff<RowType>& Diff = PendingDiffs[PendingDiffReadIndex];
			if (!NativeListeners.IsEmpty())
			{
				BroadcastNativeDiff(Diff, Ctx);
			}
			else
			{
				Conn->BroadcastDiff(Table, Diff, Ctx);
			}
			++PendingDiffReadIndex;
			if (PendingDiffReadIndex == PendingDiffs.Num())
			{
				PendingDiffs.Reset();
				PendingDiffReadIndex = 0;
			}
		}

		virtual const FString& GetTableName() const override
		{
			return TableName;
		}

		virtual void RegisterNativeListener(const FNativeTableListenerBinding& Binding) override
		{
			checkf(!bBroadcastingNativeListeners,
				TEXT("Cannot register native SpacetimeDB table listener during broadcast for table '%s'."),
				*TableName);
			checkf(Binding.IsComplete(), TEXT("Incomplete native SpacetimeDB table listener for table '%s'."), *TableName);
			checkf(Binding.RowTypeId == GetNativeTableListenerTypeId<RowType>(),
				TEXT("Native SpacetimeDB table listener row type mismatch for table '%s'."), *TableName);
			checkf(Binding.EventContextTypeId == GetNativeTableListenerTypeId<EventContext>(),
				TEXT("Native SpacetimeDB table listener context type mismatch for table '%s'."), *TableName);
			for (const FNativeTableListenerBinding& ExistingBinding : NativeListeners)
			{
				checkf(ExistingBinding.Owner != Binding.Owner,
					TEXT("Duplicate native SpacetimeDB table listener owner for table '%s'."),
					*TableName);
			}
			NativeListeners.Add(Binding);
		}

		virtual void UnregisterNativeListener(void* Owner) override
		{
			checkf(!bBroadcastingNativeListeners,
				TEXT("Cannot unregister native SpacetimeDB table listener during broadcast for table '%s'."),
				*TableName);
			checkf(Owner != nullptr, TEXT("Cannot unregister null native SpacetimeDB table listener owner for table '%s'."), *TableName);
			const int32 ListenerIndex = NativeListeners.IndexOfByPredicate(
				[Owner](const FNativeTableListenerBinding& Binding)
				{
					return Binding.Owner == Owner;
				});
			checkf(ListenerIndex != INDEX_NONE,
				TEXT("Missing native SpacetimeDB table listener for table '%s'."),
				*TableName);
			NativeListeners.RemoveAtSwap(ListenerIndex, 1, EAllowShrinking::No);
		}

	private:
		void BroadcastNativeDiff(const FTableAppliedDiff<RowType>& Diff, const EventContext& Context)
		{
			TGuardValue<bool> BroadcastingScope(bBroadcastingNativeListeners, true);
			for (const FNativeTableListenerBinding& Listener : NativeListeners)
			{
				BroadcastNativeDiffToListener(Diff, Context, Listener);
			}
		}

		void BroadcastNativeDiffToListener(
			const FTableAppliedDiff<RowType>& Diff,
			const EventContext& Context,
			const FNativeTableListenerBinding& Listener)
		{
			checkf(Listener.IsComplete(), TEXT("Incomplete native SpacetimeDB table listener for table '%s'."), *TableName);
			if (Listener.DiffThunk != nullptr)
			{
				Listener.DiffThunk(Listener.Owner, &Context, &Diff);
				return;
			}

			for (const TSharedPtr<RowType>& Row : Diff.Inserts)
			{
				checkf(Row.IsValid(), TEXT("Invalid SpacetimeDB native insert diff row for table '%s'."), *TableName);
				Listener.InsertThunk(Listener.Owner, &Context, Row.Get());
			}

			for (const TSharedPtr<RowType>& Row : Diff.Deletes)
			{
				checkf(Row.IsValid(), TEXT("Invalid SpacetimeDB native delete diff row for table '%s'."), *TableName);
				Listener.DeleteThunk(Listener.Owner, &Context, Row.Get());
			}

			checkf(Diff.UpdateDeletes.Num() == Diff.UpdateInserts.Num(),
				TEXT("Mismatched SpacetimeDB native update diff counts for table '%s'."), *TableName);
			for (int32 Index = 0; Index < Diff.UpdateInserts.Num(); ++Index)
			{
				const TSharedPtr<RowType>& OldRow = Diff.UpdateDeletes[Index];
				const TSharedPtr<RowType>& NewRow = Diff.UpdateInserts[Index];
				checkf(OldRow.IsValid() && NewRow.IsValid(), TEXT("Invalid SpacetimeDB native update diff row for table '%s'."), *TableName);
				Listener.UpdateThunk(Listener.Owner, &Context, OldRow.Get(), NewRow.Get());
			}
		}

		FString TableName;
		TableClass* Table;
		TArray<FTableAppliedDiff<RowType>> PendingDiffs;
		int32 PendingDiffReadIndex = 0;
		TArray<FNativeTableListenerBinding> NativeListeners;
		bool bBroadcastingNativeListeners = false;
	};
	//** Register a table with the connection. This will allow the connection to handle updates for the table.
	template<typename RowType, typename TableClass, typename EventContext>
	void RegisterTable(const FString& TableName, TableClass* Table)
	{
		RegisterTable<RowType>(TableName);
		FScopeLock Lock(&RegisteredTablesMutex);
		RegisteredTables.Add(TableName, MakeShared<TTableUpdateHandler<RowType, TableClass, EventContext>>(TableName, Table));
		RegisteredTablesSnapshot = MakeShared<TMap<FString, TSharedPtr<ITableUpdateHandler>>>(RegisteredTables);
	}

	template<typename RowType, typename EventContext, typename OwnerType,
		void (OwnerType::*InsertFn)(const EventContext&, const RowType&),
		void (OwnerType::*UpdateFn)(const EventContext&, const RowType&, const RowType&),
		void (OwnerType::*DeleteFn)(const EventContext&, const RowType&)>
	void RegisterNativeTableListener(const FString& TableName, OwnerType* Owner)
	{
		static_assert(std::is_base_of_v<UObject, OwnerType>, "Native SpacetimeDB table listener owner must derive from UObject.");
		checkf(Owner != nullptr, TEXT("Cannot register null native SpacetimeDB table listener owner for table '%s'."), *TableName);

		FNativeTableListenerBinding Binding;
		Binding.Owner = Owner;
		Binding.RowTypeId = GetNativeTableListenerTypeId<RowType>();
		Binding.EventContextTypeId = GetNativeTableListenerTypeId<EventContext>();
		Binding.InsertThunk = [](void* RawOwner, const void* RawContext, const void* RawRow)
		{
			(static_cast<OwnerType*>(RawOwner)->*InsertFn)(
				*static_cast<const EventContext*>(RawContext),
				*static_cast<const RowType*>(RawRow));
		};
		Binding.UpdateThunk = [](void* RawOwner, const void* RawContext, const void* RawOldRow, const void* RawNewRow)
		{
			(static_cast<OwnerType*>(RawOwner)->*UpdateFn)(
				*static_cast<const EventContext*>(RawContext),
				*static_cast<const RowType*>(RawOldRow),
				*static_cast<const RowType*>(RawNewRow));
		};
		Binding.DeleteThunk = [](void* RawOwner, const void* RawContext, const void* RawRow)
		{
			(static_cast<OwnerType*>(RawOwner)->*DeleteFn)(
				*static_cast<const EventContext*>(RawContext),
				*static_cast<const RowType*>(RawRow));
		};

		FScopeLock Lock(&RegisteredTablesMutex);
		TSharedPtr<ITableUpdateHandler>* Handler = RegisteredTables.Find(TableName);
		checkf(Handler != nullptr && Handler->IsValid(),
			TEXT("Missing SpacetimeDB table handler while registering native listener for table '%s'."), *TableName);
		(*Handler)->RegisterNativeListener(Binding);
	}

	template<typename RowType, typename EventContext, typename OwnerType,
		void (OwnerType::*DiffFn)(const EventContext&, const FTableAppliedDiff<RowType>&)>
	void RegisterNativeTableDiffListener(const FString& TableName, OwnerType* Owner)
	{
		static_assert(std::is_base_of_v<UObject, OwnerType>, "Native SpacetimeDB table diff listener owner must derive from UObject.");
		checkf(Owner != nullptr, TEXT("Cannot register null native SpacetimeDB table diff listener owner for table '%s'."), *TableName);

		FNativeTableListenerBinding Binding;
		Binding.Owner = Owner;
		Binding.RowTypeId = GetNativeTableListenerTypeId<RowType>();
		Binding.EventContextTypeId = GetNativeTableListenerTypeId<EventContext>();
		Binding.DiffThunk = [](void* RawOwner, const void* RawContext, const void* RawDiff)
		{
			(static_cast<OwnerType*>(RawOwner)->*DiffFn)(
				*static_cast<const EventContext*>(RawContext),
				*static_cast<const FTableAppliedDiff<RowType>*>(RawDiff));
		};

		FScopeLock Lock(&RegisteredTablesMutex);
		TSharedPtr<ITableUpdateHandler>* Handler = RegisteredTables.Find(TableName);
		checkf(Handler != nullptr && Handler->IsValid(),
			TEXT("Missing SpacetimeDB table handler while registering native diff listener for table '%s'."), *TableName);
		(*Handler)->RegisterNativeListener(Binding);
	}

	template<typename RowType, typename EventContext, typename OwnerType,
		void (OwnerType::*InsertFn)(const EventContext&, const RowType&),
		void (OwnerType::*UpdateFn)(const EventContext&, const RowType&, const RowType&),
		void (OwnerType::*DeleteFn)(const EventContext&, const RowType&)>
	void UnregisterNativeTableListener(const FString& TableName, OwnerType* Owner)
	{
		static_assert(std::is_base_of_v<UObject, OwnerType>, "Native SpacetimeDB table listener owner must derive from UObject.");
		checkf(Owner != nullptr, TEXT("Cannot unregister null native SpacetimeDB table listener owner for table '%s'."), *TableName);

		FScopeLock Lock(&RegisteredTablesMutex);
		TSharedPtr<ITableUpdateHandler>* Handler = RegisteredTables.Find(TableName);
		checkf(Handler != nullptr && Handler->IsValid(),
			TEXT("Missing SpacetimeDB table handler while unregistering native listener for table '%s'."), *TableName);
		(*Handler)->UnregisterNativeListener(Owner);
	}

	template<typename RowType, typename EventContext, typename OwnerType,
		void (OwnerType::*DiffFn)(const EventContext&, const FTableAppliedDiff<RowType>&)>
	void UnregisterNativeTableDiffListener(const FString& TableName, OwnerType* Owner)
	{
		static_assert(std::is_base_of_v<UObject, OwnerType>, "Native SpacetimeDB table diff listener owner must derive from UObject.");
		checkf(Owner != nullptr, TEXT("Cannot unregister null native SpacetimeDB table diff listener owner for table '%s'."), *TableName);

		FScopeLock Lock(&RegisteredTablesMutex);
		TSharedPtr<ITableUpdateHandler>* Handler = RegisteredTables.Find(TableName);
		checkf(Handler != nullptr && Handler->IsValid(),
			TEXT("Missing SpacetimeDB table handler while unregistering native diff listener for table '%s'."), *TableName);
		(*Handler)->UnregisterNativeListener(Owner);
	}
	//** Take preprocessed table row data. */
	template<typename RowType>
	bool TakePreprocessedTableData(const FTableUpdateType& Update, TSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>>& OutData)
	{
		checkf(ActivePreprocessedTableData != nullptr, TEXT("No active inbound message while applying table update '%s'."), *Update.TableName);
		FPreprocessedTableKey Key(Update.TableName);
		TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>* Found = ActivePreprocessedTableData->Find(Key);
		checkf(Found != nullptr && Found->Num() > 0, TEXT("Missing message-scoped preprocessed data for table '%s'."), *Update.TableName);
		OutData = StaticCastSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>>((*Found)[0]);
		Found->RemoveAt(0, 1, EAllowShrinking::No);
		if (Found->Num() == 0)
		{
			ActivePreprocessedTableData->Remove(Key);
		}
		checkf(OutData.IsValid(), TEXT("Invalid message-scoped preprocessed data for table '%s'."), *Update.TableName);
		return true;
	}


protected:

	friend class UDbConnectionBuilderBase;
	friend class UDbConnectionBuilder;
	friend class FSpacetimeDbInboundWorker;
	friend class UWebsocketManager;
	friend class USubscriptionHandleBase;
	friend class USubscriptionBuilder;
	friend class URemoteReducers;

	/** Allow derived classes to override the delegates used when connecting */
	void SetOnConnectDelegate(const FOnConnectBaseDelegate& Delegate) { OnConnectBaseDelegate = Delegate; }
	void SetOnDisconnectDelegate(const FOnDisconnectBaseDelegate& Delegate) { OnDisconnectBaseDelegate = Delegate; }

	UFUNCTION()
	void HandleWSError(const FString& Error);
	UFUNCTION()
	void HandleWSClosed(int32 StatusCode, const FString& Reason, bool bWasClean);
	UFUNCTION()
	void HandleWSBinaryMessage(const TArray<uint8>& Message);
	void HandleWSBinaryMessageOwned(TArray<uint8>&& Message);
	void StartInboundMessageWorker();
	void StopInboundMessageWorker();
	void ClearInboundMessageQueues();
	void NotifyInboundWorkerIfNeeded();
	void DrainInboundRawMessagesOnWorker();
	bool BuildInboundParsedMessage(const FInboundRawMessage& RawMessage, FInboundParsedMessage& OutMessage);
	void EnqueueInboundProtocolError(uint64 SequenceId, int32 PayloadSizeBytes, uint8 CompressionTag, int32 QueueDepthAtEnqueue, int64 QueuedBytesAtEnqueue, const FString& ErrorMessage);
	bool IsInboundProtocolErrorQueued() const;
	bool IsInboundEpochCurrentAndAccepting(uint64 ConnectionEpoch) const;
	void MarkInboundProtocolErrorQueued();

	virtual void Tick(float DeltaTime) override;

	virtual TStatId GetStatId() const override;

	virtual bool IsTickable() const override;

	virtual bool IsTickableInEditor() const override;

	/** Internal handler that processes a single server message. */
	void ProcessServerMessage(const FServerMessageType& Message);
	void ProcessInboundServerMessage(FInboundParsedMessage& InboundMessage, FSpacetimeDBInboundMessageApplyStats& ApplyStats);
	void PreProcessTableUpdateRows(const FString& TableName, const TArray<FTableUpdateRowsType>& RowSets, FPreprocessedTableDataMap& OutPreprocessedTableData);
	void PreProcessQueryRows(const FQueryRowsType& Rows, UE::SpacetimeDB::EQueryRowsApplyMode Mode, FPreprocessedTableDataMap& OutPreprocessedTableData);
	void PreProcessTransactionUpdate(const FTransactionUpdateType& Update, FPreprocessedTableDataMap& OutPreprocessedTableData);
	TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer> FindTableDeserializerForPreprocess(const FString& TableName);
	void StorePreprocessedTableData(const FString& TableName, TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase> Data, FPreprocessedTableDataMap& OutPreprocessedTableData);
	/** Decompress and parse a raw message. */
	bool PreProcessMessage(const TArray<uint8>& Message, FInboundParsedMessage& OutMessage);
	bool DecompressGzip(const uint8* InData, int32 InSize, TArray<uint8>& OutData);
	bool DecompressBrotli(const uint8* InData, int32 InSize, TArray<uint8>& OutData);
	void ClearPendingOperations(const FString& Reason);
	void HandleProtocolViolation(const FString& ErrorMessage);

	/** Parsed inbound messages awaiting processing on the game thread. */
	TArray<FInboundParsedMessage> PendingMessages;

	/** Mutex protecting access to PendingMessages. */
	FCriticalSection PendingMessagesMutex;
	int32 PendingMessageReadIndex = 0;
	int64 PendingParsedPayloadBytes = 0;

	/** Raw inbound messages awaiting FIFO processing by the connection-owned worker. */
	TArray<FInboundRawMessage> InboundRawMessages;
	mutable FCriticalSection InboundRawMessagesMutex;
	int64 InboundQueuedRawBytes = 0;
	uint64 InboundConnectionEpoch = 0;
	uint64 NextInboundSequenceId = 0;
	bool bInboundAcceptingMessages = false;
	bool bInboundProtocolErrorQueued = false;
	FSpacetimeDbInboundWorker* InboundWorker = nullptr;
	FCriticalSection InboundWorkerMutex;

	// Map of table name to row deserializer
	TMap<FString, TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer>> TableDeserializers;
	FCriticalSection TableDeserializersMutex;

	// Message-scoped preprocessed table rows active only while applying one inbound server message.
	FPreprocessedTableDataMap* ActivePreprocessedTableData = nullptr;

	// Map of table name to generic table update handler
	TMap<FString, TSharedPtr<ITableUpdateHandler>> RegisteredTables;
	TSharedPtr<const TMap<FString, TSharedPtr<ITableUpdateHandler>>> RegisteredTablesSnapshot;
	FCriticalSection RegisteredTablesMutex;


	/** Start a subscription. This will add the subscription to the active list and send a subscribe message to the server. */
	void StartSubscription(USubscriptionHandleBase* Handle);
	/** Unsubscribe from a subscription. This will remove the subscription from the active list and send an unsubscribe message to the server. */
	void UnsubscribeInternal(USubscriptionHandleBase* Handle);

	/** Call a reducer on the connected SpacetimeDB instance. */
	uint32 InternalCallReducer(const FString& Reducer, TArray<uint8> Args);

	/** Call a reducer on the connected SpacetimeDB instance. */
	void InternalCallProcedure(const FString& ProcedureName, TArray<uint8> Args, const FOnProcedureCompleteDelegate& Callback);

	/**
	* Update function to apply database changes.
	* Must be implemented by child classes.
	* @param Update - Struct containing update data.
	*/
	virtual void DbUpdate(const FDatabaseUpdateType& Update, const FSpacetimeDBEvent& Event) {};

	/** Event handler for reducer events. This can should overridden by child classes to handle specific reducer events. */
	virtual void ReducerEvent(const FReducerEvent& Event) {};

	/** Event handler for reducer events. This can should overridden by child classes to handle specific reducer events. */
	virtual void ReducerEventFailed(const FReducerEvent& Event, const FString ErrorMessage) {};

	/** Event handler for procedure events. This can should overridden by child classes to handle specific procedure events. */
	virtual void ProcedureEventFailed(const FProcedureEvent& Event, const FString ErrorMessage) {};

	/** Event handler for error events. This can should overridden by child classes to handle specific error events. */
	virtual void TriggerError(const FString& ErrorMessage) {};

	/** Event handler for subscription events. This can should overridden by child classes to handle specific subscription events. */
	virtual void TriggerSubscription() {};

	/** Apply updates for all registered tables using the provided context pointer */
	void ApplyRegisteredTableUpdates(const FDatabaseUpdateType& Update, void* Context);

	/** Called when a subscription is updated. */
	UPROPERTY()
	TMap<uint32, TObjectPtr<USubscriptionHandleBase>> ActiveSubscriptions;

	/** Pending reducer call metadata keyed by request id for ReducerResult correlation. */
	UPROPERTY()
	TMap<uint32, FReducerCallInfoType> PendingReducerCalls;

	UPROPERTY()
	TObjectPtr<UProcedureCallbacks> ProcedureCallbacks;
	/** Get the next request id for a message. This is used to track requests and responses. */
	uint32 NextRequestId;
	/** Get the next subscription id for a subscription. This is used to track subscriptions and their responses. */
	uint32 NextSubscriptionId;
	/** Get the next request id for a message. This is used to track requests and responses. */
	uint32 GetNextRequestId();
	/** Get the next subscription id for a subscription. This is used to track subscriptions and their responses. */
	uint32 GetNextSubscriptionId();

	/** The WebSocket manager used to connect to the server. */
	UPROPERTY()
	UWebsocketManager* WebSocket = nullptr;

	/** The URI of the SpacetimeDB server to connect to. */
	UPROPERTY()
	FString Uri;
	/** The module name to connect to. This is used to identify the SpacetimeDB instance. */
	UPROPERTY()
	FString ModuleName;
	/** The token used to authenticate the connection. */
	UPROPERTY()
	FString Token;

	/** The identity of the SpacetimeDB instance. This is used to identify the connection. */
	UPROPERTY()
	FSpacetimeDBIdentity Identity;
	UPROPERTY()
	/** Whether the identity has been set. This is used to prevent multiple identity sets. */
	bool bIsIdentitySet = false;
	/** The connection id of the SpacetimeDB instance. This is used to identify the connection. */
	UPROPERTY()
	FSpacetimeDBConnectionId ConnectionId;

	UPROPERTY()
	bool bIsAutoTicking = false;

	FSpacetimeDBInboundApplyBudget InboundApplyBudget;
	struct FPendingTableBroadcast
	{
		TSharedPtr<ITableUpdateHandler> Handler;
		int32 StatsIndex = INDEX_NONE;
	};
	TArray<FPendingTableBroadcast> TableUpdateHandlersScratch;
	FSpacetimeDBInboundMessageApplyStats* ActiveInboundMessageApplyStats = nullptr;

	/** Guard to avoid repeatedly handling the same fatal protocol error. */
	FThreadSafeBool bProtocolViolationHandled = false;

	UPROPERTY()
	FOnConnectErrorDelegate OnConnectErrorDelegate;
	UPROPERTY()
	FOnDisconnectBaseDelegate OnDisconnectBaseDelegate;
	UPROPERTY()
	FOnConnectBaseDelegate OnConnectBaseDelegate;

	/** Called when the connection is established. */
	template <typename TableClass, typename RowType, typename EventContext>
	void BroadcastDiff(TableClass* Table, const FTableAppliedDiff<RowType>& Diff, const EventContext& Context)
	{
		if (!Table) return;

		// Broadcast the diff to the table's delegates
		if (Table->OnInsert.IsBound())
		{
			for (const TSharedPtr<RowType>& Row : Diff.Inserts)
			{
				checkf(Row.IsValid(), TEXT("Invalid SpacetimeDB insert diff row."));
				Table->OnInsert.Broadcast(Context, *Row);
			}
		}

		// Event tables intentionally omit delete/update delegates.
		if constexpr (THasOnDeleteDelegate<TableClass>::value)
		{
			if (Table->OnDelete.IsBound())
			{
				for (const TSharedPtr<RowType>& Row : Diff.Deletes)
				{
					checkf(Row.IsValid(), TEXT("Invalid SpacetimeDB delete diff row."));
					Table->OnDelete.Broadcast(Context, *Row);
				}
			}
		}

		if constexpr (THasOnUpdateDelegate<TableClass>::value)
		{
			if (Table->OnUpdate.IsBound())
			{
				checkf(Diff.UpdateDeletes.Num() == Diff.UpdateInserts.Num(), TEXT("Mismatched SpacetimeDB update diff counts."));
				for (int32 Index = 0; Index < Diff.UpdateInserts.Num(); ++Index)
				{
					const TSharedPtr<RowType>& OldRow = Diff.UpdateDeletes[Index];
					const TSharedPtr<RowType>& NewRow = Diff.UpdateInserts[Index];
					checkf(OldRow.IsValid() && NewRow.IsValid(), TEXT("Invalid SpacetimeDB update diff row."));
					Table->OnUpdate.Broadcast(Context, *OldRow, *NewRow);
				}
			}
		}
	}
};
