#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"
#include "Tests/TestCounter.h"

#include "Connection/Callback.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"

#include "CommonTestFunctions.generated.h"

#define TESTLOG_SUCCESS(Test, Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  + %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		Test.AddInfo(LogMessage); \
	} while (false)

#define TESTLOG_FAIL(Test, Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  x %s"), *UserMessage); \
		UE_LOG(LogTemp, Error, TEXT("%s"), *LogMessage); \
		Test.AddError(LogMessage); \
	} while (false)

UCLASS()
class TESTVIEWPKCLIENT_API UTestHelperDelegates : public UObject
{
	GENERATED_BODY()

public:
	TFunction<void(UDbConnection*, FSpacetimeDBIdentity, const FString&)> OnConnect;
	UFUNCTION()
	void HandleConnect(UDbConnection* Conn, FSpacetimeDBIdentity Identity, const FString& Token);

	TFunction<void(UDbConnection*, const FString&)> OnConnectError;
	UFUNCTION()
	void HandleConnectError(UDbConnection* Conn, const FString& Error);

	TFunction<void(UDbConnection*, const FString&)> OnDisconnect;
	UFUNCTION()
	void HandleDisconnect(UDbConnection* Conn, const FString& Error);

	TFunction<void(FSubscriptionEventContext)> OnSubscriptionApplied;
	UFUNCTION()
	void HandleSubscriptionApplied(FSubscriptionEventContext Ctx);

	TFunction<void(FErrorContext)> OnSubscriptionError;
	UFUNCTION()
	void HandleSubscriptionError(FErrorContext Ctx);
};

UDbConnection* ConnectThen(
	TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<void(UDbConnection*)> Callback);

bool GetDbName(FString& DBName, FString& Error);
bool ValidateParameterConfig(FAutomationTestBase* Test);
bool ReportTestResult(FAutomationTestBase& Test, const FString& TestName, TSharedPtr<FTestCounter> Counter, bool bTimedOut);

template<typename T>
T* CreateTestHandler()
{
	T* Handler = NewObject<T>(GetTransientPackage());
	Handler->AddToRoot();
	Handler->Counter = MakeShared<FTestCounter>();
	return Handler;
}
