#pragma once

#include "CoreMinimal.h"
#include "Blueprint/UserWidget.h"
#include "UsernameChooserWidget.generated.h"

class UEditableTextBox;
class UButton;

UCLASS()
class CLIENT_UNREAL_API UUsernameChooserWidget : public UUserWidget
{
	GENERATED_BODY()

public:
	UFUNCTION()
	void Hide();
protected:
	UPROPERTY(meta=(BindWidget))
	TObjectPtr<UEditableTextBox> UsernameInputField;
	UPROPERTY(meta=(BindWidget))
	TObjectPtr<UButton> PlayButton;

	virtual void NativeConstruct() override;
	virtual void NativeDestruct() override;

private:
	UFUNCTION()
	void OnPlayPressed();
	UFUNCTION()
	void HandlePlayerInserted(const FEventContext& Context, const FPlayerType& NewPlayer);
};
