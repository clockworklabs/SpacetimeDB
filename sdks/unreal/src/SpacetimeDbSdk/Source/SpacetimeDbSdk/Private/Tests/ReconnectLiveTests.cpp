#include "ReconnectTestConnection.h"
#include "Connection/DbConnectionBuilder.h"
#include "Misc/AutomationTest.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

void UReconnectTestConnection::StartLive(const FString& InUri, const FString& InDatabase)
{
	Initialize();
	bLive = true;
	FAutomaticReconnectOptions Options;
	Options.MinDelaySeconds = 0.5f;
	Options.MaxDelaySeconds = 1;
	NewObject<UDbConnectionBuilderBase>()
	    ->WithUriBase(InUri)
	    ->WithDatabaseNameBase(InDatabase)
	    ->WithAutomaticReconnectOptionsBase(Options)
	    ->WithTokenProviderBase(TokenProvider)
	    ->OnConnectBase(OnConnectBaseDelegate)
	    ->OnAutomaticReconnectBase(OnAutomaticReconnectBaseDelegate)
	    ->OnDisconnectWithReconnectBase(OnDisconnectWithReconnectBaseDelegate)
	    ->OnConnectErrorWithReconnectBase(OnConnectErrorWithReconnectDelegate)
	    ->BuildConnection(this);
}

#if WITH_DEV_AUTOMATION_TESTS
// These rows and reducer argument layouts match sdk-test's pk_u_32 tables.
// Use real WebSockets and the production cache adapter; only expose transport
// teardown and the retry deadline to avoid timing-dependent fault injection.
class FReconnectLiveCommand : public IAutomationLatentCommand
{
public:
	FReconnectLiveCommand(FAutomationTestBase* InTest, const FString& Uri, const FString& Database)
	    : Test(InTest), A(TEXT("pk_u_32")), B(TEXT("pk_u_32_two"))
	{
		Reader = NewObject<UReconnectTestConnection>();
		Reader->AddToRoot();
		Writer = NewObject<UReconnectTestConnection>();
		Writer->AddToRoot();
		Reader->AttachTable(&A);
		Reader->AttachTable(&B);
		Reader->StartLive(Uri, Database);
		Writer->StartLive(Uri, Database);
	}
	virtual bool Update() override
	{
		Reader->FrameTick();
		Writer->FrameTick();
		if (FPlatformTime::Seconds() - Started > 45)
		{
			Test->AddError(
			    FString::Printf(TEXT("Live reconnect timed out at step %d (reader errors %d, writer errors %d)"), Step,
			                    Reader->Errors, Writer->Errors));
			return Finish();
		}
		if (Step == 0 && Reader->Connects == 1 && Writer->Connects == 1)
		{
			Reader->ProviderToken = Reader->CurrentToken();
			OriginalId = Reader->GetConnectionId();
			Reader->TryGetIdentity(OriginalIdentity);
			Writer->CallReducerTyped(TEXT("insert_pk_u_32"), FReconnectTestRow{1, 10});
			Writer->CallReducerTyped(TEXT("insert_pk_u_32_two"), FReconnectTestRow{2, 20});
			++Step;
		}
		else if (Step == 1 && Writer->CallErrors.Num() == 2)
		{
			First = Reader->Subscribe({TEXT("SELECT * FROM pk_u_32"), TEXT("SELECT * FROM pk_u_32_two")});
			Second = Reader->Subscribe({TEXT("SELECT * FROM pk_u_32")});
			++Step;
		}
		else if (Step == 2 && Reader->Applied == 2)
		{
			Test->TestEqual(TEXT("initial row"), Value(A, 1), 10);
			A.OnUpdate.AddLambda(
			    [this](const auto&, const auto&, const auto&)
			    {
				    ++Updates;
				    Test->TestEqual(TEXT("all replay tables visible inside row callback"), Value(B, 2), 21);
			    });
			A.OnInsert.AddLambda(
			    [this](const auto&, const auto&)
			    {
				    ++UnexpectedRows;
			    });
			A.OnDelete.AddLambda(
			    [this](const auto&, const auto&)
			    {
				    ++UnexpectedRows;
			    });
			Reader->DropSocket();
			Reader->FrameTick();
			Reader->DelayRecovery();
			Test->TestTrue(TEXT("drop enters recovery"), Reader->IsReconnecting());
			Writer->CallReducerTyped(TEXT("update_pk_u_32"), FReconnectTestRow{1, 11});
			Writer->CallReducerTyped(TEXT("update_pk_u_32_two"), FReconnectTestRow{2, 21});
			++Step;
		}
		else if (Step == 3 && Writer->CallErrors.Num() == 4)
		{
			Test->TestEqual(TEXT("cache stays readable and stale"), Value(A, 1), 10);
			Reader->RetryNow();
			++Step;
		}
		else if (Step == 4 && Reader->Applied == 4)
		{
			Test->TestEqual(TEXT("one initial connect"), Reader->Connects, 1);
			Test->TestEqual(TEXT("automatic callback"), Reader->Reconnects, 1);
			Test->TestEqual(TEXT("one net update"), Updates, 1);
			Test->TestEqual(TEXT("no spurious inserts or deletes"), UnexpectedRows, 0);
			Test->TestTrue(TEXT("new connection ID"), Reader->GetConnectionId() != OriginalId);
			FSpacetimeDBIdentity Identity;
			Reader->TryGetIdentity(Identity);
			Test->TestTrue(TEXT("same anonymous identity"), Identity == OriginalIdentity);
			Test->TestEqual(TEXT("retained token needs no refresh"), Reader->ProviderCalls, 0);
			Test->TestTrue(TEXT("both original handles active"), First->IsActive() && Second->IsActive());
			Reader->DropSocket();
			Reader->FrameTick();
			Reader->DelayRecovery();
			Second->Unsubscribe();
			Rejected = Reader->Subscribe({TEXT("SELECT * FROM reconnect_missing_table")});
			Reader->EnableProvider();
			// Readable, far-future expiry, but rejected signature: only the HTTP
			// rejection should force refresh, not the local expiry heuristic.
			Reader->SetToken(TEXT("eyJhbGciOiJFUzI1NiJ9.eyJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMH0.invalid"));
			Reader->RetryNow();
			++Step;
		}
		else if (Step == 5 && Reader->Applied == 5 && Rejected->IsEnded())
		{
			Test->TestEqual(TEXT("token provider refreshed once"), Reader->ProviderCalls, 1);
			Test->TestEqual(TEXT("second automatic reconnect"), Reader->Reconnects, 2);
			Test->TestEqual(TEXT("unchanged snapshot produces no row event"), Updates, 1);
			Test->TestTrue(TEXT("offline cancellation survives replay"), Second->IsEnded());
			Test->TestTrue(TEXT("valid set survives partial batch error"), First->IsActive());
			const auto Key = UE::SpacetimeDB::Serialize(FReconnectTestRow{1, 11});
			const auto* Entry = A.Data->GetTable(A.Name)->Entries.Find(Key);
			if (!Test->TestNotNull(TEXT("reconciled row exists"), Entry))
			{
				return Finish();
			}
			Test->TestEqual(TEXT("cancelled overlap removes its reference"), Entry->RefCount, 1);
			Test->TestEqual(TEXT("rejected retained credential reported once"), Reader->Errors, 1);
			Reader->DropSocket();
			Reader->FrameTick();
			Reader->SetToken(TEXT("unreadable"));
			Reader->ProviderToken = TEXT("fresh-but-invalid");
			Reader->RetryNow();
			++Step;
		}
		else if (Step == 6 && Reader->Errors == 2 && !Reader->IsReconnecting())
		{
			Test->TestEqual(TEXT("fresh rejected token is terminal"), Reader->ProviderCalls, 2);
			Test->TestFalse(TEXT("terminal error has no retry"), Reader->bLastRetry);
			Test->TestFalse(TEXT("terminal connection inactive"), Reader->IsActive());
			return Finish();
		}
		return false;
	}

private:
	int32 Value(const FReconnectTestTable& Table, uint32 Id)
	{
		const auto Cache = Table.Data->GetTable(Table.Name);
		const auto* Row = Cache.IsValid() ? Cache->FindByUniqueIndex<uint32>(TEXT("id"), Id) : nullptr;
		return Row ? Row->Value : -1;
	}
	bool Finish()
	{
		Reader->Disconnect();
		Writer->Disconnect();
		Reader->RemoveFromRoot();
		Writer->RemoveFromRoot();
		return true;
	}
	FAutomationTestBase* Test;
	UReconnectTestConnection *Reader, *Writer;
	FReconnectTestTable A, B;
	USubscriptionHandleBase *First = nullptr, *Second = nullptr, *Rejected = nullptr;
	FSpacetimeDBConnectionId OriginalId;
	FSpacetimeDBIdentity OriginalIdentity;
	int32 Step = 0, Updates = 0, UnexpectedRows = 0;
	double Started = FPlatformTime::Seconds();
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReconnectLiveServer, "SpacetimeDB.TestClient.ReconnectLiveServer",
                                 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReconnectLiveServer::RunTest(const FString&)
{
	FString Database = FPlatformMisc::GetEnvironmentVariable(TEXT("SPACETIME_SDK_TEST_DB_NAME"));
	FString Uri = FPlatformMisc::GetEnvironmentVariable(TEXT("SPACETIME_SDK_TEST_SERVER_URL"));
	if (Database.IsEmpty())
	{
		FParse::Value(FCommandLine::Get(), TEXT("-SpacetimeDbName="), Database);
	}
	if (Uri.IsEmpty())
	{
		FParse::Value(FCommandLine::Get(), TEXT("-SpacetimeServerUrl="), Uri);
	}
	if (Uri.IsEmpty())
	{
		Uri = TEXT("localhost:3000");
	}
	if (Database.IsEmpty())
	{
		AddError(
		    TEXT("ReconnectLiveServer needs an sdk-test database via SPACETIME_SDK_TEST_DB_NAME or -SpacetimeDbName"));
		return false;
	}
	ADD_LATENT_AUTOMATION_COMMAND(FReconnectLiveCommand(this, Uri, Database));
	return true;
}
#endif
