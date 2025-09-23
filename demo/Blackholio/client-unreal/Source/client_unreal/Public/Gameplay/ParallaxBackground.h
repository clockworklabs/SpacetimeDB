#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "ParallaxBackground.generated.h"

class UPaperSpriteComponent;

UCLASS()
class CLIENT_UNREAL_API AParallaxBackground : public AActor
{
	GENERATED_BODY()

public:
	AParallaxBackground();

	virtual void Tick(float DeltaTime) override;

	UPROPERTY(EditAnywhere, Category="BH|Parallax")
	float Multiplier = -0.02f;
	UPROPERTY(EditAnywhere, Category="BH|Parallax")
	float FixedY;
};
