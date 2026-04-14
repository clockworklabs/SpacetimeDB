#pragma once

#include "CoreMinimal.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "Tables/RemoteTable.h"
#include "Tests/TestCounter.h"
#include "Tests/TestHandler.h"
#include "Misc/AutomationTest.h"

#include "Connection/Callback.h"

#include "Engine/DeveloperSettings.h"

#include "CommonTestFunctions.generated.h"


/**
 * Logs a success message to the output log and the automation test results window.
 * @param Format The format string for the message.
 * @param ... The arguments for the format string.
 */
#define TESTLOG_SUCCESS(Test, Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ✓ %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		Test.AddInfo(LogMessage); \
	} while (false)

 /**
  * Logs a failure message to the output log and the automation test results window, and marks the test as failed.
  * @param Format The format string for the message.
  * @param ... The arguments for the format string.
  */
#define TESTLOG_FAIL(Test, Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ✗ %s"), *UserMessage); \
		UE_LOG(LogTemp, Error, TEXT("%s"), *LogMessage); \
		Test.AddError(LogMessage); \
	} while (false)

  /**
   * Logs an informational message to the output log and the automation test results window.
   * @param Format The format string for the message.
   * @param ... The arguments for the format string.
   */
#define TESTLOG_INFO(Test, Format, ...) \
	do \
	{ \
		const FString UserMessage = FString::Printf(Format, ##__VA_ARGS__); \
		const FString LogMessage = FString::Printf(TEXT("  ℹ %s"), *UserMessage); \
		UE_LOG(LogTemp, Log, TEXT("%s"), *LogMessage); \
		Test.AddInfo(LogMessage); \
	} while (false)

/**
 * Custom settings for Spacetime DB tests.
 */
UCLASS(config = EditorPerProjectUserSettings, defaultconfig, meta = (DisplayName = "Spacetime DB"))
class TESTPROCCLIENT_API USpacetimeDBSettings : public UDeveloperSettings
{
	GENERATED_BODY()

public:
	/** Default DB name for Spacetime tests if no CLI arg or env var is set */
	UPROPERTY(EditAnywhere, config, Category = "Spacetime DB")
	FString SpacetimeDbTestName;
};

// Utility UObject that forwards dynamic delegate calls to C++ lambdas. Dynamic
// delegates require a UObject instance to bind to, so tests install lambdas into
// this wrapper and bind its UFUNCTION thunks to the SDK's delegates.
UCLASS()
class UTestHelperDelegates : public UObject
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

	TFunction<void(FSubscriptionEventContextBase)> OnSubscriptionEnd;
	UFUNCTION()
	void HandleSubscriptionEnd(FSubscriptionEventContextBase Ctx);

	TFunction<void(FErrorContext)> OnSubscriptionError;
	UFUNCTION()
	void HandleSubscriptionError(FErrorContext Ctx);
};

// Connect to the test database and invoke a callback once connected.
// Registers an on_connect test with the provided suffix on the counter.
UDbConnection* ConnectWithThen(TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<UDbConnectionBuilder* (UDbConnectionBuilder*)> WithBuilder,
	TFunction<void(UDbConnection*)> Callback);

// Convenience: connect with default builder.
UDbConnection* ConnectThen(TSharedPtr<FTestCounter> Counter,
	const FString& TestName,
	TFunction<void(UDbConnection*)> Callback);

// Convenience: connect and perform no additional work.
UDbConnection* Connect(TSharedPtr<FTestCounter> Counter);

// Subscribe to all tables and invoke callback when applied.
void SubscribeAllThen(UDbConnection* Conn,
	TFunction<void(FSubscriptionEventContext)> Callback);

// Subscribe to specific queries and invoke callback when applied.
void SubscribeTheseThen(UDbConnection* Conn,
	const TArray<FString>& Queries,
	TFunction<void(FSubscriptionEventContext)> Callback);

// Assert that a specific table is empty.
bool AssertTableEmpty(FAutomationTestBase* Test,
	URemoteTables* Db,
	const FString& TableName);

// Assert that all tables are empty.
bool AssertAllTablesEmpty(FAutomationTestBase* Test,
	URemoteTables* Db);

// Get the Database name from environment variables.
bool GetDbName(FString& DBName, FString& Error);

//Validate that the test parameters are configured correctly.
bool ValidateParameterConfig(FAutomationTestBase* Test);

// Report a test result to the automation framework.
bool ReportTestResult(FAutomationTestBase& Test, const FString& TestName, TSharedPtr<FTestCounter> Counter, bool bTimedOut);

// Factory method for creating test handlers.
template<typename T>
T* CreateTestHandler()
{
	T* Handler = NewObject<T>(GetTransientPackage());
	Handler->AddToRoot();
	Handler->Counter = MakeShared<FTestCounter>();
	return Handler;
}