#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "Tests/TestCounter.h"

#include "TestHandler.generated.h"

UCLASS()
class TESTVIEWCLIENT_API UTestHandler : public UObject
{
	GENERATED_BODY()

public:
	TSharedPtr<FTestCounter> Counter;
};

UCLASS()
class TESTVIEWCLIENT_API UViewBlueprintRuntimeHandler : public UTestHandler
{
	GENERATED_BODY()

public:
	FSpacetimeDBIdentity ExpectedIdentity = FSpacetimeDBIdentity::FromHex(TEXT("0x1111111111111111111111111111111111111111111111111111111111111111"));

	UFUNCTION()
	void OnPlayersAtLevel0Insert(const FEventContext& Context, const FPlayerType& Value);

	UFUNCTION()
	void OnPlayersAtLevel0Update(const FEventContext& Context, const FPlayerType& OldValue, const FPlayerType& NewValue);

	UFUNCTION()
	void OnPlayersAtLevel0Delete(const FEventContext& Context, const FPlayerType& Value);
};
