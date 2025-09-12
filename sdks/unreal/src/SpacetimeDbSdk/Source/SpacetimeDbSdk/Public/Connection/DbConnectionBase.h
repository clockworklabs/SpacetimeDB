#pragma once

#include "CoreMinimal.h"
#include "UObject/NoExportTypes.h"
#include "Types/Builtins.h"
#include "Websocket.h"
#include "Subscription.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "DBCache/TableAppliedDiff.h"
#include "HAL/CriticalSection.h"
#include "Containers/Queue.h"
#include "HAL/ThreadSafeBool.h"
#include "BSATN/UEBSATNHelpers.h"
#include "Connection/SetReducerFlags.h"
#include "Connection/Callback.h"

#include "DbConnectionBase.generated.h"



class UDbConnectionBuilder;

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


/** Key used to index preprocessed table data without relying on row addresses */
struct FPreprocessedTableKey
{
	uint32 TableId;
	FString TableName;

	FPreprocessedTableKey() : TableId(0) {}
	FPreprocessedTableKey(uint32 InId, const FString& InName)
		: TableId(InId), TableName(InName) {
	}

	friend bool operator==(const FPreprocessedTableKey& A, const FPreprocessedTableKey& B)
	{
		return A.TableId == B.TableId && A.TableName == B.TableName;
	}
};

FORCEINLINE uint32 GetTypeHash(const FPreprocessedTableKey& Key)
{
	return HashCombine(GetTypeHash(Key.TableId), GetTypeHash(Key.TableName));
}

UCLASS()
class SPACETIMEDBSDK_API UDbConnectionBase : public UObject, public FTickableGameObject
{
	GENERATED_BODY()

public:

	/** The default constructor is private to prevent instantiation without using the builder. */
	explicit UDbConnectionBase(const FObjectInitializer& ObjectInitializer = FObjectInitializer::Get());

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
	void CallReducerTyped(const FString& Reducer, const ArgsStruct& Args, USetReducerFlagsBase* Flags)
	{
		TArray<uint8> Bytes = UE::SpacetimeDB::Serialize(Args);
		InternalCallReducer(Reducer, MoveTemp(Bytes), Flags);
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
		virtual void UpdateCache(UDbConnectionBase* Conn, const FTableUpdateType& Update, void* Context) = 0;

		/** Broadcast the previously stored diff */
		virtual void BroadcastDiff(UDbConnectionBase* Conn, void* Context) = 0;
	};

	template<typename RowType, typename TableClass, typename EventContext>
	class TTableUpdateHandler : public ITableUpdateHandler
	{
	public:
		explicit TTableUpdateHandler(TableClass* InTable) : Table(InTable) {}

		//** Update the in-memory cache for the table and store the diff */
		virtual void UpdateCache(UDbConnectionBase* Conn, const FTableUpdateType& Update, void* Context) override
		{
			// Attempt to take preprocessed data if available
			TSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>> Pre;
			if (Conn->TakePreprocessedTableData<RowType>(Update, Pre))
			{
				// If preprocessed data is available, use it to update the table
				LastDiff = Table->Update(Pre->Inserts, Pre->Deletes);
			}
			else
			{
				// If no preprocessed data, process the update directly. Backup
				UE_LOG(LogTemp, Warning, TEXT("No preprocessed data for table update. Processing directly."));
				TArray<FWithBsatn<RowType>> Inserts, Deletes;
				UE::SpacetimeDB::ProcessTableUpdateWithBsatn<RowType>(Update, Inserts, Deletes);
				LastDiff = Table->Update(Inserts, Deletes);
			}
		}
		//** Broadcast the last stored diff to the table's delegates */
		virtual void BroadcastDiff(UDbConnectionBase* Conn, void* Context) override
		{
			EventContext& Ctx = *reinterpret_cast<EventContext*>(Context);
			Conn->BroadcastDiff(Table, LastDiff, Ctx);
		}

	private:
		TableClass* Table;
		FTableAppliedDiff<RowType> LastDiff;
	};
	//** Register a table with the connection. This will allow the connection to handle updates for the table.
	template<typename RowType, typename TableClass, typename EventContext>
	void RegisterTable(const FString& TableName, TableClass* Table)
	{
		RegisterTable<RowType>(TableName);
		FScopeLock Lock(&RegisteredTablesMutex);
		RegisteredTables.Add(TableName, MakeShared<TTableUpdateHandler<RowType, TableClass, EventContext>>(Table));
	}
	//** Take preprocessed table row data. */
	template<typename RowType>
	bool TakePreprocessedTableData(const FTableUpdateType& Update, TSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>>& OutData)
	{
		FScopeLock Lock(&PreprocessedDataMutex);
		FPreprocessedTableKey Key(Update.TableId, Update.TableName);
		if (TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>* Found = PreprocessedTableData.Find(Key))
		{
			if (Found->Num() > 0)
			{
				OutData = StaticCastSharedPtr<UE::SpacetimeDB::TPreprocessedTableData<RowType>>((*Found)[0]);
				Found->RemoveAt(0);
				if (Found->Num() == 0)
				{
					PreprocessedTableData.Remove(Key);
				}
				return OutData.IsValid();
			}
		}
		return false;
	}


protected:

	friend class UDbConnectionBuilderBase;
	friend class UDbConnectionBuilder;
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

	virtual void Tick(float DeltaTime) override;

	virtual TStatId GetStatId() const override;

	virtual bool IsTickable() const override;

	virtual bool IsTickableInEditor() const override;

	/** Internal handler that processes a single server message. */
	void ProcessServerMessage(const FServerMessageType& Message);
	void PreProcessDatabaseUpdate(const FDatabaseUpdateType& Update);
	/** Decompress and parse a raw message. */
	FServerMessageType PreProcessMessage(const TArray<uint8>& Message);
	bool DecompressPayload(ECompressableQueryUpdateTag Variant, const TArray<uint8>& In, TArray<uint8>& Out);
	bool DecompressGzip(const TArray<uint8>& InData, TArray<uint8>& OutData);
	bool DecompressBrotli(const TArray<uint8>& InData, TArray<uint8>& OutData);

	/** Pending messages awaiting processing on the game thread. */
	TArray<FServerMessageType> PendingMessages;

	/** Mutex protecting access to PendingMessages. */
	FCriticalSection PendingMessagesMutex;

	/** Map of preprocessed messages keyed by their sequential id. */
	TMap<int32, FServerMessageType> PreprocessedMessages;

	/** Protects PreprocessedMessages and PendingMessages ordering state. */
	FCriticalSection PreprocessMutex;

	/** Counter for assigning ids to incoming messages. */
	FThreadSafeCounter NextPreprocessId;

	/** Id of the next message expected to be released. */
	int32 NextReleaseId = 0;

	// Map of table name to row deserializer
	TMap<FString, TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer>> TableDeserializers;
	FCriticalSection TableDeserializersMutex;

	// Map from table update pointer to preprocessed data
	TMap<FPreprocessedTableKey, TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>> PreprocessedTableData;
	FCriticalSection PreprocessedDataMutex;

	// Map of table name to generic table update handler
	TMap<FString, TSharedPtr<ITableUpdateHandler>> RegisteredTables;
	FCriticalSection RegisteredTablesMutex;


	/** Start a subscription. This will add the subscription to the active list and send a subscribe message to the server. */
	void StartSubscription(USubscriptionHandleBase* Handle);
	/** Unsubscribe from a subscription. This will remove the subscription from the active list and send an unsubscribe message to the server. */
	void UnsubscribeInternal(USubscriptionHandleBase* Handle);

	/** Call a reducer on the connected SpacetimeDB instance. */
	void InternalCallReducer(const FString& Reducer, TArray<uint8> Args, USetReducerFlagsBase* Flags);

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

	/** Event handler for error events. This can should overridden by child classes to handle specific error events. */
	virtual void TriggerError(const FString& ErrorMessage) {};

	/** Event handler for subscription events. This can should overridden by child classes to handle specific subscription events. */
	virtual void TriggerSubscription() {};

	/** Apply updates for all registered tables using the provided context pointer */
	void ApplyRegisteredTableUpdates(const FDatabaseUpdateType& Update, void* Context);

	/** Called when a subscription is updated. */
	TMap<int32, TObjectPtr<USubscriptionHandleBase>> ActiveSubscriptions;

	/** Get the next request id for a message. This is used to track requests and responses. */
	int32 NextRequestId;
	/** Get the next subscription id for a subscription. This is used to track subscriptions and their responses. */
	int32 NextSubscriptionId;
	/** Get the next request id for a message. This is used to track requests and responses. */
	int32 GetNextRequestId();
	/** Get the next subscription id for a subscription. This is used to track subscriptions and their responses. */
	int32 GetNextSubscriptionId();

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

	FOnConnectErrorDelegate OnConnectErrorDelegate;
	FOnDisconnectBaseDelegate OnDisconnectBaseDelegate;
	FOnConnectBaseDelegate OnConnectBaseDelegate;

	/** Called when the connection is established. */
	template <typename TableClass, typename RowType, typename EventContext>
	void BroadcastDiff(TableClass* Table, const FTableAppliedDiff<RowType>& Diff, const EventContext& Context)
	{
		if (!Table) return;

		// Broadcast the diff to the table's delegates
		if (Table->OnInsert.IsBound())
		{
			for (const TPair<TArray<uint8>, RowType>& Pair : Diff.Inserts)
			{
				Table->OnInsert.Broadcast(Context, Pair.Value);
			}
		}

		// If the table has a delete delegate, broadcast deletes
		if (Table->OnDelete.IsBound())
		{
			for (const TPair<TArray<uint8>, RowType>& Pair : Diff.Deletes)
			{
				Table->OnDelete.Broadcast(Context, Pair.Value);
			}
		}

		// If the table has an update delegate, broadcast updates
		if (Table->OnUpdate.IsBound())
		{
			int32 Count = FMath::Min(Diff.UpdateDeletes.Num(), Diff.UpdateInserts.Num());
			for (int32 Index = 0; Index < Count; ++Index)
			{
				const RowType& OldRow = Diff.UpdateDeletes[Index];
				const RowType& NewRow = Diff.UpdateInserts[Index];
				Table->OnUpdate.Broadcast(Context, OldRow, NewRow);
			}
		}
	}
};
