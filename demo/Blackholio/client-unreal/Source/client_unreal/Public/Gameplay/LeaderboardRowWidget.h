#pragma once

#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "LeaderboardRowWidget.generated.h"

class UTextBlock;

UCLASS()
class CLIENT_UNREAL_API ULeaderboardRowWidget : public UUserWidget
{
	GENERATED_BODY()
public:
	UFUNCTION(BlueprintCallable)
	void SetData(const FString& Username, int32 Mass);

protected:
	UPROPERTY(meta=(BindWidget))
	TObjectPtr<UTextBlock> UsernameText = nullptr;

	UPROPERTY(meta=(BindWidget))
	TObjectPtr<UTextBlock> MassText = nullptr;
};
