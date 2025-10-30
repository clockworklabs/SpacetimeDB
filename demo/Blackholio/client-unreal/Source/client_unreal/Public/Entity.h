#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Entity.generated.h"

struct FEventContext;
struct FEntityType;

UCLASS()
class CLIENT_UNREAL_API AEntity : public AActor
{
	GENERATED_BODY()
	
public:	
	AEntity();

protected:
	UPROPERTY(EditDefaultsOnly, Category="BH|Entity")
	float LerpTime = 0.f;
	UPROPERTY(EditDefaultsOnly, Category="BH|Entity")
	float LerpDuration = 0.10f;
	UPROPERTY(EditDefaultsOnly, Category="BH|Entity")
	float DespawnTime = 0.2f;
	
	FVector LerpStartPosition = FVector::ZeroVector;
	FVector LerpTargetPosition = FVector::ZeroVector;
	float TargetScale = 1.f;
	
public:
	UPROPERTY(BlueprintReadOnly, Category="BH|Entity")
	int32 EntityId = 0;
	virtual void Tick(float DeltaTime) override;
	void ConsumeDespawn(float DeltaTime);
	
	void Spawn(int32 InEntityId);
	virtual void OnEntityUpdated(const FEntityType& NewVal);
	virtual void OnDelete(const FEventContext& Context);
	bool ConsumeDelete(const FEventContext& Context);
	
	void SetColor(const FLinearColor& Color) const;

	static float MassToRadius(int32 Mass) { return FMath::Sqrt(static_cast<float>(Mass)); }
	static float MassToDiameter(int32 Mass) { return MassToRadius(Mass) * 2.f; }

private:
	UPROPERTY()
	TObjectPtr<AEntity> ConsumingEntity = nullptr;
	bool bIsDespawning = false;
	float DespawnElapsed = 0.f;
	FVector ConsumeStartPosition = FVector::ZeroVector;
	FVector ConsumeStartScale = FVector::ZeroVector;	

};
