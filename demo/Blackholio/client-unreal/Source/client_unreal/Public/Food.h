#pragma once

#include "CoreMinimal.h"
#include "Entity.h"
#include "Food.generated.h"

struct FFoodType;

UCLASS()
class CLIENT_UNREAL_API AFood : public AEntity
{
	GENERATED_BODY()

public:
	AFood();
	void Spawn(const FFoodType& FoodEntity);
protected:
	UPROPERTY(EditDefaultsOnly, Category="BH|Food")
	TArray<FLinearColor> ColorPalette;
};
