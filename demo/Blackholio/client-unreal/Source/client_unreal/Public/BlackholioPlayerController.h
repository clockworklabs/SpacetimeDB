#pragma once

#include "CoreMinimal.h"
#include "GameFramework/PlayerController.h"
#include "InputActionValue.h"
#include "BlackholioPlayerController.generated.h"

class APlayerPawn;
class UInputAction;
class UInputMappingContext;
class URespawnWidget;
class UUsernameChooserWidget;
class ULeaderboardWidget;

UCLASS()
class CLIENT_UNREAL_API ABlackholioPlayerController : public APlayerController
{
	GENERATED_BODY()

public:
	ABlackholioPlayerController();

	UFUNCTION(BlueprintCallable, Category = "BH|Functions")
	void ShowDeathScreen();
	
protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;
	virtual void OnPossess(APawn* InPawn) override;
	virtual void SetupInputComponent() override;
	FVector2D ComputeDesiredDirection() const;

	UPROPERTY(EditDefaultsOnly, Category="BH|Config")
	TSubclassOf<UUserWidget> UsernameChooserClass;

	UPROPERTY(EditDefaultsOnly, Category="BH|Config")
	TSubclassOf<UUserWidget> RespawnClass;

	UPROPERTY(EditDefaultsOnly, Category="BH|Config")
	TSubclassOf<UUserWidget> LeaderboardClass;
private:
	UFUNCTION()
	void EnsureMappingContext() const;
	
	UPROPERTY()
	TObjectPtr<APlayerPawn> LocalPlayer;

	UPROPERTY()
	float SendUpdatesFrequency = 0.0333f;
	float LastMovementSendTimestamp = 0.f;
	bool bShowedUsernameChooser = false;

	TOptional<FVector2D> LockInputPosition;

	UPROPERTY(EditDefaultsOnly, Category="BH|Input")
	TObjectPtr<UInputMappingContext> PlayerMappingContext = nullptr;

	UPROPERTY(EditDefaultsOnly, Category="BH|Input")
	TObjectPtr<UInputAction> SplitAction = nullptr;

	UPROPERTY(EditDefaultsOnly, Category="BH|Input")
	TObjectPtr<UInputAction> SuicideAction = nullptr;

	UPROPERTY(EditDefaultsOnly, Category="BH|Input")
	TObjectPtr<UInputAction> ToggleInputLockAction = nullptr;

	UPROPERTY()
	TObjectPtr<URespawnWidget> RespawnWidget = nullptr;
	UPROPERTY()
	TObjectPtr<UUsernameChooserWidget> UsernameChooserWidget = nullptr;
	UPROPERTY()
	TObjectPtr<ULeaderboardWidget> LeaderboardWidget = nullptr;

	// Input handlers (Enhanced Input)
	void OnSplitTriggered(const FInputActionValue& Value);
	void OnSuicideTriggered(const FInputActionValue& Value);
	void OnToggleInputLockTriggered(const FInputActionValue& Value);
};
