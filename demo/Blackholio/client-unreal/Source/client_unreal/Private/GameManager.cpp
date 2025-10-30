#include "GameManager.h"
#include "Circle.h"
#include "Entity.h"
#include "Food.h"
#include "PlayerPawn.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/Tables/CircleTable.g.h"
#include "ModuleBindings/Tables/ConfigTable.g.h"
#include "ModuleBindings/Tables/EntityTable.g.h"
#include "ModuleBindings/Tables/FoodTable.g.h"
#include "ModuleBindings/Tables/PlayerTable.g.h"

AGameManager* AGameManager::Instance = nullptr;

AGameManager::AGameManager()
{
	PrimaryActorTick.bCanEverTick = true;
	PrimaryActorTick.bStartWithTickEnabled = true;
	
	BorderISM = CreateDefaultSubobject<UInstancedStaticMeshComponent>(TEXT("BorderISM"));
	SetRootComponent(BorderISM);

	if (CubeMesh != nullptr)
		return;
    
	static ConstructorHelpers::FObjectFinder<UStaticMesh> CubeAsset(TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (CubeAsset.Succeeded())
	{
		CubeMesh = CubeAsset.Object;
	}
}

AEntity* AGameManager::GetEntity(int32 EntityId) const
{
	if (const TWeakObjectPtr<AEntity>* WeakEntity = EntityMap.Find(EntityId))
	{
		if (!WeakEntity->IsValid())
		{
			return nullptr;
		}
		if (AEntity* Entity = WeakEntity->Get())
		{
			return Entity;
		}
	}

	return nullptr;
}

void AGameManager::BeginPlay()
{
	Super::BeginPlay();
	Instance = this;

	FOnConnectDelegate ConnectDelegate;
	BIND_DELEGATE_SAFE(ConnectDelegate, this, AGameManager, HandleConnect);
	FOnDisconnectDelegate DisconnectDelegate;
	BIND_DELEGATE_SAFE(DisconnectDelegate, this, AGameManager, HandleDisconnect);
	FOnConnectErrorDelegate ConnectErrorDelegate;
	BIND_DELEGATE_SAFE(ConnectErrorDelegate, this, AGameManager, HandleConnectError);

	UCredentials::Init(FString::Printf(TEXT("%s-%s"), *TokenFilePath, *ServerUri));
	FString Token = UCredentials::LoadToken();

	UDbConnectionBuilder* Builder = UDbConnection::Builder()
	                                            ->WithUri(ServerUri)
	                                            ->WithModuleName(ModuleName)
	                                            ->OnConnect(ConnectDelegate)
	                                            ->OnDisconnect(DisconnectDelegate)
	                                            ->OnConnectError(ConnectErrorDelegate);

	if (!Token.IsEmpty())
	{
		Builder->WithToken(Token);
	}

	Conn = Builder->Build();
}

void AGameManager::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
	Disconnect();
	if (Instance == this)
	{
		Instance = nullptr;
	}
	Super::EndPlay(EndPlayReason);
}

void AGameManager::Tick(float DeltaTime)
{
	if (IsConnected())
	{
		Conn->FrameTick();
	}
}

void AGameManager::HandleConnect(UDbConnection* InConn, FSpacetimeDBIdentity Identity, const FString& Token)
{
	UE_LOG(LogTemp, Log, TEXT("Connected."));
	UCredentials::SaveToken(Token);
	LocalIdentity = Identity;

	Conn->Db->Circle->OnInsert.AddDynamic(this, &AGameManager::OnCircleInsert);
	Conn->Db->Entity->OnUpdate.AddDynamic(this, &AGameManager::OnEntityUpdate);
	Conn->Db->Entity->OnDelete.AddDynamic(this, &AGameManager::OnEntityDelete);
	Conn->Db->Food->OnInsert.AddDynamic(this, &AGameManager::OnFoodInsert);
	Conn->Db->Player->OnInsert.AddDynamic(this, &AGameManager::OnPlayerInsert);
	Conn->Db->Player->OnDelete.AddDynamic(this, &AGameManager::OnPlayerDelete);

	FOnSubscriptionApplied AppliedDelegate;
	BIND_DELEGATE_SAFE(AppliedDelegate, this, AGameManager, HandleSubscriptionApplied);
	Conn->SubscriptionBuilder()
		->OnApplied(AppliedDelegate)
		->SubscribeToAllTables();
}

void AGameManager::HandleConnectError(const FString& Error)
{
	UE_LOG(LogTemp, Log, TEXT("Connection error %s"), *Error);
}

void AGameManager::HandleDisconnect(UDbConnection* InConn, const FString& Error)
{
	UE_LOG(LogTemp, Log, TEXT("Disconnected."));
	if (!Error.IsEmpty())
	{
		UE_LOG(LogTemp, Log, TEXT("Disconnect error %s"), *Error);
	}
}

void AGameManager::HandleSubscriptionApplied(FSubscriptionEventContext& Context)
{
	UE_LOG(LogTemp, Log, TEXT("Subscription applied!"));
	this->bSubscriptionsApplied = true;
	
	// Once we have the initial subscription sync'd to the client cache
	// Get the world size from the config table and set up the arena
	int64 WorldSize = Conn->Db->Config->Id->Find(0).WorldSize;
	SetupArena(WorldSize);

	FPlayerType Player = Context.Db->Player->Identity->Find(LocalIdentity);
	if (!Player.Name.IsEmpty())
	{
		this->PlayerNameAtStart = Player.Name;
		if (Context.Db->Circle->PlayerId->Filter(Player.PlayerId).Num() == 0)
		{
			Context.Reducers->EnterGame(Player.Name);
		}
	}
}

void AGameManager::SetupArena(int64 WorldSizeMeters)
{
	if (!BorderISM || !CubeMesh) return;

	BorderISM->ClearInstances();
	BorderISM->SetStaticMesh(CubeMesh);
	if (BorderMaterial)
	{
		BorderISM->SetMaterial(0, BorderMaterial);
	}

	// Convert from meters (int64) → centimeters (double for precision)
	const double worldSizeCmDouble = static_cast<double>(WorldSizeMeters) * 100.0;

	// Clamp to avoid float overflow in transforms
	const double clampedWorldSizeCmDouble = FMath::Clamp(
		worldSizeCmDouble,
		0.0,
		FLT_MAX * 0.25 // safe margin
	);

	// Convert to float for actual Unreal math
	const float worldSizeCm = static_cast<float>(clampedWorldSizeCmDouble);

	const float borderThicknessCm = BorderThickness; // already cm

	// Create four borders
	CreateBorderCube(
		FVector2f(worldSizeCm * 0.5f, worldSizeCm + borderThicknessCm * 0.5f), // North
		FVector2f(worldSizeCm + borderThicknessCm * 2.0f, borderThicknessCm)
	);

	CreateBorderCube(
		FVector2f(worldSizeCm * 0.5f, -borderThicknessCm * 0.5f), // South
		FVector2f(worldSizeCm + borderThicknessCm * 2.0f, borderThicknessCm)
	);

	CreateBorderCube(
		FVector2f(worldSizeCm + borderThicknessCm * 0.5f, worldSizeCm * 0.5f), // East
		FVector2f(borderThicknessCm, worldSizeCm + borderThicknessCm * 2.0f)
	);

	CreateBorderCube(
		FVector2f(-borderThicknessCm * 0.5f, worldSizeCm * 0.5f), // West
		FVector2f(borderThicknessCm, worldSizeCm + borderThicknessCm * 2.0f)
	);	
}

void AGameManager::CreateBorderCube(const FVector2f Position, const FVector2f Size) const
{
	// Scale from the 100cm default cube to desired size (in cm)
	const FVector Scale(Size.X / 100.0f, BorderHeight / 100.0f, Size.Y / 100.0f);

	// Place so the bottom sits on Z=0 (cube is centered)
	const FVector Location(Position.X, BorderHeight * 0.5f, Position.Y);

	const FTransform Transform(FRotator::ZeroRotator, Location, Scale);
	BorderISM->AddInstance(Transform);
}

void AGameManager::OnCircleInsert(const FEventContext& Context, const FCircleType& NewRow)
{
    if (EntityMap.Contains(NewRow.EntityId)) return;
    SpawnCircle(NewRow);
}

void AGameManager::OnEntityUpdate(const FEventContext& Context, const FEntityType& OldRow, const FEntityType& NewRow)
{
    if (TWeakObjectPtr<AEntity>* WeakEntity = EntityMap.Find(NewRow.EntityId))
    {
        if (!WeakEntity->IsValid())
        {
            return;
        }
        if (AEntity* Entity = WeakEntity->Get())
        {
            Entity->OnEntityUpdated(NewRow);
        }
    }
}

void AGameManager::OnEntityDelete(const FEventContext& Context, const FEntityType& RemovedRow)
{
    TWeakObjectPtr<AEntity> EntityPtr;
    const bool bHadEntry = EntityMap.RemoveAndCopyValue(RemovedRow.EntityId, EntityPtr);
    const bool bIsValid =EntityPtr.IsValid(); 
    if (!bHadEntry || !bIsValid)
    {
        return;
    }

    if (AEntity* Entity = EntityPtr.Get())
    {
        Entity->OnDelete(Context);
    }
}

void AGameManager::OnFoodInsert(const FEventContext& Context, const FFoodType& NewRow)
{
    if (EntityMap.Contains(NewRow.EntityId)) return;
    SpawnFood(NewRow);
}

void AGameManager::OnPlayerInsert(const FEventContext& Context, const FPlayerType& NewRow)
{
    SpawnOrGetPlayer(NewRow);
}

void AGameManager::OnPlayerDelete(const FEventContext& Context, const FPlayerType& RemovedRow)
{
    TWeakObjectPtr<APlayerPawn> PlayerPtr;
    const bool bHadEntry = PlayerMap.RemoveAndCopyValue(RemovedRow.PlayerId, PlayerPtr);

    if (!bHadEntry || !PlayerPtr.IsValid())
    {
        return;
    }

    if (APlayerPawn* Player = PlayerPtr.Get())
    {
        Player->Destroy();
    }
}

APlayerPawn* AGameManager::SpawnOrGetPlayer(const FPlayerType& PlayerRow)
{
    TWeakObjectPtr<APlayerPawn> WeakPlayer = PlayerMap.FindRef(PlayerRow.PlayerId);
    if (WeakPlayer.IsValid())
    {
        return WeakPlayer.Get();
    }
    
    if (!PlayerClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - PlayerClass not set."));
	    return nullptr;
    }
    FActorSpawnParameters Params;
    Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;
    APlayerPawn* Player = GetWorld()->SpawnActor<APlayerPawn>(PlayerClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Player)
    {
        Player->Initialize(PlayerRow);
        PlayerMap.Add(PlayerRow.PlayerId, Player);
    }
    return Player;
}

ACircle* AGameManager::SpawnCircle(const FCircleType& CircleRow)
{
    if (!CircleClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - CircleClass not set."));
	    return nullptr;
    }
    // Need player row for username
    const FPlayerType PlayerRow = Conn->Db->Player->PlayerId->Find(CircleRow.PlayerId);
    APlayerPawn* OwningPlayer = SpawnOrGetPlayer(PlayerRow);
    
    FActorSpawnParameters Params;
    auto* Circle = GetWorld()->SpawnActor<ACircle>(CircleClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Circle)
    {
        Circle->Spawn(CircleRow, OwningPlayer);
        EntityMap.Add(CircleRow.EntityId, Circle);
        if (OwningPlayer)
            OwningPlayer->OnCircleSpawned(Circle);
    }
    return Circle;
}

AFood* AGameManager::SpawnFood(const FFoodType& FoodEntity)
{
    if (!FoodClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - FoodClass not set."));
        return nullptr;
    }
    
    FActorSpawnParameters Params;
    AFood* Food = GetWorld()->SpawnActor<AFood>(FoodClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Food)
    {
        Food->Spawn(FoodEntity);
        EntityMap.Add(FoodEntity.EntityId, Food);
    }
    return Food;
}