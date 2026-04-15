#include "Tests/CommonTestFunctions.h"

void UTestHelperDelegates::HandleConnect(UDbConnection* Conn, FSpacetimeDBIdentity Identity, const FString& Token)
{
	if (OnConnect)
	{
		OnConnect(Conn, Identity, Token);
	}
}

void UTestHelperDelegates::HandleConnectError(UDbConnection* Conn, const FString& Error)
{
	if (OnConnectError)
	{
		OnConnectError(Conn, Error);
	}
}

void UTestHelperDelegates::HandleDisconnect(UDbConnection* Conn, const FString& Error)
{
	if (OnDisconnect)
	{
		OnDisconnect(Conn, Error);
	}
}

void UTestHelperDelegates::HandleSubscriptionApplied(FSubscriptionEventContext Ctx)
{
	if (OnSubscriptionApplied)
	{
		OnSubscriptionApplied(Ctx);
	}
}

void UTestHelperDelegates::HandleSubscriptionError(FErrorContext Ctx)
{
	if (OnSubscriptionError)
	{
		OnSubscriptionError(Ctx);
	}
}

UDbConnection* ConnectThen(
	TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<void(UDbConnection*)> Callback)
{
	FString DbName;
	FString DbNameError;
	if (!GetDbName(DbName, DbNameError))
	{
		return nullptr;
	}

	UCredentials::Init(TestName);

	const FString ConnectTestName = FString::Printf(TEXT("on_connect_%s"), *TestName);
	Counter->Register(ConnectTestName);

	UTestHelperDelegates* TestHelper = NewObject<UTestHelperDelegates>();
	TestHelper->AddToRoot();

	TestHelper->OnConnect = [Counter, Callback, ConnectTestName](UDbConnection* Conn, FSpacetimeDBIdentity, const FString&)
	{
		Callback(Conn);
		Counter->MarkSuccess(ConnectTestName);
	};
	TestHelper->OnConnectError = [Counter, ConnectTestName](UDbConnection*, const FString& Error)
	{
		Counter->MarkFailure(ConnectTestName, FString::Printf(TEXT("Connect error: %s"), *Error));
	};
	TestHelper->OnDisconnect = [Counter, ConnectTestName](UDbConnection*, const FString& Error)
	{
		Counter->MarkFailure(ConnectTestName, FString::Printf(TEXT("Disconnected: %s"), *Error));
	};

	FOnConnectDelegate ConnectDelegate;
	BIND_DELEGATE_SAFE(ConnectDelegate, TestHelper, UTestHelperDelegates, HandleConnect);

	FOnDisconnectDelegate DisconnectDelegate;
	BIND_DELEGATE_SAFE(DisconnectDelegate, TestHelper, UTestHelperDelegates, HandleDisconnect);

	FOnConnectErrorDelegate ErrorDelegate;
	BIND_DELEGATE_SAFE(ErrorDelegate, TestHelper, UTestHelperDelegates, HandleConnectError);

	UDbConnection* Conn = UDbConnection::Builder()
		->WithUri(TEXT("localhost:3000"))
		->WithDatabaseName(DbName)
		->OnConnect(ConnectDelegate)
		->OnDisconnect(DisconnectDelegate)
		->OnConnectError(ErrorDelegate)
		->Build();

	if (Conn)
	{
		Conn->SetAutoTicking(true);
		Conn->AddToRoot();
	}

	return Conn;
}

bool GetDbName(FString& DBName, FString& Error)
{
	const FString DbNameEnv = FPlatformMisc::GetEnvironmentVariable(TEXT("SPACETIME_SDK_TEST_DB_NAME"));
	if (!DbNameEnv.IsEmpty())
	{
		DBName = DbNameEnv;
		return true;
	}

	FString CmdValue;
	if (FParse::Value(FCommandLine::Get(), TEXT("-SpacetimeDbName="), CmdValue))
	{
		DBName = CmdValue;
		return true;
	}

	Error = TEXT("No DB name. Pass -SpacetimeDbName=<name> or set SPACETIME_SDK_TEST_DB_NAME.");
	return false;
}

bool ValidateParameterConfig(FAutomationTestBase* Test)
{
	FString DbName;
	FString DbNameError;
	if (!GetDbName(DbName, DbNameError))
	{
		Test->AddError(DbNameError);
		return false;
	}
	return true;
}

bool ReportTestResult(FAutomationTestBase& Test, const FString& TestName, TSharedPtr<FTestCounter> Counter, bool bTimedOut)
{
	bool bHasFailure = false;

	for (const FString& Msg : Counter->GetFailures())
	{
		TESTLOG_FAIL(Test, TEXT("Operation - %s"), *Msg);
		bHasFailure = true;
	}
	for (const FString& Msg : Counter->GetSuccesses())
	{
		TESTLOG_SUCCESS(Test, TEXT("Operation - %s"), *Msg);
	}

	if (bTimedOut)
	{
		TESTLOG_FAIL(Test, TEXT("Timed out waiting for operation"));
		bHasFailure = true;
	}
	if (Counter->IsAborted())
	{
		TESTLOG_FAIL(Test, TEXT("Test aborted due to precondition failure"));
		bHasFailure = true;
	}

	if (!bHasFailure)
	{
		TESTLOG_SUCCESS(Test, TEXT("Test Success"));
		Test.TestTrue(*TestName, true);
	}
	else
	{
		TESTLOG_FAIL(Test, TEXT("Test failed"));
	}

	return !bHasFailure;
}
