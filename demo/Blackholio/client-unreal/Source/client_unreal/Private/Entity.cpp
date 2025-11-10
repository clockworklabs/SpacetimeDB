#include "Entity.h"
#include "DbVector2.h"
#include "GameManager.h"
#include "PaperSpriteComponent.h"
#include "ModuleBindings/Tables/EntityTable.g.h"

AEntity::AEntity()
{
	PrimaryActorTick.bCanEverTick = true;
	LerpTime = 0.f;
}

void AEntity::Tick(float DeltaTime)
{
	Super::Tick(DeltaTime);

	if (bIsDespawning)
	{
		ConsumeDespawn(DeltaTime);
		
		return;
	}
	// Interpolate the position and scale
	LerpTime = FMath::Min(LerpTime + DeltaTime, LerpDuration);
	const float Alpha = (LerpDuration > 0.f) ? (LerpTime / LerpDuration) : 1.f;
	SetActorLocation(FMath::Lerp(LerpStartPosition, LerpTargetPosition, Alpha));
	
	const float NewScale = FMath::FInterpTo(GetActorScale3D().X, TargetScale, DeltaTime, 8.f);
	SetActorScale3D(FVector(NewScale));
}

void AEntity::ConsumeDespawn(float DeltaTime)
{
	if (!bIsDespawning || !ConsumingEntity)
		return;

	DespawnElapsed = FMath::Min(DespawnElapsed + DeltaTime, DespawnTime);
	const float T = (DespawnTime > 0.f) ? (DespawnElapsed / DespawnTime) : 1.f;

	const FVector CurrentTargetPos = ConsumingEntity->GetActorLocation();
	SetActorLocation(FMath::Lerp(ConsumeStartPosition, CurrentTargetPos, T));
	SetActorScale3D(FMath::Lerp(ConsumeStartScale, FVector::ZeroVector, T));

	if (DespawnElapsed >= DespawnTime)
	{
		bIsDespawning = false;
		ConsumingEntity = nullptr;
		Destroy();
	}
}

void AEntity::Spawn(int32 InEntityId)
{
	EntityId = InEntityId;

	const FEntityType EntityRow = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(InEntityId);

	LerpStartPosition = LerpTargetPosition = ToFVector(EntityRow.Position);
	SetActorLocation(LerpStartPosition);
	TargetScale = MassToDiameter(EntityRow.Mass);
	SetActorScale3D(FVector::OneVector);
	LerpTime = 0.f;
}

void AEntity::OnEntityUpdated(const FEntityType& NewVal)
{
	LerpStartPosition = GetActorLocation();
	LerpTargetPosition = ToFVector(NewVal.Position);
	TargetScale = MassToDiameter(NewVal.Mass);
	LerpTime = 0.f;
}

void AEntity::OnDelete(const FEventContext& Context)
{
	if (ConsumeDelete(Context))
		return;
	
	Destroy();
}

bool AEntity::ConsumeDelete(const FEventContext& Context)
{
	if (!Context.Event.IsReducer())
		return false;

	const FReducer Reducer = Context.Event.GetAsReducer();

	if (!Reducer.IsConsumeEntity())
		return false;

	const FConsumeEntityArgs Args = Reducer.GetAsConsumeEntity();
	const int32 ConsumerId = Args.Request.ConsumerEntityId;
	ConsumingEntity = AGameManager::Instance->GetEntity(ConsumerId);
	if (!ConsumingEntity)
		return false;

	bIsDespawning = true;
	DespawnElapsed = 0.f;
	ConsumeStartPosition = GetActorLocation();
	ConsumeStartScale = GetActorScale3D();
	return true;
}

void AEntity::SetColor(const FLinearColor& Color) const
{
	if (UPaperSpriteComponent* SpriteComponent = FindComponentByClass<UPaperSpriteComponent>())
	{
		SpriteComponent->SetSpriteColor(Color);
	}
}