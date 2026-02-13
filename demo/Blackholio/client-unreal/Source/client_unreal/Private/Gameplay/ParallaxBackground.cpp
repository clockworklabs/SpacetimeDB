#include "Gameplay/ParallaxBackground.h"

#include "GameManager.h"
#include "Kismet/GameplayStatics.h"
#include "ModuleBindings/Tables/ConfigTable.g.h"

AParallaxBackground::AParallaxBackground()
{
	PrimaryActorTick.bCanEverTick = true;
}

void AParallaxBackground::Tick(float DeltaTime)
{
	Super::Tick(DeltaTime);

	APlayerCameraManager* PCM = UGameplayStatics::GetPlayerCameraManager(this, 0);
	if (!PCM) return;

	const FVector Cam = PCM->GetCameraLocation();

	uint64 WorldSize = AGameManager::Instance->Conn->Db->Config->Id->Find(0).WorldSize;
	float WorldCenter = (WorldSize);
	const FVector NewLoc(
		(Cam.X+WorldCenter) * Multiplier,    // plane X
		FixedY,                // depth constant
		(Cam.Z+WorldCenter) * Multiplier     // plane Z
	);

	SetActorLocation(NewLoc);
}

