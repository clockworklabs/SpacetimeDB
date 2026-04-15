#pragma once

#include "CoreMinimal.h"
#include "Entity.h"
#include "Circle.generated.h"

struct FCircleType;
class APlayerPawn;

UCLASS()
class CLIENT_UNREAL_API ACircle : public AEntity
{
	GENERATED_BODY()

public:
	ACircle();
	
	UPROPERTY(BlueprintReadOnly, Category="BH|Circle")
	int32 OwnerPlayerId = 0;
	UPROPERTY(BlueprintReadOnly, Category="BH|Circle")
	FString Username;

	void Spawn(const FCircleType& Circle, APlayerPawn* InOwner);
	virtual void OnDelete(const FEventContext& Context) override;
	
	UFUNCTION(BlueprintCallable, Category="BH|Circle")
	void SetUsername(const FString& InUsername);
	
	DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnUsernameChanged, const FString&, NewUsername);
	UPROPERTY(BlueprintAssignable, Category="BH|Circle")
	FOnUsernameChanged OnUsernameChanged;

protected:
	UPROPERTY(EditDefaultsOnly, Category="BH|Circle")
	TArray<FLinearColor> ColorPalette;

private:
	TWeakObjectPtr<APlayerPawn> Owner;
};
