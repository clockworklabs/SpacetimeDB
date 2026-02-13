#include "Circle.h"
#include "PlayerPawn.h"
#include "ModuleBindings/Types/CircleType.g.h"

ACircle::ACircle()
{
	ColorPalette = {
		// Yellow
		FLinearColor::FromSRGBColor(FColor(175, 159, 49, 255)),
		FLinearColor::FromSRGBColor(FColor(175, 116, 49, 255)),

		// Purple
		FLinearColor::FromSRGBColor(FColor(112, 47, 252, 255)),
		FLinearColor::FromSRGBColor(FColor(51,  91, 252, 255)),

		// Red
		FLinearColor::FromSRGBColor(FColor(176, 54, 54, 255)),
		FLinearColor::FromSRGBColor(FColor(176, 109, 54, 255)),
		FLinearColor::FromSRGBColor(FColor(141, 43, 99, 255)),

		// Blue
		FLinearColor::FromSRGBColor(FColor(2,   188, 250, 255)),
		FLinearColor::FromSRGBColor(FColor(7,   50,  251, 255)),
		FLinearColor::FromSRGBColor(FColor(2,   28,  146, 255)),
	};
}

void ACircle::Spawn(const FCircleType& Circle, APlayerPawn* InOwner)
{
	Super::Spawn(Circle.EntityId);

	const int32 Index = ColorPalette.Num() ? static_cast<int32>(InOwner->PlayerId % ColorPalette.Num()) : 0;
	const FLinearColor Color = ColorPalette.IsValidIndex(Index) ? ColorPalette[Index] : FLinearColor::Green;
	SetColor(Color);

	this->Owner = InOwner;
	SetUsername(InOwner->GetUsername());
}

void ACircle::OnDelete(const FEventContext& Context)
{
	Super::OnDelete(Context);
	Owner->OnCircleDeleted(this);
}

void ACircle::SetUsername(const FString& InUsername)
{
	if (Username.Equals(InUsername, ESearchCase::CaseSensitive))
		return;

	Username = InUsername;
	OnUsernameChanged.Broadcast(Username);
}
