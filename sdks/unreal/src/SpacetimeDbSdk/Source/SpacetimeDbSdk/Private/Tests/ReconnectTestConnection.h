#pragma once
#include "Connection/DbConnectionBase.h"
#include "DBCache/ClientCache.h"
#include "Tables/RemoteTable.h"
#include "UObject/StrongObjectPtr.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ReconnectTestConnection.generated.h"

struct FReconnectTestRow
{
	uint32 Id = 0;
	int32 Value = 0;
	bool operator==(const FReconnectTestRow& Other) const
	{
		return Id == Other.Id && Value == Other.Value;
	}
};
namespace UE::SpacetimeDB
{
	UE_SPACETIMEDB_STRUCT(FReconnectTestRow, Id, Value);
}

// Stands in for a generated table. Like generated code, its cache changes only through URemoteTable::BaseUpdate,
// so replay reads it the same way it reads real bindings.
UCLASS()
class UReconnectTestTable : public URemoteTable
{
	GENERATED_BODY()

public:
	static UReconnectTestTable* Create(const FString& InName)
	{
		UReconnectTestTable* Table = NewObject<UReconnectTestTable>();
		Table->Name = InName;
		Table->Data = MakeShared<UClientCache<FReconnectTestRow>>();
		Table->Data->GetOrAdd(InName)->AddUniqueConstraint<uint32>(TEXT("id"),
		                                                           [](const auto& Row)
		                                                           {
			                                                           return Row.Id;
		                                                           });
		return Table;
	}
	FString Name;
	TSharedPtr<UClientCache<FReconnectTestRow>> Data;
	DECLARE_MULTICAST_DELEGATE_TwoParams(FInsert, const FEventContextBase&, const FReconnectTestRow&);
	DECLARE_MULTICAST_DELEGATE_ThreeParams(FUpdate, const FEventContextBase&, const FReconnectTestRow&,
	                                       const FReconnectTestRow&);
	FInsert OnInsert, OnDelete;
	FUpdate OnUpdate;
	FTableAppliedDiff<FReconnectTestRow> Update(TArray<FWithBsatn<FReconnectTestRow>> Inserts,
	                                            TArray<FWithBsatn<FReconnectTestRow>> Deletes)
	{
		auto Diff = BaseUpdate<FReconnectTestRow>(Inserts, Deletes, Data, Name);
		Diff.DeriveUpdatesByPrimaryKey<uint32>(
		    [](const auto& Row)
		    {
			    return Row.Id;
		    });
		return Diff;
	}
};

UCLASS()
class UReconnectTestConnection : public UDbConnectionBase
{
	GENERATED_BODY()
public:
	int32 Connects = 0, Reconnects = 0, Disconnects = 0, Errors = 0, Starts = 0, Applied = 0, ProviderCalls = 0;
	int32 LegacyDisconnects = 0, LegacyErrors = 0;
	bool bLastRetry = false, bFailProvider = false, bHoldProvider = false, bCancelOnReconnect = false;
	int32 LastAttempt = 0;
	float LastDelay = 0;
	bool bLive = false;
	FString SentToken, ProviderToken;
	void StartLive(const FString& InUri, const FString& InDatabase);
	void DropSocket()
	{
		WebSocket->Disconnect();
		QueueFailure(TEXT("test socket dropped"), false);
	}
	FString CurrentToken() const
	{
		return Token;
	}
	void DelayRecovery()
	{
		ReconnectAt = FPlatformTime::Seconds() + 3600;
	}
	TArray<FClientMessageType> Sent;
	TArray<ESpacetimeDBCallError> CallErrors;
	TArray<ESpacetimeDBCallError> FailedCallErrors;
	UPROPERTY()
	TObjectPtr<UReconnectTokenRequest> HeldRequest;

	void Initialize(bool bEnable = true);
	void ConnectIdentity(const FString& Hex = TEXT("1111111111111111111111111111111111111111111111111111111111111111"),
	                     const FString& NewToken = TEXT("retained"));
	void Drop(bool bTerminal = false, bool bAuth = false)
	{
		QueueFailure(TEXT("test socket dropped"), bTerminal, bAuth);
		FrameTick();
	}
	/** Close or error through the real socket handlers, without calling FrameTick. */
	void SocketClosed()
	{
		HandleWSClosed(1006, TEXT("test socket closed"), false);
	}
	void SocketError()
	{
		HandleWSError(TEXT("test socket error"));
	}
	/** Reserve the next message slot, as if that message were still being parsed on a worker. */
	uint64 BeginParse()
	{
		return NextIncomingId++;
	}
	/** Deliver a reserved message the way the worker does, without calling FrameTick. */
	void FinishParse(uint64 Id, const FServerMessageType& Message)
	{
		FIncomingEvent Event;
		Event.Message = Message;
		PreProcessServerMessage(Message, *TableDeserializers, Event.TableData);
		AcceptIncomingEvent(ConnectionGeneration, Id, MoveTemp(Event));
	}
	void RetryNow()
	{
		ReconnectAt = 0;
		FrameTick();
	}
	void EnableProvider()
	{
		TokenProvider.BindDynamic(this, &UReconnectTestConnection::ProvideToken);
	}
	void SetToken(const FString& InToken)
	{
		Token = InToken;
	}
	bool NeedsToken() const
	{
		return TokenNeedsRefresh();
	}
	uint64 Generation() const
	{
		return ConnectionGeneration;
	}
	void LateFailure(uint64 OldGeneration)
	{
		FIncomingEvent Event;
		Event.bFailure = true;
		AcceptIncomingEvent(OldGeneration, 0, MoveTemp(Event));
		FrameTick();
	}
	void Inject(const FServerMessageType& Message)
	{
		// Same row preprocessing the background parser does.
		FIncomingEvent Event;
		Event.Message = Message;
		PreProcessServerMessage(Message, *TableDeserializers, Event.TableData);
		AcceptIncomingEvent(ConnectionGeneration, NextIncomingId++, MoveTemp(Event));
		FrameTick();
	}
	void DropSessionBusy()
	{
		QueueFailure(TEXT("session busy"), false, false, true);
		FrameTick();
	}
	int32 Attempt() const
	{
		return ReconnectAttempt;
	}
	USubscriptionHandleBase* Subscribe(const TArray<FString>& Queries);
	void TrackReducer()
	{
		FReducerCallInfoType Info;
		Info.ReducerName = TEXT("test");
		PendingReducerCalls.Add(GetNextRequestId(), Info);
	}
	uint32 CallReducer()
	{
		return InternalCallReducer(TEXT("test"), TArray<uint8>());
	}
	void TrackProcedure(const FOnProcedureCompleteDelegate& Callback)
	{
		ProcedureCallbacks->RegisterCallback(Callback);
	}
	void CallProcedure(const FOnProcedureCompleteDelegate& Callback)
	{
		InternalCallProcedure(TEXT("test"), TArray<uint8>(), Callback);
	}
	void AttachTable(UReconnectTestTable* Table)
	{
		RegisterTable<FReconnectTestRow, UReconnectTestTable, FEventContextBase>(Table->Name, Table);
	}
	virtual bool IsActive() const override
	{
		return bLive ? Super::IsActive() : bEstablished && !bReconnecting && !bEnded;
	}
	virtual bool SendRawMessage(const TArray<uint8>& Bytes) override
	{
		Sent.Add(UE::SpacetimeDB::Deserialize<FClientMessageType>(Bytes));
		return bLive ? Super::SendRawMessage(Bytes) : IsActive();
	}

protected:
	virtual void StartSocket() override
	{
		++Starts;
		SentToken = Token;
		if (bLive)
		{
			Super::StartSocket();
		}
	}
	virtual void DbUpdate(const FDatabaseUpdateType& Update, const FSpacetimeDBEvent& Event) override
	{
		FEventContextBase Context;
		Context.Event = Event;
		ApplyRegisteredTableUpdates(Update, &Context);
	}
	virtual void ReducerEvent(const FReducerEvent& Event) override;
	virtual void ReducerEventFailed(const FReducerEvent& Event, const FString ErrorMessage) override
	{
		FailedCallErrors.Add(Event.CallError);
	}
	UFUNCTION()
	void Connected(UDbConnectionBase* Connection, FSpacetimeDBIdentity InIdentity, const FString& InToken)
	{
		++Connects;
	}
	UFUNCTION()
	void Reconnected(UDbConnectionBase* Connection, FSpacetimeDBIdentity InIdentity, const FString& InToken)
	{
		++Reconnects;
		if (bCancelOnReconnect)
		{
			Disconnect();
		}
	}
	UFUNCTION()
	void Disconnected(UDbConnectionBase* Connection, const FString& Error, bool bRetry, int32 InAttempt, float Delay)
	{
		++Disconnects;
		bLastRetry = bRetry;
		LastAttempt = InAttempt;
		LastDelay = Delay;
	}
	UFUNCTION()
	void ConnectError(const FString& Error, bool bRetry, int32 InAttempt, float Delay)
	{
		++Errors;
		bLastRetry = bRetry;
		LastAttempt = InAttempt;
		LastDelay = Delay;
	}
	UFUNCTION()
	void LegacyDisconnected(UDbConnectionBase* Connection, const FString& Error)
	{
		++LegacyDisconnects;
	}
	UFUNCTION()
	void LegacyConnectError(const FString& Error)
	{
		++LegacyErrors;
	}
	UFUNCTION()
	void SubscriptionApplied(const FSubscriptionEventContextBase& Context)
	{
		++Applied;
	}
	UFUNCTION()
	void ProvideToken(UReconnectTokenRequest* Request);
};

UCLASS()
class UReconnectTestSubscriptionBuilder : public USubscriptionBuilderBase
{
	GENERATED_BODY()
public:
	USubscriptionHandleBase* Build(const TArray<FString>& Queries, FSubscriptionEventDelegate Applied)
	{
		OnAppliedBase(Applied);
		return SubscribeBase(Queries, NewObject<USubscriptionHandleBase>());
	}
};
