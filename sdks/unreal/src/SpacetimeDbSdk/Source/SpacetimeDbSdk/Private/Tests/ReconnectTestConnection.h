#pragma once
#include "Connection/DbConnectionBase.h"
#include "DBCache/ClientCache.h"
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

struct FReconnectTestTable
{
	FString Name;
	TSharedPtr<UClientCache<FReconnectTestRow>> Data;
	DECLARE_MULTICAST_DELEGATE_TwoParams(FInsert, const FEventContextBase&, const FReconnectTestRow&);
	DECLARE_MULTICAST_DELEGATE_ThreeParams(FUpdate, const FEventContextBase&, const FReconnectTestRow&,
	                                       const FReconnectTestRow&);
	FInsert OnInsert, OnDelete;
	FUpdate OnUpdate;
	explicit FReconnectTestTable(const FString& InName)
	    : Name(InName), Data(MakeShared<UClientCache<FReconnectTestRow>>())
	{
		Data->GetOrAdd(Name)->AddUniqueConstraint<uint32>(TEXT("id"),
		                                                  [](const auto& Row)
		                                                  {
			                                                  return Row.Id;
		                                                  });
	}
	FTableAppliedDiff<FReconnectTestRow> Update(TArray<FWithBsatn<FReconnectTestRow>> Inserts,
	                                            TArray<FWithBsatn<FReconnectTestRow>> Deletes)
	{
		TArray<TPair<TArray<uint8>, FReconnectTestRow>> InsertRows;
		TArray<TArray<uint8>> DeleteRows;
		for (const auto& Row : Inserts)
		{
			InsertRows.Add({Row.Bsatn, Row.Row});
		}
		for (const auto& Row : Deletes)
		{
			DeleteRows.Add(Row.Bsatn);
		}
		auto Diff = Data->ApplyDiff(Name, InsertRows, DeleteRows);
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
	bool bLastRetry = false, bFailProvider = false, bHoldProvider = false, bCancelOnReconnect = false;
	int32 LastAttempt = 0;
	float LastDelay = 0;
	bool bLive = false;
	FString SentToken, ProviderToken;
	void StartLive(const FString& Uri, const FString& Database);
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
		FIncomingEvent Event;
		Event.Message = Message;
		AcceptIncomingEvent(ConnectionGeneration, NextIncomingId++, MoveTemp(Event));
		FrameTick();
	}
	USubscriptionHandleBase* Subscribe(const TArray<FString>& Queries);
	void TrackReducer()
	{
		FReducerCallInfoType Info;
		Info.ReducerName = TEXT("test");
		PendingReducerCalls.Add(GetNextRequestId(), Info);
	}
	void TrackProcedure(const FOnProcedureCompleteDelegate& Callback)
	{
		ProcedureCallbacks->RegisterCallback(Callback);
	}
	void AttachTable(FReconnectTestTable* Table)
	{
		RegisterTable<FReconnectTestRow, FReconnectTestTable, FEventContextBase>(Table->Name, Table);
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
		// Injected messages bypass the background parser.
		PreProcessDatabaseUpdate(Update);
		ApplyRegisteredTableUpdates(Update, &Context);
	}
	virtual void ReducerEvent(const FReducerEvent& Event) override;
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
	void Disconnected(UDbConnectionBase* Connection, const FString& Error, bool bRetry, int32 Attempt, float Delay)
	{
		++Disconnects;
		bLastRetry = bRetry;
		LastAttempt = Attempt;
		LastDelay = Delay;
	}
	UFUNCTION()
	void ConnectError(const FString& Error, bool bRetry, int32 Attempt, float Delay)
	{
		++Errors;
		bLastRetry = bRetry;
		LastAttempt = Attempt;
		LastDelay = Delay;
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
