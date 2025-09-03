#include "Tests/CommonTestFunctions.h"
#include "UObject/UnrealType.h"
#include "Connection/Credentials.h"

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

void UTestHelperDelegates::HandleSubscriptionEnd(FSubscriptionEventContextBase Ctx)
{
	if (OnSubscriptionEnd)
	{
		OnSubscriptionEnd(Ctx);
	}
}

void UTestHelperDelegates::HandleSubscriptionError(FErrorContext Ctx)
{
	if (OnSubscriptionError)
	{
		OnSubscriptionError(Ctx);
	}
}

static int32 GetTableCount(URemoteTable* Table)
{
	if (!Table)
	{
		return 0;
	}
	UFunction* CountFunc = Table->FindFunction(TEXT("Count"));
	if (!CountFunc)
	{
		return 0;
	}
	struct
	{
		int32 ReturnValue;
	} Params;
	Table->ProcessEvent(CountFunc, &Params);
	return Params.ReturnValue;
}

UDbConnection* ConnectWithThen(TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<UDbConnectionBuilder* (UDbConnectionBuilder*)> WithBuilder,
	TFunction<void(UDbConnection*)> Callback)
{

	FString DbName, DbNameError;
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


	UDbConnectionBuilder* Builder = UDbConnection::Builder()
		->WithUri(TEXT("localhost:3000"))
		->WithModuleName(DbName)
		->OnConnect(ConnectDelegate)
		->OnDisconnect(DisconnectDelegate)
		->OnConnectError(ErrorDelegate);

	if (WithBuilder)
	{
		Builder = WithBuilder(Builder);
	}

	UDbConnection* Conn = Builder->Build();

	if (Conn)
	{
		Conn->AddToRoot();
	}
	return Conn;
}

UDbConnection* ConnectThen(TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<void(UDbConnection*)> Callback)
{
	return ConnectWithThen(Counter, TestName, nullptr, Callback);
}

UDbConnection* Connect(TSharedPtr<FTestCounter> Counter)
{
	return ConnectThen(Counter, "", [](UDbConnection*) {});
}

void SubscribeAllThen(UDbConnection* Conn,
	TFunction<void(FSubscriptionEventContext)> Callback)
{
	UTestHelperDelegates* TestHelper = NewObject<UTestHelperDelegates>();
	TestHelper->AddToRoot();

	TestHelper->OnSubscriptionApplied = [Callback](FSubscriptionEventContext Ctx)
		{
			Callback(Ctx);
		};
	TestHelper->OnSubscriptionError = [](FErrorContext Ctx)
		{
			checkf(false, TEXT("Subscription errored: %s"), *Ctx.Error);
		};

	FOnSubscriptionApplied SubscriptionApplyDelegate;
	BIND_DELEGATE_SAFE(SubscriptionApplyDelegate, TestHelper, UTestHelperDelegates, HandleSubscriptionApplied);

	FOnSubscriptionError SubscriptionErrorDelegate;
	BIND_DELEGATE_SAFE(SubscriptionErrorDelegate, TestHelper, UTestHelperDelegates, HandleSubscriptionError);


	Conn->SubscriptionBuilder()
		->OnApplied(SubscriptionApplyDelegate)
		->OnError(SubscriptionErrorDelegate)
		->SubscribeToAllTables();
}

void SubscribeTheseThen(UDbConnection* Conn,
	const TArray<FString>& Queries,
	TFunction<void(FSubscriptionEventContext)> Callback)
{
	UTestHelperDelegates* TestHelper = NewObject<UTestHelperDelegates>();
	TestHelper->AddToRoot();

	TestHelper->OnSubscriptionApplied = [Callback](FSubscriptionEventContext Ctx)
		{
			Callback(Ctx);
		};
	TestHelper->OnSubscriptionError = [](FErrorContext Ctx)
		{
			checkf(false, TEXT("Subscription errored: %s"), *Ctx.Error);
		};

	FOnSubscriptionApplied SubscriptionApplyDelegate;
	BIND_DELEGATE_SAFE(SubscriptionApplyDelegate, TestHelper, UTestHelperDelegates, HandleSubscriptionApplied);

	FOnSubscriptionError SubscriptionErrorDelegate;
	BIND_DELEGATE_SAFE(SubscriptionErrorDelegate, TestHelper, UTestHelperDelegates, HandleSubscriptionError);

	Conn->SubscriptionBuilder()
		->OnApplied(SubscriptionApplyDelegate)
		->OnError(SubscriptionErrorDelegate)
		->Subscribe(Queries);
}

bool AssertTableEmpty(FAutomationTestBase* Test,
	URemoteTables* Db,
	const FString& TableName)
{
	if (!Db)
	{
		Test->AddError(TEXT("URemoteTables is null."));
		return false;
	}


	FProperty* TableProperty = Db->GetClass()->FindPropertyByName(*TableName);

	if (!TableProperty)
	{
		Test->AddError(FString::Printf(TEXT("No property named '%s' found on URemoteTables."), *TableName));
		return false;
	}

	FObjectProperty* ObjectProp = CastField<FObjectProperty>(TableProperty);
	if (!ObjectProp)
	{
		Test->AddError(FString::Printf(TEXT("Property '%s' is not an object property."), *TableName));
		return false;
	}

	UObject* TableObject = ObjectProp->GetObjectPropertyValue_InContainer(Db);
	if (!TableObject)
	{
		Test->AddError(FString::Printf(TEXT("Property '%s' is null."), *TableName));
		return false;
	}

	UFunction* CountFunc = TableObject->FindFunction(TEXT("Count"));
	if (!CountFunc || !CountFunc->GetReturnProperty() || !CountFunc->GetReturnProperty()->IsA(FIntProperty::StaticClass()))
	{
		Test->AddError(FString::Printf(TEXT("Function 'Count' not found or invalid on table '%s'."), *TableName));
		return false;
	}

	int32 RowCount = 0;
	TableObject->ProcessEvent(CountFunc, &RowCount);

	if (RowCount != 0)
	{
		Test->AddError(FString::Printf(TEXT("Expected table '%s' to be empty, but found %d rows."), *TableName, RowCount));
		return false;
	}

	return true;
}

bool AssertAllTablesEmpty(FAutomationTestBase* Test, URemoteTables* Db)
{
	if (!Db)
	{
		Test->AddError(TEXT("URemoteTables is null."));
		return false;
	}

	bool bAllEmpty = true;

	for (TFieldIterator<FObjectProperty> It(Db->GetClass()); It; ++It)
	{
		FObjectProperty* Property = *It;
		UObject* PropertyValue = Property->GetObjectPropertyValue_InContainer(Db);

		if (!PropertyValue)
		{
			Test->AddError(FString::Printf(TEXT("Property '%s' is null."), *Property->GetName()));
			bAllEmpty = false;
			continue;
		}

		UFunction* CountFunc = PropertyValue->FindFunction(TEXT("Count"));

		if (!CountFunc || !CountFunc->GetReturnProperty() || !CountFunc->GetReturnProperty()->IsA(FIntProperty::StaticClass()))
		{
			Test->AddError(FString::Printf(TEXT("Function 'Count' not found or invalid on property '%s'."), *Property->GetName()));
			bAllEmpty = false;
			continue;
		}

		int32 RowCount = 0;
		PropertyValue->ProcessEvent(CountFunc, &RowCount);

		if (RowCount > 0)
		{
			Test->AddError(FString::Printf(TEXT("Table '%s' is not empty (Count = %d)."), *Property->GetName(), RowCount));
			bAllEmpty = false;
		}
	}

	return bAllEmpty;
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

	// Config fallback (lets Session Frontend runs work without CLI args)
	const USpacetimeDBSettings* Settings = GetDefault<USpacetimeDBSettings>();
	if (!Settings->SpacetimeDbTestName.IsEmpty())
	{
		DBName = Settings->SpacetimeDbTestName;
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
	for(const FString& Msg : Counter->GetSuccesses())
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