#include "Gameplay/UsernameChooserWidget.h"
#include "Components/Button.h"
#include "Components/EditableTextBox.h"
#include "ModuleBindings/Tables/PlayerTable.g.h"
#include "GameManager.h"

void UUsernameChooserWidget::Hide()
{
	SetVisibility(ESlateVisibility::Collapsed);
	if (APlayerController* PC = GetOwningPlayer())
	{
		PC->SetInputMode(FInputModeGameOnly());
	}
}

void UUsernameChooserWidget::NativeConstruct()
{
	Super::NativeConstruct();

	PlayButton->OnClicked.AddDynamic(this, &UUsernameChooserWidget::OnPlayPressed);
	AGameManager* Manager = AGameManager::Instance;
	Manager->Conn->Db->Player->OnInsert.AddDynamic(this, &UUsernameChooserWidget::HandlePlayerInserted);
}

void UUsernameChooserWidget::NativeDestruct()
{
	Super::NativeDestruct();
	AGameManager* Manager = AGameManager::Instance;
	Manager->Conn->Db->Player->OnInsert.RemoveDynamic(this, &UUsernameChooserWidget::HandlePlayerInserted);
}

void UUsernameChooserWidget::OnPlayPressed()
{
	FString Name = UsernameInputField ? UsernameInputField->GetText().ToString().TrimStartAndEnd() : TEXT("");
	if (Name.IsEmpty())
	{
		Name = TEXT("<No Name>");
	}
	AGameManager* Manager = AGameManager::Instance;
	Manager->Conn->Reducers->EnterGame(Name);

	SetVisibility(ESlateVisibility::Collapsed);
	if (APlayerController* PC = GetOwningPlayer())
	{
		PC->SetInputMode(FInputModeGameOnly());
	}
}

void UUsernameChooserWidget::HandlePlayerInserted(const FEventContext& Context, const FPlayerType& NewPlayer)
{
	AGameManager* Manager = AGameManager::Instance;
	if (UsernameInputField && NewPlayer.Identity == Manager->LocalIdentity)
	{
		UsernameInputField->SetText(FText::FromString(NewPlayer.Name));
	}
}
