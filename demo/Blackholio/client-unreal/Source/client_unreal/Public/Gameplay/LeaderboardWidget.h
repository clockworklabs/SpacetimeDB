#pragma once

#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "LeaderboardWidget.generated.h"

class UVerticalBox;
class ULeaderboardRowWidget;
class APlayerPawn;

USTRUCT()
struct FLeaderboardEntry
{
	GENERATED_BODY()

	UPROPERTY() FString Username;
	UPROPERTY() int32  Mass = 0;
	UPROPERTY() TWeakObjectPtr<APlayerPawn> Pawn;
};

UCLASS()
class CLIENT_UNREAL_API ULeaderboardWidget : public UUserWidget
{
	GENERATED_BODY()
	
public:
	virtual void NativeConstruct() override;
	virtual void NativeDestruct() override;

protected:
	UPROPERTY(meta=(BindWidget))
	UVerticalBox* Root = nullptr;

	UPROPERTY(EditDefaultsOnly, Category="BH|Leaderboard")
	TSubclassOf<ULeaderboardRowWidget> RowClass;

	UPROPERTY(EditDefaultsOnly, Category="BH|Leaderboard")
	int32 MaxRowCount = 10;
	
	UPROPERTY(EditDefaultsOnly, Category="BH|Leaderboard")
	float UpdatePeriod = 0.25f;

private:
	UPROPERTY(Transient)
	TArray<ULeaderboardRowWidget*> Rows;

	FTimerHandle UpdateTimer;

	void BuildRowPool();
	void CollectPlayers(TArray<FLeaderboardEntry>& Out) const;
	void UpdateLeaderboard();
};
