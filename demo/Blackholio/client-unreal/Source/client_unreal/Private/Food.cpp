#include "Food.h"
#include "ModuleBindings/Types/FoodType.g.h"

AFood::AFood()
{
	ColorPalette = {
		// Greenish
		FLinearColor::FromSRGBColor(FColor(119, 252, 173, 255)),
		FLinearColor::FromSRGBColor(FColor(76,  250, 146, 255)),
		FLinearColor::FromSRGBColor(FColor(35,  246, 120, 255)),

		// Aqua / Teal
		FLinearColor::FromSRGBColor(FColor(119, 251, 201, 255)),
		FLinearColor::FromSRGBColor(FColor(76,  249, 184, 255)),
		FLinearColor::FromSRGBColor(FColor(35,  245, 165, 255)),
	};
}

void AFood::Spawn(const FFoodType& FoodEntity)
{
	Super::Spawn(FoodEntity.EntityId);

	const int32 Index = ColorPalette.Num() ? static_cast<int32>(EntityId % ColorPalette.Num()) : 0;
	const FLinearColor Color = ColorPalette.IsValidIndex(Index) ? ColorPalette[Index] : FLinearColor::Green;
	SetColor(Color);
}
