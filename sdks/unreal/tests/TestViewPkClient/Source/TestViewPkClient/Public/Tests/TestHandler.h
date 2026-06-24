#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "Tests/TestCounter.h"

#include "TestHandler.generated.h"

UCLASS()
class TESTVIEWPKCLIENT_API UTestHandler : public UObject
{
	GENERATED_BODY()

public:
	TSharedPtr<FTestCounter> Counter;
};

UCLASS()
class TESTVIEWPKCLIENT_API UViewPkRuntimeHandler : public UTestHandler
{
	GENERATED_BODY()

public:
	uint64 ExpectedId = 1;
	FString InitialName = TEXT("before");
	FString UpdatedName = TEXT("after");

	UFUNCTION()
	void OnAllViewPkPlayersInsert(const FEventContext& Context, const FViewPkPlayerType& Value);

	UFUNCTION()
	void OnAllViewPkPlayersUpdate(const FEventContext& Context, const FViewPkPlayerType& OldValue, const FViewPkPlayerType& NewValue);

	UFUNCTION()
	void OnAllViewPkPlayersDelete(const FEventContext& Context, const FViewPkPlayerType& Value);

	UFUNCTION()
	void OnSenderViewPkPlayersAInsert(const FEventContext& Context, const FViewPkPlayerType& Value);

	UFUNCTION()
	void OnSenderViewPkPlayersAUpdate(const FEventContext& Context, const FViewPkPlayerType& OldValue, const FViewPkPlayerType& NewValue);

	UFUNCTION()
	void OnSenderViewPkPlayersADelete(const FEventContext& Context, const FViewPkPlayerType& Value);
};
