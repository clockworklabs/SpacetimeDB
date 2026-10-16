#include "ReconnectTestConnection.h"
#include "Misc/AutomationTest.h"
#include <initializer_list>

void UReconnectTestConnection::Initialize(bool bEnable)
{
	bAutomaticReconnect = bEnable;
	ReconnectOptions.Resolve();
	OnConnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Connected);
	OnAutomaticReconnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Reconnected);
	OnDisconnectBaseDelegate.BindDynamic(this, &UReconnectTestConnection::Disconnected);
	OnConnectErrorDelegate.BindDynamic(this, &UReconnectTestConnection::ConnectError);
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

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectCacheReplay, "SpacetimeDB.TestClient.ReconnectCacheReplay",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectCacheReplay::RunTest(const FString&)
{
	auto* Conn = NewObject<UReconnectTestConnection>();
	Conn->Initialize();
	FReconnectTestTable A(TEXT("a")), B(TEXT("b"));
	Conn->AttachTable(&A);
	Conn->AttachTable(&B);
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
	int32 Inserts = 0, Deletes = 0, Updates = 0;
	auto CheckAtomic = [&]()
	{
		TestEqual(TEXT("other table already reconciled"),
		          B.Data->GetTable(TEXT("b"))->FindByUniqueIndex<uint32>(TEXT("id"), 3)->Value, 31);
	};
	A.OnInsert.AddLambda(
	    [&](const auto&, const auto&)
	    {
		    ++Inserts;
		    CheckAtomic();
	    });
	A.OnDelete.AddLambda(
	    [&](const auto&, const auto&)
	    {
		    ++Deletes;
		    CheckAtomic();
	    });
	A.OnUpdate.AddLambda(
	    [&](const auto&, const auto&, const auto&)
	    {
		    ++Updates;
		    CheckAtomic();
	    });
	Conn->Drop();
	TestEqual(TEXT("stale cache retained"), A.Data->GetTable(TEXT("a"))->Entries.Num(), 2);
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
	TestEqual(TEXT("overlap refcount restored"), A.Data->GetTable(TEXT("a"))->Entries.FindChecked(Key).RefCount, 2);
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
	          A.Data->GetTable(TEXT("a"))->Entries.FindChecked(Key).RefCount, 1);
	Conn->Disconnect();
	return true;
}
#endif
