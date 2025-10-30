#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "PlayerPawn.generated.h"

class ACircle;
struct FPlayerType;
class UCameraComponent;
class USpringArmComponent;

UCLASS()
class CLIENT_UNREAL_API APlayerPawn : public APawn
{
	GENERATED_BODY()

public:
	APlayerPawn();
	void Initialize(FPlayerType Player);

	UPROPERTY(BlueprintReadWrite, Category="BH|Player")
	int32 PlayerId = 0;
	UPROPERTY(BlueprintReadWrite, Category="BH|Player")
	bool bIsLocalPlayer = false;
	
	UPROPERTY()
	TArray<TWeakObjectPtr<ACircle>> OwnedCircles;

	UFUNCTION()
	FString GetUsername() const;
	UFUNCTION()
	void OnCircleSpawned(ACircle* Circle);
	UFUNCTION()
	void OnCircleDeleted(ACircle* Circle);

	UFUNCTION(BlueprintCallable, Category="BH|Input")
	void Split();
	UFUNCTION(BlueprintCallable, Category="BH|Input")
	void Suicide();
	
	int32 TotalMass() const;
	UFUNCTION(BlueprintPure, Category="BH|Player")
	FVector CenterOfMass() const;

protected:
	virtual void Destroyed() override;
	UPROPERTY(EditDefaultsOnly, Category="BH|Camera")
	float BaseSize = 50.f;
	UPROPERTY(EditDefaultsOnly, Category="BH|Camera")
	float MassToSizeDivisor = 5.f;
	UPROPERTY(EditDefaultsOnly, Category="BH|Camera")
	float MaxMassBonus = 50.f;
	UPROPERTY(EditDefaultsOnly, Category="BH|Camera")
	float SplitBonus = 30.f;
	
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly)
	TObjectPtr<USpringArmComponent> SpringArm;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly)
	TObjectPtr<UCameraComponent> Camera;
public:
	virtual void Tick(float DeltaTime) override;
};
