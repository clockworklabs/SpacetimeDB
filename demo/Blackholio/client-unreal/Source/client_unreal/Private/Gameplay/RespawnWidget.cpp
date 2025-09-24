#include "Gameplay/RespawnWidget.h"
#include "Components/Button.h"
#include "GameManager.h"

void URespawnWidget::NativeConstruct()
{
	Super::NativeConstruct();
	RespawnButton->OnClicked.AddDynamic(this, &URespawnWidget::OnRespawnPressed);
}

void URespawnWidget::OnRespawnPressed()
{
	AGameManager* Manager = AGameManager::Instance;
	UE_LOG(LogTemp, Warning, TEXT("Respawn calling reducer"));
	Manager->Conn->Reducers->Respawn();
	UE_LOG(LogTemp, Warning, TEXT("Respawn reducer called"));
	SetVisibility(ESlateVisibility::Collapsed);
	if (APlayerController* PC = GetOwningPlayer())
	{
		PC->SetInputMode(FInputModeGameOnly());
		//PC->bShowMouseCursor = false;
	}
}
