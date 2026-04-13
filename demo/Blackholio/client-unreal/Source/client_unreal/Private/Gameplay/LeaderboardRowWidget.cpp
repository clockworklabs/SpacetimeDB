#include "Gameplay/LeaderboardRowWidget.h"

#include "Components/TextBlock.h"

void ULeaderboardRowWidget::SetData(const FString& Username, int32 Mass)
{
	if (UsernameText)
	{
		UsernameText->SetText(FText::FromString(Username));
	}
	if (MassText)
	{
		MassText->SetText(FText::AsNumber(Mass));
	}
}
