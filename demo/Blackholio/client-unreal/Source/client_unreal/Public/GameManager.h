#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "GameManager.generated.h"

class UDbConnection;
class AEntity;
class ACircle;
class AFood;
class APlayerPawn;

UCLASS()
class CLIENT_UNREAL_API AGameManager : public AActor
{
	GENERATED_BODY()
	
public:	
	AGameManager();
	static AGameManager* Instance;
	
	UPROPERTY(EditAnywhere, Category="BH|Connection")
	FString ServerUri = TEXT("127.0.0.1:3000");
	UPROPERTY(EditAnywhere, Category="BH|Connection")
	FString ModuleName = TEXT("blackholio-unreal");
	UPROPERTY(EditAnywhere, Category="BH|Connection")
	FString TokenFilePath = TEXT(".spacetime_blackholio");

	UPROPERTY(EditAnywhere, Category="BH|Classes")
	TSubclassOf<ACircle> CircleClass;
	UPROPERTY(EditAnywhere, Category="BH|Classes")
	TSubclassOf<AFood> FoodClass;
	UPROPERTY(EditAnywhere, Category="BH|Classes")
	TSubclassOf<APlayerPawn> PlayerClass;
	
	UPROPERTY(BlueprintReadOnly, Category="BH|Connection")
	FSpacetimeDBIdentity LocalIdentity;
	UPROPERTY(BlueprintReadOnly, Category="BH|Connection")
	UDbConnection* Conn = nullptr;
	UPROPERTY()
	FString PlayerNameAtStart = TEXT("");
	UPROPERTY()
	bool bSubscriptionsApplied = false;
	
	UFUNCTION(BlueprintPure, Category="BH|Connection")
	bool IsConnected() const
	{
		return Conn != nullptr && Conn->IsActive();
	}

	UFUNCTION(BlueprintCallable, Category="BH|Connection")
	void Disconnect()
	{
		if (Conn != nullptr)
		{
			Conn->Disconnect();
			Conn = nullptr;
		}
	}

	UFUNCTION()
	AEntity* GetEntity(int32 EntityId) const;

	UFUNCTION()
	TMap<int32, TWeakObjectPtr<APlayerPawn>> GetPlayerMap() const { return PlayerMap; };
	
protected:
	virtual void BeginPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;

public:	
	virtual void Tick(float DeltaTime) override;

private:
	UFUNCTION()
	void HandleConnect(UDbConnection* InConn, FSpacetimeDBIdentity Identity, const FString& Token);
	UFUNCTION()
	void HandleConnectError(const FString& Error);
	UFUNCTION()
	void HandleDisconnect(UDbConnection* InConn, const FString& Error);
	UFUNCTION()
	void HandleSubscriptionApplied(FSubscriptionEventContext& Context);

	/* Border */
	UFUNCTION()
	void SetupArena(int64 WorldSizeMeters);
	UFUNCTION()
	void CreateBorderCube(const FVector2f Position, const FVector2f Size) const;
	
	UPROPERTY(VisibleAnywhere, Category="Arena")
	UInstancedStaticMeshComponent* BorderISM;
	UPROPERTY(EditDefaultsOnly, Category="Arena", meta=(ClampMin="1.0"))
	float BorderThickness = 50.0f;
	UPROPERTY(EditDefaultsOnly, Category="Arena", meta=(ClampMin="1.0"))
	float BorderHeight = 100.0f;
	UPROPERTY(EditDefaultsOnly, Category="Arena")
	UMaterialInterface* BorderMaterial = nullptr;
	UPROPERTY(EditDefaultsOnly, Category="Arena")
	UStaticMesh* CubeMesh = nullptr;        // defaults as /Engine/BasicShapes/Cube.Cube	
	/* Border */

	/* Data Bindings */
	UPROPERTY()
	TMap<int32, TWeakObjectPtr<AEntity>> EntityMap;
	UPROPERTY()
	TMap<int32, TWeakObjectPtr<APlayerPawn>> PlayerMap;
	
	APlayerPawn* SpawnOrGetPlayer(const FPlayerType& PlayerRow);
	ACircle* SpawnCircle(const FCircleType& CircleRow);
	AFood* SpawnFood(const FFoodType& Food);

	UFUNCTION()
	void OnCircleInsert(const FEventContext& Context, const FCircleType& NewRow);
	UFUNCTION()
	void OnEntityUpdate(const FEventContext& Context, const FEntityType& OldRow, const FEntityType& NewRow);
	UFUNCTION()
	void OnEntityDelete(const FEventContext& Context, const FEntityType& RemovedRow);
	UFUNCTION()
	void OnFoodInsert(const FEventContext& Context, const FFoodType& NewFood);
	UFUNCTION()
	void OnPlayerInsert(const FEventContext& Context, const FPlayerType& NewRow);
	UFUNCTION()
	void OnPlayerDelete(const FEventContext& Context, const FPlayerType& RemovedRow);
	/* Data Bindings */
};
