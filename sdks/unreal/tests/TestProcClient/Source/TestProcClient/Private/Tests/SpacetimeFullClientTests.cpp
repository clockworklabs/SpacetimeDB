#include "Tests/SpacetimeFullClientTests.h"

#include "Tests/UmbreallaHeaderTypes.h"
#include "Tests/UmbreallaHeaderProcedures.h"

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"

#include "Tests/TestCounter.h"
#include "Tests/CommonTestFunctions.h"
#include "Tests/TestHandler.h"

#include "Connection/Credentials.h"
#include "ModuleBindings/Tables/MyTableTable.g.h"

// #include "HAL/IPlatformFile.h"

/**
 * @return True if the counter is complete or if the timeout is reached.
 */
bool FWaitForTestCounter::Update()
{
	const double Timeout = 90.0;
	bool bStopped = false;
	bool bTimedOut = false;

	if (Counter->IsAborted())
	{
		bStopped = true;
	}

	if (Counter->IsComplete())
	{
		bStopped = true;
	}

	if (FPlatformTime::Seconds() - StartTime > Timeout)
	{
		bTimedOut = true;
		bStopped = true;
	}

	if (bStopped)
	{
		ReportTestResult(Test, TestName, Counter, bTimedOut);
	}

	return bStopped;
}

// Helpers
static FString TrimFloat(double V)
{
	FString S = LexToString(V);
	// Remove trailing zeros after decimal and possible trailing dot
	int32 Dot = INDEX_NONE;
	if (S.FindChar(TEXT('.'), Dot))
	{
		while (S.Len() > Dot + 1 && S.EndsWith(TEXT("0")))
		{
			S.RemoveAt(S.Len() - 1);
		}
		if (S.EndsWith(TEXT(".")))
		{
			S.RemoveAt(S.Len() - 1);
		}
	}
	if (S == TEXT("-0"))
	{
		S = TEXT("0");
	}
	return S;
}

static FString NormalizeTimestamp(const FSpacetimeDBTimestamp &Ts)
{
	// Headers show ToString() -> "YYYY-MM-DDTHH:MM:SS.ffffffZ"
	// Your payload uses "+00:00".
	FString Out = Ts.ToString();
	if (Out.EndsWith(TEXT("Z")))
	{
		Out.LeftChopInline(1, EAllowShrinking::No);
		Out += TEXT("+00:00");
	}
	return Out;
}

static FString NormalizeDuration(const FSpacetimeDBTimeDuration &Dur)
{
	// Headers expose microseconds; payload prints seconds with fraction.
	const double Seconds = static_cast<double>(Dur.GetMicroseconds()) / 1'000'000.0;
	return TrimFloat(Seconds);
}
//

bool FProcedureBasicTest::RunTest(const FString &Parameters)
{
	TestName = "ProcedureBasicTest";

	if (!ValidateParameterConfig(this))
		return false;
	UProcedureHandler *Handler = CreateTestHandler<UProcedureHandler>();

	Handler->Counter->Register(TEXT("ReturnEnumA"));
	Handler->Counter->Register(TEXT("ReturnEnumB")); 
	Handler->Counter->Register(TEXT("ReturnPrimitive"));
	Handler->Counter->Register(TEXT("ReturnStruct"));
	//Handler->Counter->Register(TEXT("WillPanic"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn){
		FOnReturnEnumAComplete ReturnEnumACallback;
		BIND_DELEGATE_SAFE(ReturnEnumACallback, Handler, UProcedureHandler, OnReturnEnumA);		
		Conn->Procedures->ReturnEnumA(42, ReturnEnumACallback);

		FOnReturnEnumBComplete ReturnEnumBCallback;
		BIND_DELEGATE_SAFE(ReturnEnumBCallback, Handler, UProcedureHandler, OnReturnEnumB);		
		Conn->Procedures->ReturnEnumB(TEXT("Hello, SpacetimeDB!"), ReturnEnumBCallback);

		FOnReturnPrimitiveComplete ReturnPrimitiveCallback;
		BIND_DELEGATE_SAFE(ReturnPrimitiveCallback, Handler, UProcedureHandler, OnReturnPrimitive);		
		Conn->Procedures->ReturnPrimitive(42, 27, ReturnPrimitiveCallback);

		FOnReturnStructComplete ReturnStructCallback;
		BIND_DELEGATE_SAFE(ReturnStructCallback, Handler, UProcedureHandler, OnReturnStruct);		
		Conn->Procedures->ReturnStruct(42, TEXT("Hello, SpacetimeDB!"), ReturnStructCallback);

		FOnWillPanicComplete WillPanicCallback;
		BIND_DELEGATE_SAFE(WillPanicCallback, Handler, UProcedureHandler, OnWillPanic);		
		Conn->Procedures->WillPanic(WillPanicCallback);
	});

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}


bool FProcedureInsertTransactionCommitTest::RunTest(const FString &Parameters)
{
	TestName = "ProcedureInsertTransactionCommitTest";

	if (!ValidateParameterConfig(this))
		return false;
	UProcedureHandler *Handler = CreateTestHandler<UProcedureHandler>();
	Handler->Counter->Register(TEXT("OnSubscriptionAppliedNothing"));
	Handler->Counter->Register(TEXT("InsertWithTxCommitValues")); 
	Handler->Counter->Register(TEXT("InsertWithTxCommitCallback"));
	Handler->ExpectedMyTableRow = FMyTableType();
	Handler->ExpectedMyTableRow.Field = FReturnStructType();
	Handler->ExpectedMyTableRow.Field.A = 42;
	Handler->ExpectedMyTableRow.Field.B = "magic";

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn){
		// my_table on_insert - assert it matches expected value
		Conn->Db->MyTable->OnInsert.AddDynamic(Handler, &UProcedureHandler::OnInsertWithTxCommitMyTable);
		SubscribeAllThen(Conn, [Handler, Conn](FSubscriptionEventContext Ctx)
			{
				FString Name = TEXT("OnSubscriptionAppliedNothing");
				// assert my_table is still empty
				if (Conn->Db->MyTable->Count() == 0)
				{
					Handler->Counter->MarkSuccess(Name);
				}
				else
				{
					Handler->Counter->MarkFailure(Name, TEXT("Subscription had rows for MyTable"));
				}
				// call insert_with_tx_commit_then
					// result is okay
					// assert row matches expected value
				FOnInsertWithTxCommitComplete ReturnCallback;
				BIND_DELEGATE_SAFE(ReturnCallback, Handler, UProcedureHandler, OnReturnInsertTxCommit);
				Conn->Procedures->InsertWithTxCommit(ReturnCallback);
			}); 
	});		
	
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FProcedureInsertTransactionRollbackTest::RunTest(const FString &Parameters)
{
	TestName = "ProcedureInsertTransactionRollbackTest";

	if (!ValidateParameterConfig(this))
		return false;
	UProcedureHandler *Handler = CreateTestHandler<UProcedureHandler>();

	Handler->Counter->Register(TEXT("OnSubscriptionAppliedNothing"));
	Handler->Counter->Register(TEXT("InsertWithTxRollbackValues"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn){
		// my_table on_insert - assert it matches expected value
		Conn->Db->MyTable->OnInsert.AddDynamic(Handler, &UProcedureHandler::OnInsertWithTxRollbackMyTable);
		SubscribeAllThen(Conn, [Handler, Conn](FSubscriptionEventContext Ctx)
			{
				// assert my_table is still empty
				FString Name = TEXT("OnSubscriptionAppliedNothing");
				// assert my_table is still empty
				if (Conn->Db->MyTable->Count() == 0)
				{
					Handler->Counter->MarkSuccess(Name);
				}
				else
				{
					Handler->Counter->MarkFailure(Name, TEXT("Subscription had rows for MyTable"));
				}
			
				// call insert_with_tx_rollback_then
					// result is okay
					// assert row matches expected value
				FOnInsertWithTxRollbackComplete ReturnCallback;
				BIND_DELEGATE_SAFE(ReturnCallback, Handler, UProcedureHandler, OnReturnInsertTxRollback);
				Conn->Procedures->InsertWithTxRollback(ReturnCallback);
			}); 
	});

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}