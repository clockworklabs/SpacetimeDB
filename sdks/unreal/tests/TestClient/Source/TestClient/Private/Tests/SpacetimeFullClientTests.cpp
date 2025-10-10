#include "Tests/SpacetimeFullClientTests.h"

#include "Tests/UmbreallaHeaderaTables.h"
#include "Tests/UmbreallaHeaderTypes.h"
#include "Tests/UmbreallaHeaderReducers.h"

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"

#include "Tests/TestCounter.h"
#include "Tests/CommonTestFunctions.h"
#include "Tests/TestHandler.h"

#include "Tests/PrimitiveHandlerList.def"

#include "Connection/Credentials.h"

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

bool FInsertPrimitiveTest::RunTest(const FString &Parameters)
{

	TestName = "InsertPrimitive";

	if (!ValidateParameterConfig(this))
		return false;
	UInsertPrimitiveHandler *Handler = CreateTestHandler<UInsertPrimitiveHandler>();

#define REG(Suffix, Expected, RowStructType) Handler->Counter->Register(TEXT("InsertOne" #Suffix));
	FOREACH_PRIMITIVE(REG)
#undef REG

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
#define BIND_HANDLER(Suffix, Expected, RowStructType) Conn->Db->One##Suffix->OnInsert.AddDynamic(Handler, &UInsertPrimitiveHandler::OnInsertOne##Suffix);
			FOREACH_PRIMITIVE(BIND_HANDLER)
#undef BIND_HANDLER

			//Conn->Db->OneI8->OnInsert.AddDynamic(Handler, &UTestHandler::OnInsertOneI8);

		SubscribeAllThen(Conn,[this, Handler, Conn](FSubscriptionEventContext Ctx)
		{
			if (!AssertAllTablesEmpty(this, Conn->Db))
			{
				Handler->Counter->Abort();
				return;
			}

#define CALL_INSERT(Suffix, Expected, RowStructType) Ctx.Reducers->InsertOne##Suffix(Expected);
				FOREACH_PRIMITIVE(CALL_INSERT)
#undef CALL_INSERT
		}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FSubscribeAndCancelTest::RunTest(const FString &Parameters)
{
	TestName = "SubscribeAndCancel";

	if (!ValidateParameterConfig(this))
		return false;

	TSharedPtr<FTestCounter> Counter = MakeShared<FTestCounter>();
	Counter->Register(TEXT("unsubscribe_then_called"));

	UDbConnection *Connection = ConnectThen(Counter, TestName, [this, Counter](UDbConnection *Conn)
											{
			UTestHelperDelegates* Helper = NewObject<UTestHelperDelegates>();
			Helper->AddToRoot();

			Helper->OnSubscriptionError = [Counter](FErrorContext) { Counter->MarkFailure(TEXT("unsubscribe_then_called"), TEXT("Subscription errored")); };

			FOnSubscriptionApplied AppliedDelegate; BIND_DELEGATE_SAFE(AppliedDelegate, Helper, UTestHelperDelegates, HandleSubscriptionApplied);
			FOnSubscriptionError ErrorDelegate; BIND_DELEGATE_SAFE(ErrorDelegate, Helper, UTestHelperDelegates, HandleSubscriptionError);

			USubscriptionHandle* Handle = Conn->SubscriptionBuilder()->OnApplied(AppliedDelegate)->OnError(ErrorDelegate)->Subscribe({ TEXT("SELECT * FROM one_u8;") });

			UTestHelperDelegates* EndHelper = NewObject<UTestHelperDelegates>();
			EndHelper->AddToRoot();
			EndHelper->OnSubscriptionEnd = [Counter, Handle](FSubscriptionEventContextBase)
				{
					(!Handle->IsActive() && Handle->IsEnded()) ? Counter->MarkSuccess(TEXT("unsubscribe_then_called"))
						: Counter->MarkFailure(TEXT("unsubscribe_then_called"), TEXT("Unexpected handle state"));
				};
			FSubscriptionEventDelegate EndDelegate; BIND_DELEGATE_SAFE(EndDelegate, EndHelper, UTestHelperDelegates, HandleSubscriptionEnd);
			Handle->UnsubscribeThen(EndDelegate); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Counter, FPlatformTime::Seconds()));
	return true;
}

bool FSubscribeAndUnsubscribeTest::RunTest(const FString &Parameters)
{
	TestName = "SubscribeAndUnsubscribe";

	if (!ValidateParameterConfig(this))
		return false;

	TSharedPtr<FTestCounter> Counter = MakeShared<FTestCounter>();
	Counter->Register(TEXT("unsubscribe_then_called"));

	// Use a struct to manage the state across different asynchronous calls.
	struct FTestState
	{
		UTestHelperDelegates *Helper = nullptr;
		USubscriptionHandle *Handle = nullptr;
		TSharedPtr<FTestCounter> Counter;
		UDbConnection *Conn = nullptr;
	};
	TSharedPtr<FTestState> State = MakeShared<FTestState>();
	State->Counter = Counter;

	UDbConnection *Connection = ConnectThen(Counter, TestName, [this, State](UDbConnection *Conn)
											{
			State->Conn = Conn;
			Conn->Reducers->InsertOneU8(1);

			State->Helper = NewObject<UTestHelperDelegates>();
			State->Helper->AddToRoot();

			State->Helper->OnSubscriptionApplied = [State](FSubscriptionEventContext Ctx)
				{
					// Ensure the handle is valid and active on subscription.
					if (!State->Handle || !State->Handle->IsActive() || State->Handle->IsEnded())
					{
						State->Counter->MarkFailure(TEXT("unsubscribe_then_called"), TEXT("Subscription handle is not active after subscription applied."));
						State->Helper->RemoveFromRoot();
						return;
					}

					if (Ctx.Db->OneU8->Count() != 1)
					{
						State->Counter->MarkFailure(TEXT("unsubscribe_then_called"), TEXT("Initial OneU8 row count not 1."));
						State->Helper->RemoveFromRoot();
						return;
					}

					// This lambda will run when UnsubscribeThen completes.
					State->Helper->OnSubscriptionEnd = [State](FSubscriptionEventContextBase)
						{
							// Check the final state.
							if (State->Handle && State->Handle->IsEnded() && !State->Handle->IsActive())
							{
								State->Counter->MarkSuccess(TEXT("unsubscribe_then_called"));
							}
							else
							{
								State->Counter->MarkFailure(TEXT("unsubscribe_then_called"), TEXT("Final handle state is incorrect."));
							}

							// Clean up after the test is complete.
							if (State->Helper)
							{
								State->Helper->RemoveFromRoot();
							}
						};

					FSubscriptionEventDelegate EndDelegate;
					BIND_DELEGATE_SAFE(EndDelegate, State->Helper, UTestHelperDelegates, HandleSubscriptionEnd);
					State->Handle->UnsubscribeThen(EndDelegate);
				};

			State->Helper->OnSubscriptionError = [State](const FErrorContext& ErrorContext)
				{
					State->Counter->MarkFailure(TEXT("unsubscribe_then_called"), FString::Printf(TEXT("Subscription Error %s"), *ErrorContext.Error));
					if (State->Helper)
					{
						State->Helper->RemoveFromRoot();
					}
				};

			FOnSubscriptionApplied AppliedDelegate; BIND_DELEGATE_SAFE(AppliedDelegate, State->Helper, UTestHelperDelegates, HandleSubscriptionApplied);
			FOnSubscriptionError ErrorDelegate; BIND_DELEGATE_SAFE(ErrorDelegate, State->Helper, UTestHelperDelegates, HandleSubscriptionError);

			// The handle is now stored in the state struct.
			State->Handle = Conn->SubscriptionBuilder()->OnApplied(AppliedDelegate)->OnError(ErrorDelegate)->Subscribe({ TEXT("SELECT * FROM one_u8;") }); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Counter, FPlatformTime::Seconds()));
	return true;
}

bool FSubscriptionErrorSmokeTest::RunTest(const FString &Parameters)
{
	TestName = "SubscriptionErrorSmokeTest";

	if (!ValidateParameterConfig(this))
		return false;

	TSharedPtr<FTestCounter> Counter = MakeShared<FTestCounter>();
	Counter->Register(TEXT("error_callback_is_called"));

	UDbConnection *Connection = ConnectThen(Counter, TestName, [this, Counter](UDbConnection *Conn)
											{
			UTestHelperDelegates* Helper = NewObject<UTestHelperDelegates>();
			Helper->AddToRoot();
			Helper->OnSubscriptionApplied = [Counter](FSubscriptionEventContext) { Counter->MarkFailure(TEXT("error_callback_is_called"), TEXT("Subscription should never be applied")); };
			Helper->OnSubscriptionError = [Counter](FErrorContext) { Counter->MarkSuccess(TEXT("error_callback_is_called")); };

			FOnSubscriptionApplied AppliedDelegate; BIND_DELEGATE_SAFE(AppliedDelegate, Helper, UTestHelperDelegates, HandleSubscriptionApplied);
			FOnSubscriptionError ErrorDelegate; BIND_DELEGATE_SAFE(ErrorDelegate, Helper, UTestHelperDelegates, HandleSubscriptionError);

			USubscriptionHandle* Handle = Conn->SubscriptionBuilder()->OnApplied(AppliedDelegate)->OnError(ErrorDelegate)->Subscribe({ TEXT("SELEcCT * FROM one_u8;") }); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Counter, FPlatformTime::Seconds()));
	return true;
}

bool FDeletePrimitiveTest::RunTest(const FString &Parameters)
{
	TestName = "DeletePrimitive";

	if (!ValidateParameterConfig(this))
		return false;
	UDeletePrimitiveHandler *Handler = CreateTestHandler<UDeletePrimitiveHandler>();

#define REG_UNIQUE(Suffix, Field, Literal, Expected, RowStructType) \
	Handler->Counter->Register(TEXT("InsertUnique" #Suffix));       \
	Handler->Counter->Register(TEXT("DeleteUnique" #Suffix));
	FOREACH_UNIQUE_PRIMITIVE(REG_UNIQUE)
#undef REG_UNIQUE

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
#define BIND_UNIQUE(Suffix, Field, Literal, Expected, RowStructType)                                          \
	Conn->Db->Unique##Suffix->OnInsert.AddDynamic(Handler, &UDeletePrimitiveHandler::OnInsertUnique##Suffix); \
	Conn->Db->Unique##Suffix->OnDelete.AddDynamic(Handler, &UDeletePrimitiveHandler::OnDeleteUnique##Suffix);
				FOREACH_UNIQUE_PRIMITIVE(BIND_UNIQUE)
#undef BIND_UNIQUE

					SubscribeAllThen(Conn, [Handler](FSubscriptionEventContext Ctx)
						{
#define CALL_UNIQUE(Suffix, Field, Literal, Expected, RowStructType) Ctx.Reducers->InsertUnique##Suffix(Literal, Expected);
							FOREACH_UNIQUE_PRIMITIVE(CALL_UNIQUE)
#undef CALL_UNIQUE
						}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FUpdatePrimitiveTest::RunTest(const FString &Parameters)
{
	TestName = "UpdatePrimitive";

	if (!ValidateParameterConfig(this))
		return false;
	UUpdatePrimitiveHandler *Handler = CreateTestHandler<UUpdatePrimitiveHandler>();

#define REG_PK(Suffix, Field, Literal, Expected, Updated, RowStructType) \
	Handler->Counter->Register(TEXT("InsertPk" #Suffix));                \
	Handler->Counter->Register(TEXT("UpdatePk" #Suffix));                \
	Handler->Counter->Register(TEXT("DeletePk" #Suffix));
	FOREACH_PK_PRIMITIVE(REG_PK)
#undef REG_PK

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
#define BIND_PK(Suffix, Field, Literal, Expected, Updated, RowStructType)                             \
	Conn->Db->Pk##Suffix->OnInsert.AddDynamic(Handler, &UUpdatePrimitiveHandler::OnInsertPk##Suffix); \
	Conn->Db->Pk##Suffix->OnUpdate.AddDynamic(Handler, &UUpdatePrimitiveHandler::OnUpdatePk##Suffix); \
	Conn->Db->Pk##Suffix->OnDelete.AddDynamic(Handler, &UUpdatePrimitiveHandler::OnDeletePk##Suffix);
				FOREACH_PK_PRIMITIVE(BIND_PK)
#undef BIND_PK

					SubscribeAllThen(Conn, [Handler](FSubscriptionEventContext Ctx)
						{
#define CALL_PK(Suffix, Field, Literal, Expected, Updated, RowStructType) Ctx.Reducers->InsertPk##Suffix(Literal, Expected);
							FOREACH_PK_PRIMITIVE(CALL_PK)
#undef CALL_PK
						}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertOneIdentityTest::RunTest(const FString &Parameters)
{
	TestName = "InsertIdentity";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UIdentityActionsHandler *Handler = CreateTestHandler<UIdentityActionsHandler>();
	Handler->Counter->Register(TEXT("InsertIdentity"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->OneIdentity->OnInsert.AddDynamic(Handler, &UIdentityActionsHandler::OnInsertOneIdentity);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// Create a sample FSpacetimeDBIdentity.
					FSpacetimeDBIdentity Identity;
					Identity.FromHex("0xc2006697ed2cc4ebc5384a50527a92245ee7432cebe028e5648cb00a17c02a0e");
					Handler->SetExpectedValue(Identity);

					// Call the reducer to insert the new identity.
					Ctx.Reducers->InsertOneIdentity(Identity);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertOneConnectionIdTest::RunTest(const FString &Parameters)
{
	TestName = "InsertConnectionId";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UConnectionIdActionsHandler *Handler = CreateTestHandler<UConnectionIdActionsHandler>();
	Handler->Counter->Register(TEXT("InsertConnectionId"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->OneConnectionId->OnInsert.AddDynamic(Handler, &UConnectionIdActionsHandler::OnInsertOneConnectionId);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (Ctx.Db->OneConnectionId->Count() != 0)
					{
						Handler->Counter->Abort();
						return;
					}

					// Create a sample ConnectionId.
					FSpacetimeDBConnectionId ConnectionId;
					Handler->SetExpectedvalue(ConnectionId, 1);

					// Call the reducer to insert the new connectionId.
					Ctx.Reducers->InsertOneConnectionId(ConnectionId);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertUniqueConnectionIdTest::RunTest(const FString &Parameters)
{
	TestName = "InsertUniqueConnectionId";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UConnectionIdActionsHandler *InsertHandler = CreateTestHandler<UConnectionIdActionsHandler>();
	InsertHandler->Counter->Register(TEXT("InsertUniqueConnectionId"));

	UDbConnection *Connection = ConnectThen(InsertHandler->Counter, TestName, [this, InsertHandler](UDbConnection *Conn)
											{
			Conn->Db->UniqueConnectionId->OnInsert.AddDynamic(InsertHandler, &UConnectionIdActionsHandler::OnInsertUniqueConnectionId);

			SubscribeAllThen(Conn, [this, InsertHandler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						InsertHandler->Counter->Abort();
						return;
					}
					// Create a sample FSpacetimeDBIdentity.
					FSpacetimeDBConnectionId ConnectionId;

					int32 Data = 1;
					InsertHandler->SetExpectedvalue(ConnectionId, Data);

					// Call the reducer to insert the identity first.
					Ctx.Reducers->InsertUniqueConnectionId(ConnectionId, Data);

				}); });
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, InsertHandler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertCallerIdentityTest::RunTest(const FString &Parameters)
{
	TestName = "InsertCallerIdentity";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UIdentityActionsHandler *InsertHandler = CreateTestHandler<UIdentityActionsHandler>();
	InsertHandler->Counter->Register(TEXT("InsertCallerIdentity"));

	UDbConnection *Connection = ConnectThen(InsertHandler->Counter, TestName, [this, InsertHandler](UDbConnection *Conn)
											{
			Conn->Db->OneIdentity->OnInsert.AddDynamic(InsertHandler, &UIdentityActionsHandler::OnInsertCallerIdentity);

			SubscribeAllThen(Conn, [this, InsertHandler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						InsertHandler->Counter->Abort();
						return;
					}

					// Call the reducer to insert the identity first.
					Ctx.Reducers->InsertCallerOneIdentity();

				}); });
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, InsertHandler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertCallerConnectionIdTest::RunTest(const FString &Parameters)
{
	TestName = "InsertCallerConnectionId";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UConnectionIdActionsHandler *InsertHandler = CreateTestHandler<UConnectionIdActionsHandler>();
	InsertHandler->Counter->Register(TEXT("InsertCallerConnectionId"));

	UDbConnection *Connection = ConnectThen(InsertHandler->Counter, TestName, [this, InsertHandler](UDbConnection *Conn)
											{
			Conn->Db->OneConnectionId->OnInsert.AddDynamic(InsertHandler, &UConnectionIdActionsHandler::OnInsertCallerConnectionId);

			SubscribeAllThen(Conn, [this, InsertHandler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						InsertHandler->Counter->Abort();
						return;
					}

					// Call the reducer to insert the identity first.
					Ctx.Reducers->InsertCallerOneConnectionId();
				}); });
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, InsertHandler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertOneTimestampTest::RunTest(const FString &Parameters)
{
	TestName = "InsertTimestamp";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UTimestampActionsHandler *Handler = CreateTestHandler<UTimestampActionsHandler>();
	Handler->Counter->Register(TEXT("InsertTimestamp"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->OneTimestamp->OnInsert.AddDynamic(Handler, &UTimestampActionsHandler::OnInsertOneTimestamp);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (Ctx.Db->OneTimestamp->Count() != 0)
					{
						Handler->Counter->Abort();
						return;
					}

					FSpacetimeDBTimestamp TimeStamp;
					Handler->SetExpectedvalue(TimeStamp);

					// Call the reducer to insert the new Timestamp.
					Ctx.Reducers->InsertOneTimestamp(TimeStamp);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertCallTimestampTest::RunTest(const FString &Parameters)
{
	TestName = "InsertCallTimestamp";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UTimestampActionsHandler *Handler = CreateTestHandler<UTimestampActionsHandler>();
	Handler->Counter->Register(TEXT("InsertCallTimestamp"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Reducers->OnInsertCallTimestamp.AddDynamic(Handler, &UTimestampActionsHandler::OnInsertCallTimestamp);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (Ctx.Db->OneTimestamp->Count() != 0)
					{
						Handler->Counter->Abort();
						return;
					}

					// Call the reducer to insert the new Timestamp.
					Ctx.Reducers->InsertCallTimestamp();
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

// Updates
bool FUpdatePkIdentityTest::RunTest(const FString &Parameters)
{
	TestName = "UpdateIdentity";

	if (!ValidateParameterConfig(this))
		return false;

	UIdentityActionsHandler *Handler = CreateTestHandler<UIdentityActionsHandler>();
	Handler->Counter->Register(TEXT("PkIdentity_Insert"));
	Handler->Counter->Register(TEXT("PkIdentity_Update"));
	Handler->Counter->Register(TEXT("PkIdentity_Delete"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->PkIdentity->OnInsert.AddDynamic(Handler, &UIdentityActionsHandler::OnInsertPkIdentity);
			Conn->Db->PkIdentity->OnUpdate.AddDynamic(Handler, &UIdentityActionsHandler::OnUpdatePkIdentity);
			Conn->Db->PkIdentity->OnDelete.AddDynamic(Handler, &UIdentityActionsHandler::OnDeletePkIdentity);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// Create a sample FSpacetimeDBIdentity.
					FSpacetimeDBIdentity Identity;
					Ctx.TryGetIdentity(Identity);
					int32 InsertData = 3;
					int32 UpdateData = 4;
					Handler->SetExpectedValue(Identity, InsertData, UpdateData);

					// Call the reducer to insert the new identity.
					Ctx.Reducers->InsertPkIdentity(Identity, InsertData);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FUpdatePkConnectionIdTest::RunTest(const FString &Parameters)
{
	TestName = "UpdateConnectionId";

	if (!ValidateParameterConfig(this))
		return false;

	UConnectionIdActionsHandler *InsertHandler = CreateTestHandler<UConnectionIdActionsHandler>();
	InsertHandler->Counter->Register(TEXT("PkConnectionId_Insert"));
	InsertHandler->Counter->Register(TEXT("PkConnectionId_Update"));

	UDbConnection *Connection = ConnectThen(InsertHandler->Counter, TestName, [this, InsertHandler](UDbConnection *Conn)
											{
			Conn->Db->PkConnectionId->OnInsert.AddDynamic(InsertHandler, &UConnectionIdActionsHandler::OnInsertPkConnectionId);
			Conn->Db->PkConnectionId->OnUpdate.AddDynamic(InsertHandler, &UConnectionIdActionsHandler::OnUpdatePkConnectionId);

			SubscribeAllThen(Conn, [this, InsertHandler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						InsertHandler->Counter->Abort();
						return;
					}

					FSpacetimeDBConnectionId ConnectionId;
					int32 Data = 1;

					Ctx.Reducers->InsertPkConnectionId(ConnectionId, Data);
				}); });
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, InsertHandler->Counter, FPlatformTime::Seconds()));
	return true;
}

//
bool FDeleteUniqueIdentityTest::RunTest(const FString &Parameters)
{
	TestName = "DeleteIdentity";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UIdentityActionsHandler *Handler = CreateTestHandler<UIdentityActionsHandler>();
	Handler->Counter->Register(TEXT("UniqueIdentity_Insert"));
	Handler->Counter->Register(TEXT("UniqueIdentity_Delete"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->UniqueIdentity->OnInsert.AddDynamic(Handler, &UIdentityActionsHandler::OnInsertUniqueIdentity);
			Conn->Db->UniqueIdentity->OnDelete.AddDynamic(Handler, &UIdentityActionsHandler::OnDeleteUniqueIdentity);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Create a sample FSpacetimeDBIdentity.
					FSpacetimeDBIdentity Identity;
					int32 Data = 0;

					// Call the reducer to insert the identity first.
					Ctx.Reducers->InsertUniqueIdentity(Identity, Data);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FDeletePkConnectionIdTest::RunTest(const FString &Parameters)
{
	TestName = "DeleteConnectionId";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UConnectionIdActionsHandler *Handler = CreateTestHandler<UConnectionIdActionsHandler>();
	Handler->Counter->Register(TEXT("PkConnectionId_Insert"));
	Handler->Counter->Register(TEXT("PkConnectionId_Update"));
	Handler->Counter->Register(TEXT("PkConnectionId_Delete"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->PkConnectionId->OnInsert.AddDynamic(Handler, &UConnectionIdActionsHandler::OnInsertPkConnectionId);
			Conn->Db->PkConnectionId->OnUpdate.AddDynamic(Handler, &UConnectionIdActionsHandler::OnUpdatePkConnectionId);
			Conn->Db->PkConnectionId->OnDelete.AddDynamic(Handler, &UConnectionIdActionsHandler::OnDeletePkConnectionId);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Create a sample ConnectionId.
					FSpacetimeDBConnectionId ConnectionId;
					int32 Data = 0;

					Handler->SetExpectedvalue(ConnectionId, Data);

					// Call the reducer to insert the ConnectionId first.
					Ctx.Reducers->InsertPkConnectionId(ConnectionId, Data);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FOnReducerTest::RunTest(const FString &Parameters)
{
	TestName = "OnReducer";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UOnReducerActionsHandler *Handler = CreateTestHandler<UOnReducerActionsHandler>();
	Handler->Counter->Register(TEXT("OnReducer"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Reducers->OnInsertOneU8.AddDynamic(Handler, &UOnReducerActionsHandler::OnInsertOneU8);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// Create a sample FSpacetimeDBIdentity.
					uint8 Value = 0;
					Handler->SetExpectedvalue(Value);

					// Call the reducer to insert the new identity.
					Ctx.Reducers->InsertOneU8(Value);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FOnFailReducerTest::RunTest(const FString &Parameters)
{
	TestName = "FailReducer";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UOnReducerActionsHandler *Handler = CreateTestHandler<UOnReducerActionsHandler>();

	// Register counters for both success and failure states.
	Handler->Counter->Register(TEXT("Reducer-Callback-Success"));
	Handler->Counter->Register(TEXT("Reducer-Callback-Fail"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Reducers->OnInsertPkU8.AddDynamic(Handler, &UOnReducerActionsHandler::OnInsertPkU8);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// Use a key-value pair for a table with a primary key.
					uint8 Key = 128;
					int32 InitialData = 0xbeef;
					int32 FailData = 0xbabe;
					Handler->SetExpectedKeyAndValue(Key, InitialData, FailData);

					// Trigger the first, successful insertion.
					Ctx.Reducers->InsertPkU8(Key, InitialData);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FInsertVecTest::RunTest(const FString &Parameters)
{
	TestName = "InsertVec";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UVectorDataActionsHandler *VectorHandler = CreateTestHandler<UVectorDataActionsHandler>();
	VectorHandler->Counter->Register(TEXT("InsertVecU8"));
	VectorHandler->Counter->Register(TEXT("InsertVecU16"));
	VectorHandler->Counter->Register(TEXT("InsertVecU32"));
	VectorHandler->Counter->Register(TEXT("InsertVecU64"));
	VectorHandler->Counter->Register(TEXT("InsertVecU128"));
	VectorHandler->Counter->Register(TEXT("InsertVecU256"));
	VectorHandler->Counter->Register(TEXT("InsertVecI8"));
	VectorHandler->Counter->Register(TEXT("InsertVecI16"));
	VectorHandler->Counter->Register(TEXT("InsertVecI32"));
	VectorHandler->Counter->Register(TEXT("InsertVecI64"));
	VectorHandler->Counter->Register(TEXT("InsertVecI128"));
	VectorHandler->Counter->Register(TEXT("InsertVecI256"));

	VectorHandler->Counter->Register(TEXT("InsertVecBool"));
	VectorHandler->Counter->Register(TEXT("InsertVecF32"));
	VectorHandler->Counter->Register(TEXT("InsertVecF64"));
	VectorHandler->Counter->Register(TEXT("InsertVecString"));

	VectorHandler->Counter->Register(TEXT("InsertVecIdentity"));
	VectorHandler->Counter->Register(TEXT("InsertVecConnectionId"));
	VectorHandler->Counter->Register(TEXT("InsertVecTimestamp"));

	UDbConnection *Connection = ConnectThen(VectorHandler->Counter, TestName, [this, VectorHandler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->VecU8->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU8);
			Conn->Db->VecU16->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU16);
			Conn->Db->VecU32->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU32);
			Conn->Db->VecU64->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU64);
			Conn->Db->VecU128->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU128);
			Conn->Db->VecU256->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecU256);

			Conn->Db->VecI8->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI8);
			Conn->Db->VecI16->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI16);
			Conn->Db->VecI32->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI32);
			Conn->Db->VecI64->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI64);
			Conn->Db->VecI128->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI128);
			Conn->Db->VecI256->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecI256);

			Conn->Db->VecBool->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecBool);

			Conn->Db->VecF32->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecF32);
			Conn->Db->VecF64->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecF64);

			Conn->Db->VecString->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecString);

			Conn->Db->VecIdentity->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecIdentity);
			Conn->Db->VecConnectionId->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecConnectionId);
			Conn->Db->VecTimestamp->OnInsert.AddDynamic(VectorHandler, &UVectorDataActionsHandler::OnInsertVecTimestamp);

			SubscribeAllThen(Conn, [this, VectorHandler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						VectorHandler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls ---
					FSpacetimeDBUInt128 UInt128_One = { 0, 1 };
					FSpacetimeDBUInt128 UInt128_Zero = { 0, 0 };
					FSpacetimeDBUInt256 UInt256_One = { UInt128_Zero, UInt128_One };
					FSpacetimeDBUInt256 UInt256_Zero = { UInt128_Zero, UInt128_Zero };

					FSpacetimeDBInt128 Int128_One = { 0, 1 };
					FSpacetimeDBInt128 Int128_Zero = { 0, 0 };
					FSpacetimeDBInt256 Int256_One = { UInt128_Zero, UInt128_One };
					FSpacetimeDBInt256 Int256_Zero = { UInt128_Zero, UInt128_One };

					// Unsigned Integers
					Ctx.Reducers->InsertVecU8({ 2, 6 });
					VectorHandler->ExpectedVecU8 = FVecU8Type({ 2, 6 });
					Ctx.Reducers->InsertVecU16({ 3, 5 });
					VectorHandler->ExpectedVecU16 = FVecU16Type({ 3, 5 });
					Ctx.Reducers->InsertVecU32({ 1, 9 });
					VectorHandler->ExpectedVecU32 = FVecU32Type({ 1, 9 });
					Ctx.Reducers->InsertVecU64({ 3, 8 });
					VectorHandler->ExpectedVecU64 = FVecU64Type({ 3, 8 });
					Ctx.Reducers->InsertVecU128({ UInt128_Zero, UInt128_One });
					VectorHandler->ExpectedVecU128 = FVecU128Type({ UInt128_Zero, UInt128_One });
					Ctx.Reducers->InsertVecU256({ UInt256_Zero, UInt256_One });
					VectorHandler->ExpectedVecU256 = FVecU256Type({ UInt256_Zero, UInt256_One });

					// Signed Integers
					Ctx.Reducers->InsertVecI8({ 4, 5 });
					VectorHandler->ExpectedVecI8 = FVecI8Type({ 4, 5 });
					Ctx.Reducers->InsertVecI16({ 6, 3 });
					VectorHandler->ExpectedVecI16 = FVecI16Type({ 6, 3 });
					Ctx.Reducers->InsertVecI32({ 2, 1 });
					VectorHandler->ExpectedVecI32 = FVecI32Type({ 2, 1 });
					Ctx.Reducers->InsertVecI64({ 7, 9 });
					VectorHandler->ExpectedVecI64 = FVecI64Type({ 7, 9 });
					Ctx.Reducers->InsertVecI128({ Int128_Zero, Int128_One });
					VectorHandler->ExpectedVecI128 = FVecI128Type({ Int128_Zero, Int128_One });
					Ctx.Reducers->InsertVecI256({ Int256_Zero, Int256_One });
					VectorHandler->ExpectedVecI256 = FVecI256Type({ Int256_Zero, Int256_One });

					// Booleans
					Ctx.Reducers->InsertVecBool({ false, true });
					VectorHandler->ExpectedVecBool = FVecBoolType({ false, true });

					// Floating-point numbers
					Ctx.Reducers->InsertVecF32({ 0.0f, 1.0f });
					VectorHandler->ExpectedVecF32 = FVecF32Type({ 0.0f, 1.0f });
					Ctx.Reducers->InsertVecF64({ 0.0, 1.0 });
					VectorHandler->ExpectedVecF64 = FVecF64Type({ 0.0, 1.0 });

					// Strings
					Ctx.Reducers->InsertVecString({ "zero", "one" });
					VectorHandler->ExpectedVecString = FVecStringType({ "zero", "one" });

					// Other types
					FSpacetimeDBIdentity Identity;
					FSpacetimeDBConnectionId ConnectionId;
					FSpacetimeDBTimestamp TimeStamp;

					Ctx.Reducers->InsertVecIdentity({ Identity });
					VectorHandler->ExpectedVecIdentity = FVecIdentityType({ Identity });
					Ctx.Reducers->InsertVecConnectionId({ ConnectionId });
					VectorHandler->ExpectedVecConnectionId = FVecConnectionIdType({ ConnectionId });
					Ctx.Reducers->InsertVecTimestamp({ TimeStamp });
					VectorHandler->ExpectedVecTimestamp = FVecTimestampType({ TimeStamp });
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, VectorHandler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertOptionSomeTest::RunTest(const FString &Parameters)
{
	TestName = "InsertOptionSome";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UOptionActionsHandler *Handler = CreateTestHandler<UOptionActionsHandler>();
	Handler->Counter->Register(TEXT("InsertOptionI32"));
	Handler->Counter->Register(TEXT("InsertOptionString"));
	Handler->Counter->Register(TEXT("InsertOptionIdentity"));
	Handler->Counter->Register(TEXT("InsertOptionSimpleEnum"));
	Handler->Counter->Register(TEXT("InsertOptionEveryPrimitiveStruct"));
	Handler->Counter->Register(TEXT("InsertOptionVecOptionI32"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->OptionI32->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionI32);
			Conn->Db->OptionString->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionString);
			Conn->Db->OptionIdentity->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionIdentity);
			Conn->Db->OptionSimpleEnum->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionSimpleEnum);
			Conn->Db->OptionEveryPrimitiveStruct->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionPrimitiveStruct);
			Conn->Db->OptionVecOptionI32->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionVecOptionI32);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//
					FTestClientOptionalIdentity OptionalIdentity;
					FSpacetimeDBIdentity Identity;
					Ctx.TryGetIdentity(Identity);

					Handler->ExpectedI32Type = FTestClientOptionalInt32(0);
					Handler->ExpectedStringType = FTestClientOptionalString("string");
					Handler->ExpectedIdentityType = FTestClientOptionalIdentity(Identity);
					Handler->ExpectedEnumType = FTestClientOptionalSimpleEnum(ESimpleEnumType::Zero);
					Handler->ExpectedEveryPrimitiveStructType = FTestClientOptionalEveryPrimitiveStruct();
					Handler->ExpectedVecOptionI32Type = FTestClientOptionalVecOptionalInt32(TArray({ FTestClientOptionalInt32(0), FTestClientOptionalInt32() }));

					Ctx.Reducers->InsertOptionI32(FTestClientOptionalInt32(0));
					Ctx.Reducers->InsertOptionString(FTestClientOptionalString("string"));
					Ctx.Reducers->InsertOptionIdentity(FTestClientOptionalIdentity(Identity));
					Ctx.Reducers->InsertOptionSimpleEnum(FTestClientOptionalSimpleEnum(ESimpleEnumType::Zero));
					Ctx.Reducers->InsertOptionEveryPrimitiveStruct(FTestClientOptionalEveryPrimitiveStruct());
					Ctx.Reducers->InsertOptionVecOptionI32(FTestClientOptionalVecOptionalInt32(TArray({ FTestClientOptionalInt32(0), FTestClientOptionalInt32() })));
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertOptionNoneTest::RunTest(const FString &Parameters)
{
	TestName = "InsertOptionNone";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UOptionActionsHandler *Handler = CreateTestHandler<UOptionActionsHandler>();
	Handler->Counter->Register(TEXT("InsertOptionI32"));
	Handler->Counter->Register(TEXT("InsertOptionString"));
	Handler->Counter->Register(TEXT("InsertOptionIdentity"));
	Handler->Counter->Register(TEXT("InsertOptionSimpleEnum"));
	Handler->Counter->Register(TEXT("InsertOptionEveryPrimitiveStruct"));
	Handler->Counter->Register(TEXT("InsertOptionVecOptionI32"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->OptionI32->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionI32);
			Conn->Db->OptionString->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionString);
			Conn->Db->OptionIdentity->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionIdentity);
			Conn->Db->OptionSimpleEnum->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionSimpleEnum);
			Conn->Db->OptionEveryPrimitiveStruct->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionPrimitiveStruct);
			Conn->Db->OptionVecOptionI32->OnInsert.AddDynamic(Handler, &UOptionActionsHandler::OnInsertOptionVecOptionI32);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//

					Ctx.Reducers->InsertOptionI32(FTestClientOptionalInt32());
					Ctx.Reducers->InsertOptionString(FTestClientOptionalString());
					Ctx.Reducers->InsertOptionIdentity(FTestClientOptionalIdentity());
					Ctx.Reducers->InsertOptionSimpleEnum(FTestClientOptionalSimpleEnum());
					Ctx.Reducers->InsertOptionEveryPrimitiveStruct(FTestClientOptionalEveryPrimitiveStruct());
					Ctx.Reducers->InsertOptionVecOptionI32(FTestClientOptionalVecOptionalInt32());
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertStructTest::RunTest(const FString &Parameters)
{
	TestName = "InsertStruct";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UStructActionsHandler *Handler = CreateTestHandler<UStructActionsHandler>();
	Handler->Counter->Register(TEXT("InsertOneUnitStruct"));
	Handler->Counter->Register(TEXT("InsertOneByteStruct"));
	Handler->Counter->Register(TEXT("InsertOneEveryPrimitiveStruct"));
	Handler->Counter->Register(TEXT("InsertOneEveryVecStruct"));

	Handler->Counter->Register(TEXT("InsertVecUnitStruct"));
	Handler->Counter->Register(TEXT("InsertVecByteStruct"));
	Handler->Counter->Register(TEXT("InsertVecEveryPrimitiveStruct"));
	Handler->Counter->Register(TEXT("InsertVecEveryVecStruct"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->OneUnitStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertOneUnitStruct);
			Conn->Db->OneByteStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertOneByteStruct);
			Conn->Db->OneEveryPrimitiveStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertOneEveryPrimitiveStruct);
			Conn->Db->OneEveryVecStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertOneEveryVecStruct);

			Conn->Db->VecUnitStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertVecUnitStruct);
			Conn->Db->VecByteStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertVecByteStruct);
			Conn->Db->VecEveryPrimitiveStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertVecEveryPrimitiveStruct);
			Conn->Db->VecEveryVecStruct->OnInsert.AddDynamic(Handler, &UStructActionsHandler::OnInsertVecEveryVecStruct);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//

					FByteStructType OneByteStruct;
					OneByteStruct.B = 0;
					Handler->ExpectedByteStruct = OneByteStruct;

					TArray<FByteStructType> VecByteStruct;
					VecByteStruct.Add(OneByteStruct);
					Handler->ExpectedVecByteStruct = VecByteStruct;

					FSpacetimeDBUInt128 UInt128 = { 0, 4 };
					FSpacetimeDBUInt256 UInt256 = { FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, 5) };

					FSpacetimeDBInt128 Int128 = { 0, static_cast<uint64>(-5) };
					FSpacetimeDBInt256 Int256 = { FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, static_cast<uint64>(-5)) };

					FSpacetimeDBUInt128 UInt128p = { 0x0102030405060708, 0x090a0b0c0d0e0f10 };
					FSpacetimeDBUInt256 UInt256p = { FSpacetimeDBUInt128(0x0102030405060708, 0x090a0b0c0d0e0f10), FSpacetimeDBUInt128(0x1112131415161718, 0x191a1b1c1d1e1f20)};

					FSpacetimeDBInt128 Int128p = { static_cast<uint64>(-0x0102030405060708), static_cast<uint64>(-0x090a0b0c0d0e0f10)};
					FSpacetimeDBInt256 Int256p = { FSpacetimeDBUInt128(static_cast<uint64>(-0x0102030405060708), static_cast<uint64>(-0x090a0b0c0d0e0f10)), 
												   FSpacetimeDBUInt128(static_cast<uint64>(-0x1112131415161718), static_cast<uint64>(-0x191a1b1c1d1e1f20)) };
					
					TArray<FEveryPrimitiveStructType> PrimitiveArray;
					FEveryPrimitiveStructType EveryPrimitiveStructType;
					EveryPrimitiveStructType.A = { 0x01 };
					EveryPrimitiveStructType.B = { 0x0102 };
					EveryPrimitiveStructType.C = { 0x01020304 };
					EveryPrimitiveStructType.D = { 0x0102030405060708 };
					EveryPrimitiveStructType.E = { UInt128p };
					EveryPrimitiveStructType.F = { UInt256p };
					EveryPrimitiveStructType.G = { -0x01 };
					EveryPrimitiveStructType.H = { -0x0102 };
					EveryPrimitiveStructType.I = { -0x01020304 };
					EveryPrimitiveStructType.J = { -0x0102030405060708 };
					EveryPrimitiveStructType.K = { Int128p };
					EveryPrimitiveStructType.L = { Int256p };
					EveryPrimitiveStructType.M = { false };
					EveryPrimitiveStructType.N = { 1.0 };
					EveryPrimitiveStructType.O = { -1.0 };
					EveryPrimitiveStructType.P = { "string" };
					EveryPrimitiveStructType.Q = { FSpacetimeDBIdentity() };
					EveryPrimitiveStructType.R = { FSpacetimeDBConnectionId() };
					EveryPrimitiveStructType.S = { FSpacetimeDBTimestamp(9876543210) };
					EveryPrimitiveStructType.T = { FSpacetimeDBTimeDuration(-67419000000003LL) };
					PrimitiveArray.Add(EveryPrimitiveStructType);
					Handler->ExpectedEveryPrimitiveStruct = EveryPrimitiveStructType;
					Handler->ExpectedVecPrimitiveStruct = PrimitiveArray;

					TArray<FEveryVecStructType> VecArray;
					FEveryVecStructType VecEveryVecStructType;
					VecEveryVecStructType.A = { };
					VecEveryVecStructType.B = { 1 };
					VecEveryVecStructType.C = { 2, 2 };
					VecEveryVecStructType.D = { 3, 3, 3 };
					VecEveryVecStructType.E = { UInt128, UInt128, UInt128, UInt128 };
					VecEveryVecStructType.F = { UInt256, UInt256, UInt256, UInt256, UInt256 };
					VecEveryVecStructType.G = { -1 };
					VecEveryVecStructType.H = { -2, -2 };
					VecEveryVecStructType.I = { -3, -3, -3 };
					VecEveryVecStructType.J = { -4, -4, -4, -4 };
					VecEveryVecStructType.K = { Int128, Int128, Int128, Int128, Int128 };
					VecEveryVecStructType.L = { Int256, Int256, Int256, Int256, Int256, Int256 };
					VecEveryVecStructType.M = { false, true, true, false };
					VecEveryVecStructType.N = { 0.0, -1.0, 1.0, -2.0, 2.0 };
					VecEveryVecStructType.O = { 0.0, -0.5, 0.5, -1.5, 1.5 };
					VecEveryVecStructType.P = { "vec", "of", "strings" };
					VecEveryVecStructType.Q = { FSpacetimeDBIdentity() };
					VecEveryVecStructType.R = { FSpacetimeDBConnectionId() };
					VecEveryVecStructType.S = { FSpacetimeDBTimestamp(9876543210) };
					VecEveryVecStructType.T = { FSpacetimeDBTimeDuration(-67419000000003LL) };
					VecArray.Add(VecEveryVecStructType);
					Handler->ExpectedEveryVecStruct = VecEveryVecStructType;
					Handler->ExpectedVecEveryVecStruct = VecArray;

					Ctx.Reducers->InsertOneUnitStruct(FUnitStructType());
					Ctx.Reducers->InsertOneByteStruct(OneByteStruct);
					Ctx.Reducers->InsertOneEveryPrimitiveStruct(EveryPrimitiveStructType);
					Ctx.Reducers->InsertOneEveryVecStruct(VecEveryVecStructType);

					Ctx.Reducers->InsertVecUnitStruct(TArray<FUnitStructType>());
					Ctx.Reducers->InsertVecByteStruct(VecByteStruct);
					Ctx.Reducers->InsertVecEveryPrimitiveStruct(PrimitiveArray);
					Ctx.Reducers->InsertVecEveryVecStruct(VecArray);
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertSimpleEnumTest::RunTest(const FString &Parameters)
{
	TestName = "InsertSimpleEnum";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UEnumActionsHandler *Handler = CreateTestHandler<UEnumActionsHandler>();
	Handler->Counter->Register(TEXT("InsertOneSimpleEnum"));
	Handler->Counter->Register(TEXT("InsertVecSimpleEnum"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->OneSimpleEnum->OnInsert.AddDynamic(Handler, &UEnumActionsHandler::OnInsertOneSimpleEnum);
			Conn->Db->VecSimpleEnum->OnInsert.AddDynamic(Handler, &UEnumActionsHandler::OnInsertVecSimpleEnum);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//
					FOneSimpleEnumType OneSimpleEnum;
					OneSimpleEnum.E = ESimpleEnumType::One;
					Handler->ExpectedSimpleEnum = OneSimpleEnum;

					FVecSimpleEnumType VecSimpleEnum;
					VecSimpleEnum.E = { ESimpleEnumType::Zero, ESimpleEnumType::One, ESimpleEnumType::Two };
					Handler->ExpectedVecEnum = VecSimpleEnum;

					Ctx.Reducers->InsertOneSimpleEnum(OneSimpleEnum.E);
					Ctx.Reducers->InsertVecSimpleEnum(VecSimpleEnum.E);
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertEnumWithPayloadTest::RunTest(const FString &Parameters)
{
	TestName = "InsertEnumWithPayload";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UEnumActionsHandler *Handler = CreateTestHandler<UEnumActionsHandler>();
	Handler->Counter->Register(TEXT("InsertOneEnumWithPayload"));
	Handler->Counter->Register(TEXT("InsertVecEnumWithPayload"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->OneEnumWithPayload->OnInsert.AddDynamic(Handler, &UEnumActionsHandler::OnInsertOneEnumWithPayload);
			Conn->Db->VecEnumWithPayload->OnInsert.AddDynamic(Handler, &UEnumActionsHandler::OnInsertVecEnumWithPayload);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//
					FSpacetimeDBIdentity Identity;
					Ctx.TryGetIdentity(Identity);

					FVecEnumWithPayloadType VecEnumWithPayload;
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U8(0));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U16(1));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U32(2));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U64(3));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U128(FSpacetimeDBUInt128(0, 4)));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::U256(FSpacetimeDBUInt256(FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, 5))));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I8(0));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I16(-1));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I32(-2));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I64(-3));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I128(FSpacetimeDBInt128(0, static_cast<uint64>(-4))));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::I256(FSpacetimeDBInt256(FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, static_cast<uint64>(-5)))));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::Bool(true));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::F32(0.0));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::F64(100.0));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::Str("enum holds string"));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::Identity(Identity));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::Bytes({ 0xde, 0xad, 0xbe, 0xef }));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::Strings({ "enum", "of", "vec", "of", "strings" }));
					VecEnumWithPayload.E.Add(FEnumWithPayloadType::SimpleEnums({ ESimpleEnumType::Zero, ESimpleEnumType::One, ESimpleEnumType::Two }));

					Handler->ExpectedVecEnumWithPayload = VecEnumWithPayload;

					Ctx.Reducers->InsertOneEnumWithPayload(FEnumWithPayloadType::U8(0));
					Ctx.Reducers->InsertVecEnumWithPayload(VecEnumWithPayload.E);
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertDeleteLargeTableTest::RunTest(const FString &Parameters)
{
	TestName = "InsertDeleteLargeTable";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// --- 1. SETUP ---
	// Create the handler that will contain our test logic and state.
	ULargeTableActionHandler *Handler = CreateTestHandler<ULargeTableActionHandler>();
	Handler->Counter->Register(TEXT("InsertLargeTable"));
	Handler->Counter->Register(TEXT("DeleteLargeTable"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// --- 2. SUBSCRIBE TO EVENTS ---
			// Bind the handler's member functions to the database delegates.
			Conn->Db->LargeTable->OnInsert.AddDynamic(Handler, &ULargeTableActionHandler::OnInsertLargeTable);
			Conn->Db->LargeTable->OnDelete.AddDynamic(Handler, &ULargeTableActionHandler::OnDeleteLargeTable);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					FLargeTableType LargeTable;

					FByteStructType ByteStruct;
					ByteStruct.B = 0;

					FSpacetimeDBUInt128 UInt128 = { 0, 4 };
					FSpacetimeDBUInt256 UInt256 = { FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, 5) };

					FSpacetimeDBInt128 Int128 = { 0, static_cast<uint64>(-5) };
					FSpacetimeDBInt256 Int256 = { FSpacetimeDBUInt128(0, 0), FSpacetimeDBUInt128(0, static_cast<uint64>(-5)) };

					FSpacetimeDBUInt128 UInt128p = { 0x0102030405060708, 0x090a0b0c0d0e0f10 };
					FSpacetimeDBUInt256 UInt256p = { FSpacetimeDBUInt128(0x0102030405060708, 0x090a0b0c0d0e0f10), FSpacetimeDBUInt128(0x1112131415161718, 0x191a1b1c1d1e1f20) };

					FSpacetimeDBInt128 Int128p = { static_cast<uint64>(-0x0102030405060708), static_cast<uint64>(-0x090a0b0c0d0e0f10) };
					FSpacetimeDBInt256 Int256p = { FSpacetimeDBUInt128(static_cast<uint64>(-0x0102030405060708), static_cast<uint64>(-0x090a0b0c0d0e0f10)),
												   FSpacetimeDBUInt128(static_cast<uint64>(-0x1112131415161718), static_cast<uint64>(-0x191a1b1c1d1e1f20)) };

					FEveryPrimitiveStructType EveryPrimitiveStructType;
					EveryPrimitiveStructType.A = { 0x01 };
					EveryPrimitiveStructType.B = { 0x0102 };
					EveryPrimitiveStructType.C = { 0x01020304 };
					EveryPrimitiveStructType.D = { 0x0102030405060708 };
					EveryPrimitiveStructType.E = { UInt128p };
					EveryPrimitiveStructType.F = { UInt256p };
					EveryPrimitiveStructType.G = { -0x01 };
					EveryPrimitiveStructType.H = { -0x0102 };
					EveryPrimitiveStructType.I = { -0x01020304 };
					EveryPrimitiveStructType.J = { -0x0102030405060708 };
					EveryPrimitiveStructType.K = { Int128p };
					EveryPrimitiveStructType.L = { Int256p };
					EveryPrimitiveStructType.M = { false };
					EveryPrimitiveStructType.N = { 1.0 };
					EveryPrimitiveStructType.O = { -1.0 };
					EveryPrimitiveStructType.P = { "string" };
					EveryPrimitiveStructType.Q = { FSpacetimeDBIdentity() };
					EveryPrimitiveStructType.R = { FSpacetimeDBConnectionId() };
					EveryPrimitiveStructType.S = { FSpacetimeDBTimestamp(9876543210) };
					EveryPrimitiveStructType.T = { FSpacetimeDBTimeDuration(-67419000000003LL) };

					FEveryVecStructType VecEveryVecStructType;
					VecEveryVecStructType.A = { };
					VecEveryVecStructType.B = { 1 };
					VecEveryVecStructType.C = { 2, 2 };
					VecEveryVecStructType.D = { 3, 3, 3 };
					VecEveryVecStructType.E = { UInt128, UInt128, UInt128, UInt128 };
					VecEveryVecStructType.F = { UInt256, UInt256, UInt256, UInt256, UInt256 };
					VecEveryVecStructType.G = { -1 };
					VecEveryVecStructType.H = { -2, -2 };
					VecEveryVecStructType.I = { -3, -3, -3 };
					VecEveryVecStructType.J = { -4, -4, -4, -4 };
					VecEveryVecStructType.K = { Int128, Int128, Int128, Int128, Int128 };
					VecEveryVecStructType.L = { Int256, Int256, Int256, Int256, Int256, Int256 };
					VecEveryVecStructType.M = { false, true, true, false };
					VecEveryVecStructType.N = { 0.0, -1.0, 1.0, -2.0, 2.0 };
					VecEveryVecStructType.O = { 0.0, -0.5, 0.5, -1.5, 1.5 };
					VecEveryVecStructType.P = { "vec", "of", "strings" };
					VecEveryVecStructType.Q = { FSpacetimeDBIdentity() };
					VecEveryVecStructType.R = { FSpacetimeDBConnectionId() };
					VecEveryVecStructType.S = { FSpacetimeDBTimestamp(9876543210) };
					VecEveryVecStructType.T = { FSpacetimeDBTimeDuration(-67419000000003LL) };

					LargeTable.A = 0;
					LargeTable.B = 1;
					LargeTable.C = 2;
					LargeTable.D = 3;
					LargeTable.E = UInt128;
					LargeTable.F = UInt256;
					LargeTable.G = 0;
					LargeTable.H = -1;
					LargeTable.I = -2;
					LargeTable.J = -3;
					LargeTable.K = Int128;
					LargeTable.L = Int256;
					LargeTable.M = false;
					LargeTable.N = 0.0;
					LargeTable.O = 1.0;
					LargeTable.P = "string";
					LargeTable.Q = ESimpleEnumType::Zero;
					LargeTable.R = FEnumWithPayloadType::Bool(false);
					LargeTable.S = FUnitStructType();
					LargeTable.T = ByteStruct;
					LargeTable.U = EveryPrimitiveStructType;
					LargeTable.V = VecEveryVecStructType;

					Handler->ExpectedLargeTable = LargeTable;

					// Call the insert reducer, which will trigger the 'OnInsertLargeTable' callback.
					Ctx.Reducers->InsertLargeTable(
						LargeTable.A,
						LargeTable.B,
						LargeTable.C,
						LargeTable.D,
						LargeTable.E,
						LargeTable.F,
						LargeTable.G,
						LargeTable.H,
						LargeTable.I,
						LargeTable.J,
						LargeTable.K,
						LargeTable.L,
						LargeTable.M,
						LargeTable.N,
						LargeTable.O,
						LargeTable.P,
						LargeTable.Q,
						LargeTable.R,
						LargeTable.S,
						LargeTable.T,
						LargeTable.U,
						LargeTable.V
						);
				}); });

	// --- 4. WAIT FOR COMPLETION ---
	// The latent command waits until both 'InsertLargeTable' and 'DeleteLargeTable'
	// have been marked as done in the handler.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FInsertPrimitivesAsStringTest::RunTest(const FString &Parameters)
{
	TestName = "InsertPrimitivesAsString";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UInsertPrimitiveHandler *Handler = CreateTestHandler<UInsertPrimitiveHandler>();
	Handler->Counter->Register(TEXT("InsertPrimitivesAsString"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Subscribe to the insert event for each table.
			// These handlers will contain the validation logic (the "unwrap" equivalent).
			Conn->Db->VecString->OnInsert.AddDynamic(Handler, &UInsertPrimitiveHandler::OnInsertPrimitivesAsString);

			SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}

					// --- Replicating the 'insert_one' calls --//
														   // Populate every field mirroring Rust's every_primitive_struct().
					FEveryPrimitiveStructType PrimitiveStructType;
					PrimitiveStructType.A = 0x01;
					PrimitiveStructType.B = 0x0102;
					PrimitiveStructType.C = 0x01020304;
					PrimitiveStructType.D = 0x0102030405060708ULL;
					PrimitiveStructType.E = FSpacetimeDBUInt128(0x0102030405060708ULL, 0x090A0B0C0D0E0F10ULL);
					PrimitiveStructType.F = FSpacetimeDBUInt256(
						FSpacetimeDBUInt128(0x0102030405060708ULL, 0x090A0B0C0D0E0F10ULL),
						FSpacetimeDBUInt128(0x1112131415161718ULL, 0x191A1B1C1D1E1F20ULL));
					PrimitiveStructType.G = -0x01;
					PrimitiveStructType.H = -0x0102;
					PrimitiveStructType.I = -0x01020304;
					PrimitiveStructType.J = -0x0102030405060708LL;
					PrimitiveStructType.K = FSpacetimeDBInt128(0xFEFDFCFBFAF9F8F7ULL, 0xF6F5F4F3F2F1F0F0ULL);
					PrimitiveStructType.L = FSpacetimeDBInt256(
						FSpacetimeDBUInt128(0xFEFDFCFBFAF9F8F7ULL, 0xF6F5F4F3F2F1F0EFULL),
						FSpacetimeDBUInt128(0xEEEDECEBEAE9E8E7ULL, 0xE6E5E4E3E2E1E0E0ULL));
					PrimitiveStructType.M = false;
					PrimitiveStructType.N = 1.0f;
					PrimitiveStructType.O = -1.0;
					PrimitiveStructType.P = TEXT("string");
					PrimitiveStructType.Q = FSpacetimeDBIdentity();
					PrimitiveStructType.R = FSpacetimeDBConnectionId();
					PrimitiveStructType.S = FSpacetimeDBTimestamp(9876543210LL);
					PrimitiveStructType.T = FSpacetimeDBTimeDuration(-67419000000003LL);

					TArray<FString> ExpectedStrings;
					ExpectedStrings.Reserve(20);

					ExpectedStrings.Add(LexToString(PrimitiveStructType.A));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.B));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.C));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.D));

					ExpectedStrings.Add(PrimitiveStructType.E.ToDecimalString()); // UInt128
					ExpectedStrings.Add(PrimitiveStructType.F.ToDecimalString()); // UInt256

					ExpectedStrings.Add(LexToString(PrimitiveStructType.G));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.H));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.I));
					ExpectedStrings.Add(LexToString(PrimitiveStructType.J));

					ExpectedStrings.Add(PrimitiveStructType.K.ToDecimalString()); // Int128 
					ExpectedStrings.Add(PrimitiveStructType.L.ToDecimalString()); // Int256

					ExpectedStrings.Add(LexToString(PrimitiveStructType.M));

					// Floats
					ExpectedStrings.Add(TrimFloat(PrimitiveStructType.N));
					ExpectedStrings.Add(TrimFloat(PrimitiveStructType.O));

					ExpectedStrings.Add(PrimitiveStructType.P);

					// Identity/ConnectionId
					ExpectedStrings.Add(PrimitiveStructType.Q.ToHex().Replace(TEXT("0x"), TEXT("")));
					ExpectedStrings.Add(PrimitiveStructType.R.ToHex().Replace(TEXT("0x"), TEXT("")));

					ExpectedStrings.Add(NormalizeTimestamp(PrimitiveStructType.S));
					ExpectedStrings.Add(NormalizeDuration(PrimitiveStructType.T));

					// Push the normalized expectation into the same handler instance
					Handler->ExpectedStrings = MoveTemp(ExpectedStrings);

					//Call Reducer
					Ctx.Reducers->InsertPrimitivesAsStrings(PrimitiveStructType);
				}); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

static FString GetReauthTokenPath()
{
    const FString Dir = FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Tests"));
    IFileManager::Get().MakeDirectory(*Dir, /*Tree*/true);
    return FPaths::Combine(Dir, TEXT("reauth_token.txt"));
}

bool FReauth1Test::RunTest(const FString &Parameters)
{
	TestName = "Reauth";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UTestHandler *Handler = CreateTestHandler<UTestHandler>();
	Handler->Counter->Register(TEXT("ReauthPart1"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			UCredentials::Init(TestName);
			const FString Token = UCredentials::LoadToken();
			UE_LOG(LogTemp, Display, TEXT("[Reauth1] Loaded token: '%s'"), *Token);
			if (!Token.IsEmpty())
			{
				const FString TokenFilePath = GetReauthTokenPath();
				const bool bOK = FFileHelper::SaveStringToFile(Token, *TokenFilePath);
                UE_LOG(LogTemp, Display, TEXT("[Reauth1] Save token -> %s (ok=%d)"), *TokenFilePath, bOK);

				UCredentials::SaveToken(Token);
				Handler->Counter->MarkSuccess("ReauthPart1");
			}
			else
			{
				Handler->Counter->MarkFailure("ReauthPart1", TEXT("Token was not saved"));
			} });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FReauth2Test::RunTest(const FString &Parameters)
{
	TestName = "Reauth";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UTestHandler *Handler = CreateTestHandler<UTestHandler>();
	Handler->Counter->Register(TEXT("ReauthPart2"));

	UCredentials::Init(TestName);
	const FString TokenFilePath = GetReauthTokenPath();
	//const FString OldToken = UCredentials::LoadToken();
	FString OldToken;
    const bool bRead = FFileHelper::LoadFileToString(OldToken, *TokenFilePath);

    UE_LOG(LogTemp, Display, TEXT("[Reauth2] Read token (ok=%d) from %s: '%s'"),
           bRead, *TokenFilePath, *OldToken);
    if (!bRead || OldToken.IsEmpty())
    {
        Handler->Counter->MarkFailure("ReauthPart2", TEXT("Missing/empty token file"));
        ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
        return true;
    }

	UDbConnection *Connection = ConnectWithThen(
		Handler->Counter,
		TestName,
		[OldToken](UDbConnectionBuilder *Builder)
		{
			return Builder->WithToken(OldToken);
		},
		[this, Handler, OldToken](UDbConnection *Conn)
		{
			const FString CurrentToken = UCredentials::LoadToken();
			            UE_LOG(LogTemp, Display, TEXT("[Reauth2] CurrentToken='%s' OldToken='%s'"),
                   *CurrentToken, *OldToken);
			if (CurrentToken == OldToken)
			{
				Handler->Counter->MarkSuccess("ReauthPart2");
			}
			else
			{
				Handler->Counter->MarkFailure("ReauthPart2", FString(TEXT("Unexpected Token: ")) + CurrentToken);
			}
		});

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FShouldFailTest::RunTest(const FString &Parameters)
{
	TestName = "ShouldFail";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UTestHandler *Handler = CreateTestHandler<UTestHandler>();
	Handler->Counter->Register(TEXT("ShouldFail"));
	Handler->Counter->MarkFailure("ShouldFail", "This is an intentional failure.");

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FCallerAlwaysNotifiedTest::RunTest(const FString &Parameters)
{
	TestName = "CallerAlwaysNotified";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create and register a test counter to track completion.
	UTestHandler *Handler = CreateTestHandler<UTestHandler>();
	Handler->Counter->Register(TEXT("NoOpSucceeds"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{ SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
															   {
					// Perform initial check to ensure all tables are empty before the test.
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->Abort();
						return;
					}
	
					Ctx.Reducers->OnNoOpSucceeds.AddDynamic(Handler, &UTestHandler::OnNoOpSucceeds);
					Ctx.Reducers->NoOpSucceeds(); }); });

	// Wait for the test counter to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FSubscribeAllSelectStarTest::RunTest(const FString &Parameters)
{
	TestName = "SubscribeAllSelectStar";

	if (!ValidateParameterConfig(this))
		return false;
	UInsertPrimitiveHandler *Handler = CreateTestHandler<UInsertPrimitiveHandler>();

	Handler->Counter->Register(TEXT("on_subscription_applied_nothing"));
#define REG(Suffix, Expected, RowStructType) Handler->Counter->Register(TEXT("InsertOne" #Suffix));
	FOREACH_PRIMITIVE(REG)
#undef REG

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
#define BIND_HANDLER(Suffix, Expected, RowStructType) Conn->Db->One##Suffix->OnInsert.AddDynamic(Handler, &UInsertPrimitiveHandler::OnInsertOne##Suffix);
				FOREACH_PRIMITIVE(BIND_HANDLER)
#undef BIND_HANDLER

					SubscribeAllThen(Conn, [this, Handler, Conn](FSubscriptionEventContext Ctx)
						{
							if (!AssertAllTablesEmpty(this, Conn->Db))
							{
								Handler->Counter->MarkFailure(TEXT("on_subscription_applied_nothing"), TEXT("Tables not empty"));
								return;
							}

							Handler->Counter->MarkSuccess(TEXT("on_subscription_applied_nothing"));

#define CALL_INSERT(Suffix, Expected, RowStructType) Ctx.Reducers->InsertOne##Suffix(Expected);
							FOREACH_PRIMITIVE(CALL_INSERT)
#undef CALL_INSERT
						}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FRowDeduplicationTest::RunTest(const FString &Parameters)
{
	TestName = "RowDeduplication";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	URowDeduplicationHandler *Handler = CreateTestHandler<URowDeduplicationHandler>();
	Handler->Counter->Register(TEXT("on_subscription_applied_nothing"));
	Handler->Counter->Register(TEXT("ins_24"));
	Handler->Counter->Register(TEXT("ins_42"));
	Handler->Counter->Register(TEXT("del_24"));
	Handler->Counter->Register(TEXT("upd_42"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->PkU32->OnInsert.AddDynamic(Handler, &URowDeduplicationHandler::OnInsertPkU32);
			Conn->Db->PkU32->OnDelete.AddDynamic(Handler, &URowDeduplicationHandler::OnDeletePkU32);
			Conn->Db->PkU32->OnUpdate.AddDynamic(Handler, &URowDeduplicationHandler::OnUpdatePkU32);

			TArray<FString> Queries = {
					TEXT("SELECT * FROM pk_u32 WHERE pk_u32.n < 100;"),
					TEXT("SELECT * FROM pk_u32 WHERE pk_u32.n < 200;")
			};

			SubscribeTheseThen(Conn, Queries, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->MarkFailure(TEXT("on_subscription_applied_nothing"), TEXT("tables not empty"));
						Handler->Counter->Abort();
						return;
					}
					Handler->Counter->MarkSuccess(TEXT("on_subscription_applied_nothing"));
					Ctx.Reducers->InsertPkU32(24, 0xbeef);
					Ctx.Reducers->InsertPkU32(42, 0xbeef);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FRowDeduplicationJoinRAndSTest::RunTest(const FString &Parameters)
{
	TestName = "RowDeduplicationJoinRAndS";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	URowDeduplicationJoinHandler *Handler = CreateTestHandler<URowDeduplicationJoinHandler>();
	Handler->Counter->Register(TEXT("on_subscription_applied_nothing"));
	Handler->Counter->Register(TEXT("pk_u32_on_insert"));
	Handler->Counter->Register(TEXT("pk_u32_on_update"));
	Handler->Counter->Register(TEXT("unique_u32_on_insert"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			Conn->Db->PkU32->OnInsert.AddDynamic(Handler, &URowDeduplicationJoinHandler::OnInsertPkU32);
			Conn->Db->PkU32->OnUpdate.AddDynamic(Handler, &URowDeduplicationJoinHandler::OnUpdatePkU32);
			Conn->Db->PkU32->OnDelete.AddDynamic(Handler, &URowDeduplicationJoinHandler::OnDeletePkU32);
			Conn->Db->UniqueU32->OnInsert.AddDynamic(Handler, &URowDeduplicationJoinHandler::OnInsertUniqueU32);
			Conn->Db->UniqueU32->OnDelete.AddDynamic(Handler, &URowDeduplicationJoinHandler::OnDeleteUniqueU32);

			TArray<FString> Queries = {
					TEXT("SELECT * FROM pk_u32;"),
					TEXT("SELECT unique_u32.* FROM unique_u32 JOIN pk_u32 ON unique_u32.n = pk_u32.n;")
			};

			SubscribeTheseThen(Conn, Queries, [Handler](FSubscriptionEventContext Ctx)
				{
					Handler->Counter->MarkSuccess(TEXT("on_subscription_applied_nothing"));
					Ctx.Reducers->InsertPkU32(42, 50);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FRowDeduplicationRJoinSandRJoinTTest::RunTest(const FString &Parameters)
{
	TestName = "RowDeduplicationRJoinSAndRJoinT";

	if (!ValidateParameterConfig(this))
		return false;

	TSharedPtr<FTestCounter> Counter = MakeShared<FTestCounter>();

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Counter, FPlatformTime::Seconds()));
	return true;
}

bool FLhsJoinUpdateTest::RunTest(const FString &Parameters)
{
	TestName = "TestLhsJoinUpdate";

	if (!ValidateParameterConfig(this))
		return false;

	ULhsJoinUpdateHandler *Handler = CreateTestHandler<ULhsJoinUpdateHandler>();
	Handler->Counter->Register(TEXT("on_insert_1"));
	Handler->Counter->Register(TEXT("on_insert_2"));
	Handler->Counter->Register(TEXT("on_update_1"));
	Handler->Counter->Register(TEXT("on_update_2"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn)
											{
			Conn->Reducers->OnInsertPkU32.AddDynamic(Handler, &ULhsJoinUpdateHandler::OnInsertPkU32);
			Conn->Reducers->OnUpdatePkU32.AddDynamic(Handler, &ULhsJoinUpdateHandler::OnUpdatePkU32);

			TArray<FString> Queries = {
							TEXT("SELECT p.* FROM pk_u32 p WHERE n = 1"),
							TEXT("SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5")
			};

			SubscribeTheseThen(Conn, Queries, [Handler](FSubscriptionEventContext Ctx)
				{
					Ctx.Reducers->InsertPkU32(1, 0);
					Ctx.Reducers->InsertPkU32(2, 0);
					Ctx.Reducers->InsertUniqueU32(1, 3);
					Ctx.Reducers->InsertUniqueU32(2, 4);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FLhsJoinUpdateDisjointQueriesTest::RunTest(const FString &Parameters)
{
	TestName = "TestLhsJoinUpdateDisjointQueries";

	if (!ValidateParameterConfig(this))
		return false;

	ULhsJoinUpdateDisjointQueriesHandler *Handler = CreateTestHandler<ULhsJoinUpdateDisjointQueriesHandler>();
	Handler->Counter->Register(TEXT("on_insert_1"));
	Handler->Counter->Register(TEXT("on_insert_2"));
	Handler->Counter->Register(TEXT("on_update_1"));
	Handler->Counter->Register(TEXT("on_update_2"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn)
											{
			Conn->Reducers->OnInsertPkU32.AddDynamic(Handler, &ULhsJoinUpdateDisjointQueriesHandler::OnInsertPkU32Reducer);
			Conn->Reducers->OnUpdatePkU32.AddDynamic(Handler, &ULhsJoinUpdateDisjointQueriesHandler::OnUpdatePkU32Reducer);

			TArray<FString> Queries = {
					TEXT("SELECT p.* FROM pk_u32 p WHERE n = 1;"),
					TEXT("SELECT p.* FROM pk_u32 p JOIN unique_u32 u ON p.n = u.n WHERE u.data > 0 AND u.data < 5 AND u.n != 1;")
			};

			SubscribeTheseThen(Conn, Queries, [Handler](FSubscriptionEventContext Ctx)
				{
					Ctx.Reducers->InsertPkU32(1, 0);
					Ctx.Reducers->InsertPkU32(2, 0);
					Ctx.Reducers->InsertUniqueU32(1, 3);
					Ctx.Reducers->InsertUniqueU32(2, 4);
				}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FIntraQueryBagSemanticsForJoinTest::RunTest(const FString &Parameters)
{
	TestName = "TestIntraQueryBagSemanticsForJoin";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create the handler for this test.
	UBagSemanticsTestHandler *Handler = CreateTestHandler<UBagSemanticsTestHandler>();
	Handler->Counter->Register(TEXT("on_subscription_applied_nothing"));
	Handler->Counter->Register(TEXT("pk_u32_on_delete"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [this, Handler](UDbConnection *Conn)
											{
			// Bind the on_delete handler for the PkU32 table.
			Conn->Db->PkU32->OnDelete.AddDynamic(Handler, &UBagSemanticsTestHandler::OnDeletePkU32);

			// Subscribe to both tables and the join query.
			 // Subscribe to both tables and the join query.
			TArray<FString> Queries = {
							TEXT("SELECT * FROM btree_u32"),
							TEXT("SELECT pk_u32.* FROM pk_u32 JOIN btree_u32 ON pk_u32.n = btree_u32.n"),
			};
			SubscribeTheseThen(Conn, Queries, [this, Handler, Conn](FSubscriptionEventContext Ctx)
				{
					// Insert (n: 0, data: 0) into btree_u32.
					// No on_insert for PkU32 should fire because it is empty.
					Ctx.Reducers->InsertIntoBtreeU32(TArray<FBTreeU32Type>({ FBTreeU32Type(0, 0) }));

					// Now insert a row into pk_u32 and a duplicate into btree_u32.
					// This creates a multiplicity of 2 for the join result.
					// An on_insert for PkU32 should fire now.
					Ctx.Reducers->InsertIntoPkBtreeU32(TArray<FPkU32Type>({ FPkU32Type(0, 0) }), TArray<FBTreeU32Type>({ FBTreeU32Type(0, 1) }));

					// Delete one of the joining rows from btree_u32.
					// The multiplicity of the join result becomes 1, so no on_delete for PkU32 fires.
					Ctx.Reducers->DeleteFromBtreeU32(TArray<FBTreeU32Type>({ FBTreeU32Type(0, 0) }));

					// Delete the last joining row from btree_u32.
					// The multiplicity of the join result becomes 0, so on_delete for PkU32 fires.
					Ctx.Reducers->DeleteFromBtreeU32(TArray<FBTreeU32Type>({ FBTreeU32Type(0, 1) }));

					if (!AssertAllTablesEmpty(this, Conn->Db))
					{
						Handler->Counter->MarkFailure(TEXT("on_subscription_applied_nothing"), TEXT("tables not empty"));
						Handler->Counter->Abort();
						return;
					}
					Handler->Counter->MarkSuccess(TEXT("on_subscription_applied_nothing"));
				}); });

	// Wait for the final `on_delete` event to signal completion.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FPkSimpleEnumTest::RunTest(const FString &Parameters)
{
	TestName = "PkSimpleEnum";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UPkSimpleEnumHandler *Handler = CreateTestHandler<UPkSimpleEnumHandler>();
	Handler->Counter->Register(TEXT("InsertPkSimpleEnum"));
	Handler->Counter->Register(TEXT("UpdatePkPkSimpleEnum"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn)
											{
		Conn->Db->PkSimpleEnum->OnInsert.AddDynamic(Handler, &UPkSimpleEnumHandler::OnInsertPkSimpleEnum);
		Conn->Db->PkSimpleEnum->OnUpdate.AddDynamic(Handler, &UPkSimpleEnumHandler::OnUpdatePkSimpleEnum);
		Conn->Db->PkSimpleEnum->OnDelete.AddDynamic(Handler, &UPkSimpleEnumHandler::OnDeletePkSimpleEnum);

		TArray<FString> Queries;
		Queries.Add(TEXT("SELECT * FROM pk_simple_enum"));
		SubscribeTheseThen(Conn, Queries, [Handler](FSubscriptionEventContext Ctx) 
		{
				Handler->Data1 = 42;
				Handler->Data2 = 24;
				Handler->A = ESimpleEnumType::Two;
				Ctx.Reducers->InsertPkSimpleEnum(Handler->A, Handler->Data1);
		}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FParameterizedSubscriptionTest::RunTest(const FString &Parameters)
{
	TestName = "TestParameterizedSubscription";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create a counter for the subscription phase.
	UTestHandler *SubscriptionCounter = CreateTestHandler<UTestHandler>();
	SubscriptionCounter->Counter->Register(TEXT("client_0"));
	SubscriptionCounter->Counter->Register(TEXT("client_1"));

	// Create the main test counter to track the final insert and update events.
	UTestHandler *MainCounter = CreateTestHandler<UTestHandler>();
	MainCounter->Counter->Register(TEXT("insert_1")); // For client 0
	MainCounter->Counter->Register(TEXT("update_2")); // For client 0
	MainCounter->Counter->Register(TEXT("insert_3")); // For client 1
	MainCounter->Counter->Register(TEXT("update_4")); // For client 1

	// --- Client 0: Alice ---
	// Create a new handler for the first client
	UParameterizedSubscriptionHandler *AliceHandler = CreateTestHandler<UParameterizedSubscriptionHandler>();
	AliceHandler->Counters = MainCounter;
	AliceHandler->ExpectedOldData = 1;
	AliceHandler->ExpectedNewData = 2;

	UDbConnection *Connection = ConnectThen(AliceHandler->Counter, FString::Printf(TEXT("%s_client_0"), *TestName), [this, AliceHandler, SubscriptionCounter](UDbConnection *Conn)
											{
			// Subscribe to the 'pk_identity' table's insert and update events
			Conn->Db->PkIdentity->OnInsert.AddDynamic(AliceHandler, &UParameterizedSubscriptionHandler::OnInsertPkIdentity);
			Conn->Db->PkIdentity->OnUpdate.AddDynamic(AliceHandler, &UParameterizedSubscriptionHandler::OnUpdatePkIdentity);

			// Subscribe with the parameterized query
			FSpacetimeDBIdentity ClientIdentity;
			Conn->TryGetIdentity(ClientIdentity);
			AliceHandler->ExpectedIdentity = ClientIdentity;

			TArray<FString> Queries;
			Queries.Add(TEXT("SELECT * FROM pk_identity WHERE i = :sender"));
			SubscribeTheseThen(Conn, Queries, [this, AliceHandler, Conn, SubscriptionCounter](FSubscriptionEventContext Ctx)
				{
					// Signal that this client has successfully subscribed
					SubscriptionCounter->Counter->MarkSuccess("client_0");

					// Perform the insert and update calls
					Conn->Reducers->InsertPkIdentity(AliceHandler->ExpectedIdentity, AliceHandler->ExpectedOldData);
					Conn->Reducers->UpdatePkIdentity(AliceHandler->ExpectedIdentity, AliceHandler->ExpectedNewData);
				}); });

	// --- Client 1: Bob ---
	// Create a new handler for the second client
	UParameterizedSubscriptionHandler *BobHandler = CreateTestHandler<UParameterizedSubscriptionHandler>();
	BobHandler->Counters = MainCounter;
	BobHandler->ExpectedOldData = 3;
	BobHandler->ExpectedNewData = 4;

	UDbConnection *Connection2 = ConnectThen(BobHandler->Counter, FString::Printf(TEXT("%s_client_1"), *TestName), [this, BobHandler, SubscriptionCounter](UDbConnection *Conn)
											 {
			// Subscribe to the 'pk_identity' table's insert and update events
			Conn->Db->PkIdentity->OnInsert.AddDynamic(BobHandler, &UParameterizedSubscriptionHandler::OnInsertPkIdentity);
			Conn->Db->PkIdentity->OnUpdate.AddDynamic(BobHandler, &UParameterizedSubscriptionHandler::OnUpdatePkIdentity);

			// Subscribe with the parameterized query
			FSpacetimeDBIdentity ClientIdentity;
			Conn->TryGetIdentity(ClientIdentity);
			BobHandler->ExpectedIdentity = ClientIdentity;

			TArray<FString> Queries;
			Queries.Add(TEXT("SELECT * FROM pk_identity WHERE i = :sender"));
			SubscribeTheseThen(Conn, Queries, [this, BobHandler, Conn, SubscriptionCounter](FSubscriptionEventContext Ctx)
				{
					// Signal that this client has successfully subscribed
					SubscriptionCounter->Counter->MarkSuccess("client_1");

					// Perform the insert and update calls
					Conn->Reducers->InsertPkIdentity(BobHandler->ExpectedIdentity, BobHandler->ExpectedOldData);
					Conn->Reducers->UpdatePkIdentity(BobHandler->ExpectedIdentity, BobHandler->ExpectedNewData);
				}); });

	// Wait for all final insert and update events to complete.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, SubscriptionCounter->Counter, FPlatformTime::Seconds()));
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, MainCounter->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FRlsSubscriptionTest::RunTest(const FString &Parameters)
{
	TestName = "TestRlsSubscription";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	// Create a single test counter to track the final insert events.
	URLSSubscriptionHandler *MainHandler = CreateTestHandler<URLSSubscriptionHandler>();
	MainHandler->Counter->Register(TEXT("Alice"));
	MainHandler->Counter->Register(TEXT("Bob"));

	// --- Client 0: Alice ---
	// Create a new handler for the first client
	URLSSubscriptionHandler *AliceHandler = CreateTestHandler<URLSSubscriptionHandler>();
	AliceHandler->MainCounter = MainHandler;

	UDbConnection *Connection = ConnectThen(AliceHandler->Counter, FString::Printf(TEXT("%s_client_0"), *TestName), [this, AliceHandler, MainHandler](UDbConnection *Conn)
											{
			// Subscribe to the 'users' table insert event
			Conn->Db->Users->OnInsert.AddDynamic(AliceHandler, &URLSSubscriptionHandler::OnInsertUser);

			// Subscribe to the 'users' table
			TArray<FString> Queries;
			Queries.Add(TEXT("SELECT * FROM users"));
			SubscribeTheseThen(Conn, Queries, [this, AliceHandler, Conn, MainHandler](FSubscriptionEventContext Ctx)
				{
					// Get the identity for later validation
					FSpacetimeDBIdentity Identity;
					if (Ctx.TryGetIdentity(Identity))
					{
						AliceHandler->ExpectedUserType = FUsersType(Identity, FString("Alice"));

						// We perform the insert directly here without waiting for the other client.
						// The subscription is what matters. Both clients will insert, and both will receive the other's insert.
						Ctx.Reducers->InsertUser(AliceHandler->ExpectedUserType.Name, AliceHandler->ExpectedUserType.Identity);
					}
					else
					{
						MainHandler->Counter->MarkFailure("Alice", "Failed to get identity for Alice");
					}
					
				}); });

	// --- Client 1: Bob ---
	// Create a new handler for the second client
	URLSSubscriptionHandler *BobHandler = CreateTestHandler<URLSSubscriptionHandler>();
	BobHandler->MainCounter = MainHandler;

	UDbConnection *Connection2 = ConnectThen(BobHandler->Counter, FString::Printf(TEXT("%s_client_1"), *TestName), [this, BobHandler, MainHandler](UDbConnection *Conn)
											 {
			// Subscribe to the 'users' table insert event
			Conn->Db->Users->OnInsert.AddDynamic(BobHandler, &URLSSubscriptionHandler::OnInsertUser);

			// Subscribe to the 'users' table
			TArray<FString> Queries;
			Queries.Add(TEXT("SELECT * FROM users"));
			SubscribeTheseThen(Conn, Queries, [this, BobHandler, Conn, MainHandler](FSubscriptionEventContext Ctx)
				{
					FSpacetimeDBIdentity Identity;
					if (Ctx.TryGetIdentity(Identity))
					{
						BobHandler->ExpectedUserType = FUsersType(Identity, FString("Bob"));

						// We perform the insert directly here without waiting for the other client.
						// The subscription is what matters. Both clients will insert, and both will receive the other's insert.
						Ctx.Reducers->InsertUser(BobHandler->ExpectedUserType.Name, BobHandler->ExpectedUserType.Identity);
					}
					else
					{
						MainHandler->Counter->MarkFailure("Bob", "Failed to get identity for Bob");
					}
				}); });

	// Wait for all final insert events to complete.
	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, MainHandler->Counter, FPlatformTime::Seconds()));

	return true;
}

bool FIndexedSimpleEnumTest::RunTest(const FString &Parameters)
{
	TestName = "IndexedSimpleEnum";

	if (!ValidateParameterConfig(this))
	{
		return false;
	}

	UIndexedSimpleEnumHandler *Handler = CreateTestHandler<UIndexedSimpleEnumHandler>();

	Handler->Counter->Register(TEXT("IndexedSimpleEnum"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn)
											{
		Conn->Db->IndexedSimpleEnum->OnInsert.AddDynamic(Handler, &UIndexedSimpleEnumHandler::OnInsertIndexedSimpleEnum);

		TArray<FString> Queries;
		Queries.Add(TEXT("SELECT * FROM indexed_simple_enum"));
		SubscribeTheseThen(Conn, Queries, [Handler](FSubscriptionEventContext Ctx) 
		{
			Handler->A1 = ESimpleEnumType::Two;
			Handler->A2 = ESimpleEnumType::One;

			Ctx.Reducers->InsertIntoIndexedSimpleEnum(Handler->A1);
		}); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}

bool FOverlappingSubscriptionsTest::RunTest(const FString &Parameters)
{
	TestName = "OverlappingSubscriptions";

	if (!ValidateParameterConfig(this))
		return false;
	UOverlappingSubscriptionsHandler *Handler = CreateTestHandler<UOverlappingSubscriptionsHandler>();
	Handler->Counter->Register(TEXT("OverlappingSubscriptions_call_insert_reducer"));
	Handler->Counter->Register(TEXT("OverlappingSubscriptions_insert_reducer_done"));
	Handler->Counter->Register(TEXT("OverlappingSubscriptions_subscribe_with_row_present"));
	Handler->Counter->Register(TEXT("OverlappingSubscriptions_call_update_reducer"));
	Handler->Counter->Register(TEXT("OverlappingSubscriptions_update_row"));

	UDbConnection *Connection = ConnectThen(Handler->Counter, TestName, [Handler](UDbConnection *Conn)
											{
		Handler->Connection = Conn;
		Conn->Reducers->OnInsertPkU8.AddDynamic(Handler, &UOverlappingSubscriptionsHandler::OnInsertPkU8Reducer);
		Conn->Db->PkU8->OnUpdate.AddDynamic(Handler, &UOverlappingSubscriptionsHandler::OnUpdatePkU8);

		Conn->Reducers->InsertPkU8(1, 0);
		Handler->Counter->MarkSuccess(TEXT("OverlappingSubscriptions_call_insert_reducer")); });

	ADD_LATENT_AUTOMATION_COMMAND(FWaitForTestCounter(*this, TestName, Handler->Counter, FPlatformTime::Seconds()));
	return true;
}
