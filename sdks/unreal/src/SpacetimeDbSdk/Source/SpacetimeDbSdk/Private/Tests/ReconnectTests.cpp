#include "ReconnectTestConnection.h"
#include "Misc/AutomationTest.h"
#include <initializer_list>

void UReconnectTestConnection::Initialize(bool bEnable)
{
	bAutomaticReconnect = bEnable;
	ReconnectOptions.Resolve();
	OnConnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Connected);
	OnAutomaticReconnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Reconnected);
	OnDisconnectWithReconnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Disconnected);
	OnConnectErrorWithReconnectDelegate.BindDynamic(this, &UReconnectTestConnection::ConnectError);
	OnDisconnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::LegacyDisconnected);
	OnConnectErrorDelegate.BindDynamic(this, &UReconnectTestConnection::LegacyConnectError);
}

void UReconnectTestConnection::ConnectIdentity(const FString& Hex, const FString& NewToken)
{
	FInitialConnectionType Initial;
	Initial.Identity = FSpacetimeDBIdentity::FromHex(Hex);
	Initial.Token = NewToken;
	Initial.ConnectionId = FSpacetimeDBConnectionId::FromHex(FGuid::NewGuid().ToString(EGuidFormats::Digits));
	Inject(FServerMessageType::InitialConnection(Initial));
}

USubscriptionHandleBase* UReconnectTestConnection::Subscribe(const TArray<FString>& Queries)
{
	FSubscriptionEventDelegate AppliedDelegate;
	AppliedDelegate.BindDynamic(this, &UReconnectTestConnection::SubscriptionApplied);
	auto* Handle = NewObject<UReconnectTestSubscriptionBuilder>()->Build(Queries, AppliedDelegate);
	StartSubscription(Handle);
	return Handle;
}

void UReconnectTestConnection::ProvideToken(UReconnectTokenRequest* Request)
{
	++ProviderCalls;
	if (bHoldProvider)
	{
		HeldRequest = Request;
		return;
	}
	if (bFailProvider)
	{
		bFailProvider = false;
		Request->Complete(FString(), TEXT("provider unavailable"));
	}
	else
	{
		Request->Complete(ProviderToken.IsEmpty() ? TEXT("refreshed") : ProviderToken, FString());
	}
}

void UReconnectTestConnection::ReducerEvent(const FReducerEvent& Event)
{
	CallErrors.Add(Event.CallError);
	if (bLive && Event.Status.IsFailed())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Live test reducer failed: %s"), *Event.Status.GetAsFailed());
	}
}

#if WITH_DEV_AUTOMATION_TESTS
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectLifecycle, "SpacetimeDB.TestClient.ReconnectLifecycle",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectLifecycle::RunTest(const FString&)
{
	auto* InitialFailure = NewObject<UReconnectTestConnection>();
	InitialFailure->Initialize();
	InitialFailure->Drop();
	TestEqual(TEXT("initial failure does not retry"), InitialFailure->Errors, 1);
	TestFalse(TEXT("no initial retry"), InitialFailure->IsReconnecting());
	auto* Disabled = NewObject<UReconnectTestConnection>();
	Disabled->Initialize(false);
	Disabled->ConnectIdentity();
	Disabled->Drop();
	TestFalse(TEXT("reconnect is opt in"), Disabled->IsReconnecting());

	auto* Conn = NewObject<UReconnectTestConnection>();
	Conn->Initialize();
	Conn->ConnectIdentity();
	const auto ConnectionId = Conn->GetConnectionId();
	const uint64 OldGeneration = Conn->Generation();
	Conn->TrackReducer();
	int32 ProcedureCompletions = 0;
	FOnProcedureCompleteDelegate Callback;
	Callback.BindLambda(
	    [&](const FSpacetimeDBEvent& Event, const TArray<uint8>&, bool bSuccess)
	    {
		    ++ProcedureCompletions;
		    TestFalse(TEXT("interrupted procedure failed"), bSuccess);
		    TestTrue(TEXT("procedure result unknown"),
		             Event.GetAsProcedure().CallError == ESpacetimeDBCallError::UnknownResult);
	    });
	Conn->TrackProcedure(Callback);
	Conn->Drop();
	TestTrue(TEXT("retrying established connection"), Conn->IsReconnecting());
	TestEqual(TEXT("attempt starts at one"), Conn->LastAttempt, 1);
	TestTrue(TEXT("delay bound"), Conn->LastDelay >= 1.0f && Conn->LastDelay <= 30.0f);
	TestTrue(TEXT("reducer result unknown"),
	         Conn->CallErrors.Num() == 1 && Conn->CallErrors[0] == ESpacetimeDBCallError::UnknownResult);
	TestEqual(TEXT("procedure settled once"), ProcedureCompletions, 1);
	AddExpectedError(TEXT("Cannot call reducer, not connected to server!"), EAutomationExpectedErrorFlags::Contains, 1);
	TestEqual(TEXT("reducer during recovery not sent"), Conn->CallReducer(), 0u);
	TestTrue(TEXT("reducer during recovery reported"),
	         Conn->FailedCallErrors.Num() == 2 && Conn->FailedCallErrors[1] == ESpacetimeDBCallError::Disconnected);
	Conn->RetryNow();
	TestEqual(TEXT("anonymous credentials retained"), Conn->SentToken, FString(TEXT("retained")));
	Conn->ConnectIdentity();
	TestEqual(TEXT("initial callback only once"), Conn->Connects, 1);
	TestEqual(TEXT("automatic callback"), Conn->Reconnects, 1);
	TestTrue(TEXT("new connection id"), Conn->GetConnectionId() != ConnectionId);
	Conn->LateFailure(OldGeneration);
	TestTrue(TEXT("old socket ignored"), Conn->IsActive());
	Conn->Drop();
	Conn->Disconnect();
	Conn->RetryNow();
	TestFalse(TEXT("cancellation stops retry"), Conn->IsReconnecting());
	TestFalse(TEXT("final disconnect has no retry"), Conn->bLastRetry);
	TestEqual(TEXT("no additional socket opened"), Conn->Starts, 1);

	auto* Different = NewObject<UReconnectTestConnection>();
	Different->Initialize();
	Different->ConnectIdentity();
	Different->Drop();
	AddExpectedError(TEXT("Reconnect returned a different identity"), EAutomationExpectedErrorFlags::Contains, 1);
	Different->ConnectIdentity(TEXT("2222222222222222222222222222222222222222222222222222222222222222"));
	TestFalse(TEXT("identity change terminal"), Different->IsReconnecting());
	TestEqual(TEXT("identity change reported as connect error"), Different->Errors, 1);
	auto* Cancel = NewObject<UReconnectTestConnection>();
	Cancel->Initialize();
	Cancel->ConnectIdentity();
	Cancel->Drop();
	Cancel->bCancelOnReconnect = true;
	Cancel->ConnectIdentity();
	TestFalse(TEXT("cancel in callback stays ended"), Cancel->IsActive());
	TestEqual(TEXT("no replay after callback cancellation"), Cancel->Sent.Num(), 0);

	TestEqual(TEXT("legacy connect error still fires"), InitialFailure->LegacyErrors, 1);
	TestEqual(TEXT("legacy disconnect still fires"), Conn->LegacyDisconnects, Conn->Disconnects);

	auto* Busy = NewObject<UReconnectTestConnection>();
	Busy->Initialize();
	Busy->ConnectIdentity();
	Busy->Drop();
	Busy->RetryNow();
	Busy->DropSessionBusy();
	TestTrue(TEXT("session busy retries"), Busy->IsReconnecting());
	TestEqual(TEXT("session busy does not count as an attempt"), Busy->Attempt(), 1);
	Busy->RetryNow();
	Busy->Drop();
	TestEqual(TEXT("ordinary failure still counts"), Busy->Attempt(), 2);
	Busy->Disconnect();

	auto* Early = NewObject<UReconnectTestConnection>();
	Early->Initialize();
	auto* Queued = Early->Subscribe({TEXT("SELECT * FROM a")});
	auto* Cancelled = Early->Subscribe({TEXT("SELECT * FROM b")});
	Cancelled->Unsubscribe();
	TestEqual(TEXT("nothing sent before InitialConnection"), Early->Sent.Num(), 0);
	TestTrue(TEXT("unsent subscription ends locally"), Cancelled->IsEnded());
	TestFalse(TEXT("early subscription keeps the connection"), Early->IsReconnecting() || Early->Errors > 0);
	Early->ConnectIdentity();
	TestTrue(TEXT("queued subscription sent after connect"),
	         Early->Sent.Num() == 1 && Early->Sent[0].IsSubscribe() &&
	             Early->Sent[0].GetAsSubscribe().QueryStrings == Queued->GetQuerySqls());
	Early->Disconnect();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectSocketFailureDelivery, "SpacetimeDB.TestClient.ReconnectSocketFailureDelivery",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectSocketFailureDelivery::RunTest(const FString&)
{
	// Without reconnect, apps that stop calling FrameTick once IsActive is false must still be told.
	auto* Closed = NewObject<UReconnectTestConnection>();
	Closed->Initialize(false);
	Closed->ConnectIdentity();
	Closed->SocketClosed();
	TestEqual(TEXT("disconnect fires without FrameTick"), Closed->LegacyDisconnects, 1);
	TestEqual(TEXT("reconnect-aware disconnect fires too"), Closed->Disconnects, 1);
	TestFalse(TEXT("no retry without reconnect"), Closed->bLastRetry);

	auto* Refused = NewObject<UReconnectTestConnection>();
	Refused->Initialize(false);
	Refused->SocketError();
	TestEqual(TEXT("connect error fires without FrameTick"), Refused->LegacyErrors, 1);

	// A message still being parsed when the socket closes is applied first, not dropped.
	auto* InFlight = NewObject<UReconnectTestConnection>();
	InFlight->Initialize(false);
	const uint64 Pending = InFlight->BeginParse();
	InFlight->SocketClosed();
	TestEqual(TEXT("close waits for the earlier message"), InFlight->LegacyDisconnects + InFlight->LegacyErrors, 0);
	FInitialConnectionType Initial;
	Initial.Identity = FSpacetimeDBIdentity::FromHex(TEXT("1111111111111111111111111111111111111111111111111111111111111111"));
	Initial.Token = TEXT("retained");
	Initial.ConnectionId = FSpacetimeDBConnectionId::FromHex(FGuid::NewGuid().ToString(EGuidFormats::Digits));
	InFlight->FinishParse(Pending, FServerMessageType::InitialConnection(Initial));
	TestEqual(TEXT("earlier message applied"), InFlight->Connects, 1);
	TestEqual(TEXT("then the disconnect, without FrameTick"), InFlight->LegacyDisconnects, 1);
	TestEqual(TEXT("not reported as a connect error"), InFlight->LegacyErrors, 0);

	// Without reconnect, a close is OnDisconnect and an error OnConnectError, whatever the state, as on master.
	auto* EarlyClose = NewObject<UReconnectTestConnection>();
	EarlyClose->Initialize(false);
	EarlyClose->SocketClosed();
	TestEqual(TEXT("close before InitialConnection is a disconnect"), EarlyClose->LegacyDisconnects, 1);
	TestEqual(TEXT("and not a connect error"), EarlyClose->LegacyErrors, 0);
	auto* LateError = NewObject<UReconnectTestConnection>();
	LateError->Initialize(false);
	LateError->ConnectIdentity();
	LateError->SocketError();
	TestEqual(TEXT("error after InitialConnection is a connect error"), LateError->LegacyErrors, 1);
	TestEqual(TEXT("and not a disconnect"), LateError->LegacyDisconnects, 0);

	// With reconnect, the failure waits for FrameTick, behind any earlier messages.
	auto* Queued = NewObject<UReconnectTestConnection>();
	Queued->Initialize();
	Queued->ConnectIdentity();
	Queued->SocketClosed();
	TestEqual(TEXT("reconnect defers the disconnect"), Queued->Disconnects, 0);
	Queued->FrameTick();
	TestEqual(TEXT("FrameTick delivers it"), Queued->Disconnects, 1);
	TestTrue(TEXT("and schedules a retry"), Queued->IsReconnecting());
	Queued->Disconnect();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectExplicitDisconnect, "SpacetimeDB.TestClient.ReconnectExplicitDisconnect",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectExplicitDisconnect::RunTest(const FString&)
{
	// Without reconnect, report on the next engine tick, as the socket close did before reconnect existed.
	auto* Legacy = NewObject<UReconnectTestConnection>();
	Legacy->Initialize(false);
	Legacy->ConnectIdentity();
	Legacy->Disconnect();
	TestEqual(TEXT("not reported inside Disconnect"), Legacy->LegacyDisconnects, 0);
	FTSTicker::GetCoreTicker().Tick(0.0f);
	TestEqual(TEXT("reported on the next engine tick"), Legacy->LegacyDisconnects, 1);
	FTSTicker::GetCoreTicker().Tick(0.0f);
	TestEqual(TEXT("reported once"), Legacy->LegacyDisconnects, 1);

	auto* NeverConnected = NewObject<UReconnectTestConnection>();
	NeverConnected->Initialize(false);
	NeverConnected->Disconnect();
	FTSTicker::GetCoreTicker().Tick(0.0f);
	TestEqual(TEXT("never-connected disconnect is silent"), NeverConnected->LegacyDisconnects, 0);

	// With reconnect, report now, matching C# and the TS SDK.
	auto* Reconnecting = NewObject<UReconnectTestConnection>();
	Reconnecting->Initialize();
	Reconnecting->ConnectIdentity();
	Reconnecting->Disconnect();
	TestEqual(TEXT("reported inside Disconnect"), Reconnecting->Disconnects, 1);
	TestFalse(TEXT("final, with no retry"), Reconnecting->bLastRetry);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectFinalDisconnectState, "SpacetimeDB.TestClient.ReconnectFinalDisconnectState",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectFinalDisconnectState::RunTest(const FString&)
{
	// A final disconnect leaves subscription handles inactive but not ended (no OnEnd), as on master, C# and TS.
	auto* Closed = NewObject<UReconnectTestConnection>();
	Closed->Initialize(false);
	Closed->ConnectIdentity();
	auto* ClosedHandle = Closed->Subscribe({TEXT("SELECT * FROM a")});
	Closed->SocketClosed();
	TestFalse(TEXT("server close does not end the handle"), ClosedHandle->IsEnded());
	TestFalse(TEXT("but it is inactive"), ClosedHandle->IsActive());

	auto* Explicit = NewObject<UReconnectTestConnection>();
	Explicit->Initialize();
	Explicit->ConnectIdentity();
	auto* ExplicitHandle = Explicit->Subscribe({TEXT("SELECT * FROM a")});
	Explicit->Disconnect();
	TestFalse(TEXT("explicit Disconnect does not end the handle"), ExplicitHandle->IsEnded());

	// A procedure called while disconnected is dropped without reconnect, as on master, and reported with it.
	AddExpectedError(TEXT("Cannot call procedure, not connected to server!"), EAutomationExpectedErrorFlags::Contains, 2);
	int32 Completions = 0;
	FOnProcedureCompleteDelegate Callback;
	Callback.BindLambda(
	    [&](const FSpacetimeDBEvent&, const TArray<uint8>&, bool)
	    {
		    ++Completions;
	    });
	auto* Offline = NewObject<UReconnectTestConnection>();
	Offline->Initialize(false);
	Offline->CallProcedure(Callback);
	TestEqual(TEXT("dropped without reconnect"), Completions, 0);
	auto* Waiting = NewObject<UReconnectTestConnection>();
	Waiting->Initialize();
	Waiting->CallProcedure(Callback);
	TestEqual(TEXT("reported with reconnect"), Completions, 1);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectCredentials, "SpacetimeDB.TestClient.ReconnectCredentials",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectCredentials::RunTest(const FString&)
{
	FAutomaticReconnectOptions Options;
	Options.MinDelaySeconds = 0;
	Options.MaxDelaySeconds = 0;
	Options.Resolve();
	TestEqual(TEXT("minimum floor"), Options.MinDelaySeconds, 0.5f);
	TestEqual(TEXT("maximum floor"), Options.MaxDelaySeconds, 1.0f);
	Options.MinDelaySeconds = 2;
	Options.Resolve();
	TestEqual(TEXT("max at least min"), Options.MaxDelaySeconds, 2.0f);
	TestEqual(TEXT("saturated delay"), Options.Delay(MAX_int32, 1.0), 2.0);
	auto* Conn = NewObject<UReconnectTestConnection>();
	Conn->Initialize();
	Conn->EnableProvider();
	Conn->ConnectIdentity();
	TestTrue(TEXT("unreadable token refreshes"), Conn->NeedsToken());
	Conn->SetToken(TEXT("header.eyJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMH0.signature"));
	TestFalse(TEXT("unexpired token retained"), Conn->NeedsToken());
	Conn->Drop();
	Conn->RetryNow();
	TestEqual(TEXT("provider not needed"), Conn->ProviderCalls, 0);
	Conn->Drop(false, true);
	Conn->bFailProvider = true;
	Conn->RetryNow();
	Conn->FrameTick();
	TestTrue(TEXT("provider failure retries"), Conn->IsReconnecting());
	Conn->RetryNow();
	TestEqual(TEXT("forced refresh survives provider failure"), Conn->ProviderCalls, 2);
	TestEqual(TEXT("fresh credential used"), Conn->SentToken, FString(TEXT("refreshed")));
	Conn->Drop(false, true);
	TestFalse(TEXT("rejected fresh credential terminal"), Conn->IsReconnecting());

	auto* Pending = NewObject<UReconnectTestConnection>();
	Pending->Initialize();
	Pending->EnableProvider();
	Pending->ConnectIdentity();
	Pending->bHoldProvider = true;
	Pending->Drop();
	Pending->RetryNow();
	Pending->Disconnect();
	Pending->HeldRequest->Complete(TEXT("too late"), FString());
	Pending->FrameTick();
	TestEqual(TEXT("late provider cannot reconnect"), Pending->Starts, 0);
	return true;
}

static FQueryRowsType Rows(const FString& Table, std::initializer_list<FReconnectTestRow> Values)
{
	FSingleTableRowsType Rows;
	Rows.Table = Table;
	TArray<uint64> Offsets;
	for (const auto& Row : Values)
	{
		Offsets.Add(Rows.Rows.RowsData.Num());
		Rows.Rows.RowsData.Append(UE::SpacetimeDB::Serialize(Row));
	}
	Rows.Rows.SizeHint = FRowSizeHintType::RowOffsets(Offsets);
	FQueryRowsType Query;
	Query.Tables.Add(Rows);
	return Query;
}

static TArray<FTableUpdateType> QueryRowsToTableUpdates(const FQueryRowsType& Query)
{
	TArray<FTableUpdateType> Tables;
	for (const auto& TableRows : Query.Tables)
	{
		FTableUpdateType Table;
		Table.TableName = TableRows.Table;
		FPersistentTableRowsType Persistent;
		Persistent.Inserts = TableRows.Rows;
		Table.Rows.Add(FTableUpdateRowsType::PersistentTable(Persistent));
		Tables.Add(Table);
	}
	return Tables;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectCacheReplay, "SpacetimeDB.TestClient.ReconnectCacheReplay",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectCacheReplay::RunTest(const FString&)
{
	auto* Conn = NewObject<UReconnectTestConnection>();
	Conn->Initialize();
	TStrongObjectPtr<UReconnectTestTable> A(UReconnectTestTable::Create(TEXT("a")));
	TStrongObjectPtr<UReconnectTestTable> B(UReconnectTestTable::Create(TEXT("b")));
	Conn->AttachTable(A.Get());
	Conn->AttachTable(B.Get());
	Conn->ConnectIdentity();
	auto* First = Conn->Subscribe({TEXT("SELECT * FROM a"), TEXT("SELECT * FROM b")});
	auto* Second = Conn->Subscribe({TEXT("SELECT * FROM a")});
	auto FirstId = Conn->Sent[0].GetAsSubscribe().QuerySetId;
	auto SecondId = Conn->Sent[1].GetAsSubscribe().QuerySetId;
	FSubscribeAppliedType Initial;
	Initial.QuerySetId = FirstId;
	Initial.Rows = Rows(TEXT("a"), {{1, 10}, {2, 20}});
	Initial.Rows.Tables.Append(Rows(TEXT("b"), {{3, 30}}).Tables);
	Conn->Inject(FServerMessageType::SubscribeApplied(Initial));
	Initial.QuerySetId = SecondId;
	Initial.Rows = Rows(TEXT("a"), {{1, 10}});
	Conn->Inject(FServerMessageType::SubscribeApplied(Initial));
	// A table the bindings do not know is skipped, as before reconnect support.
	AddExpectedError(TEXT("No deserializer found for table missing"), EAutomationExpectedErrorFlags::Contains, 1);
	FQuerySetUpdateType Unknown;
	Unknown.QuerySetId = FirstId;
	Unknown.Tables = QueryRowsToTableUpdates(Rows(TEXT("missing"), {{9, 90}}));
	FTransactionUpdateType UnknownTx;
	UnknownTx.QuerySets.Add(Unknown);
	Conn->Inject(FServerMessageType::TransactionUpdate(UnknownTx));
	TestTrue(TEXT("unknown table keeps the connection"), Conn->IsActive());
	int32 Inserts = 0, Deletes = 0, Updates = 0;
	auto CheckAtomic = [&]()
	{
		TestEqual(TEXT("other table already reconciled"),
		          B->Data->GetTable(TEXT("b"))->FindByUniqueIndex<uint32>(TEXT("id"), 3)->Value, 31);
	};
	A->OnInsert.AddLambda(
	    [&](const auto&, const auto&)
	    {
		    ++Inserts;
		    CheckAtomic();
	    });
	A->OnDelete.AddLambda(
	    [&](const auto&, const auto&)
	    {
		    ++Deletes;
		    CheckAtomic();
	    });
	A->OnUpdate.AddLambda(
	    [&](const auto&, const auto&, const auto&)
	    {
		    ++Updates;
		    CheckAtomic();
	    });
	Conn->Drop();
	TestEqual(TEXT("stale cache retained"), A->Data->GetTable(TEXT("a"))->Entries.Num(), 2);
	TestFalse(TEXT("handle pending during outage"), First->IsActive());
	Conn->ConnectIdentity();
	auto Batch = Conn->Sent.Last().GetAsSubscribeBatch();
	TestEqual(TEXT("one batch carries both sets"), Batch.Sets.Num(), 2);
	FSubscribeBatchAppliedType Reply;
	Reply.RequestId = Batch.RequestId;
	for (const auto& Set : Batch.Sets)
	{
		TestTrue(TEXT("fresh query IDs"), Set.QuerySetId.Id != FirstId.Id && Set.QuerySetId.Id != SecondId.Id);
		FSubscribeSetResultType Result;
		Result.QuerySetId = Set.QuerySetId;
		FQueryRowsType Snapshot = Rows(TEXT("a"), {{1, 10}});
		if (Set.QueryStrings.Num() == 2)
		{
			Snapshot = Rows(TEXT("a"), {{1, 10}, {2, 21}, {4, 40}});
			Snapshot.Tables.Append(Rows(TEXT("b"), {{3, 31}}).Tables);
		}
		Result.Outcome = FSubscribeSetOutcomeType::Applied(Snapshot);
		Reply.Results.Add(Result);
	}
	const auto WireReply = UE::SpacetimeDB::Serialize(FServerMessageType::SubscribeBatchApplied(Reply));
	TestEqual(TEXT("batch server wire tag"), WireReply[0], uint8(8));
	const auto DecodedReply = UE::SpacetimeDB::Deserialize<FServerMessageType>(WireReply);
	TestTrue(TEXT("batch response round trip"), DecodedReply.GetAsSubscribeBatchApplied() == Reply);
	TestEqual(TEXT("batch client wire tag"), UE::SpacetimeDB::Serialize(FClientMessageType::SubscribeBatch(Batch))[0],
	          uint8(5));
	Conn->Inject(DecodedReply);
	TestEqual(TEXT("unchanged overlapping row has no event"), Deletes, 0);
	TestEqual(TEXT("one net insert"), Inserts, 1);
	TestEqual(TEXT("one net update"), Updates, 1);
	TestEqual(TEXT("both applied again"), Conn->Applied, 4);
	const auto Key = UE::SpacetimeDB::Serialize(FReconnectTestRow{1, 10});
	TestEqual(TEXT("overlap refcount restored"), A->Data->GetTable(TEXT("a"))->Entries.FindChecked(Key).RefCount, 2);
	Conn->Drop();
	Second->Unsubscribe();
	auto* New = Conn->Subscribe({TEXT("SELECT * FROM missing")});
	TestTrue(TEXT("offline unsubscribe ends handle"), Second->IsEnded());
	Conn->ConnectIdentity();
	Batch = Conn->Sent.Last().GetAsSubscribeBatch();
	Reply.RequestId = Batch.RequestId;
	Reply.Results.Empty();
	for (const auto& Set : Batch.Sets)
	{
		FSubscribeSetResultType Result;
		Result.QuerySetId = Set.QuerySetId;
		if (Set.QueryStrings.Num() == 1)
		{
			Result.Outcome = FSubscribeSetOutcomeType::Error(TEXT("bad query"));
		}
		else
		{
			auto Snapshot = Rows(TEXT("a"), {{1, 10}, {2, 21}, {4, 40}});
			Snapshot.Tables.Append(Rows(TEXT("b"), {{3, 31}}).Tables);
			Result.Outcome = FSubscribeSetOutcomeType::Applied(Snapshot);
		}
		Reply.Results.Add(Result);
	}
	Conn->Inject(FServerMessageType::SubscribeBatchApplied(Reply));
	TestTrue(TEXT("failed replay set ended"), New->IsEnded());
	TestTrue(TEXT("successful replay survives partial error"), First->IsActive());
	TestEqual(TEXT("cancelled set no longer contributes references"),
	          A->Data->GetTable(TEXT("a"))->Entries.FindChecked(Key).RefCount, 1);
	Conn->Disconnect();
	return true;
}
#endif
