#pragma once

#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "RespawnWidget.generated.h"

class UButton;

UCLASS()
class CLIENT_UNREAL_API URespawnWidget : public UUserWidget
{
	GENERATED_BODY()
protected:
	UPROPERTY(meta=(BindWidget))
	TObjectPtr<UButton> RespawnButton;

	virtual void NativeConstruct() override;

private:
	UFUNCTION()
	void OnRespawnPressed();
};
