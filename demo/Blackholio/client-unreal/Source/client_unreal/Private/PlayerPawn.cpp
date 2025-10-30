#include "PlayerPawn.h"

#include "BlackholioPlayerController.h"
#include "Circle.h"
#include "GameManager.h"
#include "Camera/CameraComponent.h"
#include "GameFramework/SpringArmComponent.h"
#include "Kismet/GameplayStatics.h"
#include "ModuleBindings/Tables/EntityTable.g.h"
#include "ModuleBindings/Tables/PlayerTable.g.h"
#include "ModuleBindings/Types/EntityType.g.h"
#include "ModuleBindings/Types/PlayerType.g.h"

APlayerPawn::APlayerPawn()
{
	PrimaryActorTick.bCanEverTick = true;
	USceneComponent* DefaultRoot = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	RootComponent = DefaultRoot;
	
	SpringArm = CreateDefaultSubobject<USpringArmComponent>(TEXT("SpringArm"));
	SpringArm->SetupAttachment(RootComponent);
	SpringArm->SetRelativeRotation(FRotator(0.f, -90.f, 0.f));
	SpringArm->TargetArmLength = 15000.f;
	SpringArm->bUsePawnControlRotation = false;
	SpringArm->bDoCollisionTest = false;

	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(SpringArm);
	Camera->SetProjectionMode(ECameraProjectionMode::Perspective);
	Camera->FieldOfView = 90.f; // top-down 90° FOV
}

void APlayerPawn::Initialize(FPlayerType Player)
{
	PlayerId = Player.PlayerId;

	if (Player.Identity == AGameManager::Instance->LocalIdentity)
	{
		bIsLocalPlayer = true;
		if (APlayerController* PC = UGameplayStatics::GetPlayerController(GetWorld(), 0))
		{
			PC->Possess(this);
		}
	}
}

FString APlayerPawn::GetUsername() const
{
	FPlayerType Player = AGameManager::Instance->Conn->Db->Player->PlayerId->Find(PlayerId);
	return Player.Name;
}

void APlayerPawn::OnCircleSpawned(ACircle* Circle)
{
	if (ensure(Circle))
	{
		OwnedCircles.AddUnique(Circle);
	}
}

void APlayerPawn::OnCircleDeleted(ACircle* Circle)
{
	if (Circle)
	{
		for (int32 i = OwnedCircles.Num() - 1; i >= 0; --i)
		{
			if (!OwnedCircles[i].IsValid() || OwnedCircles[i].Get() == Circle)
			{
				OwnedCircles.RemoveAt(i);
			}
		}
	}
	else
	{
		for (int32 i = OwnedCircles.Num() - 1; i >= 0; --i)
		{
			if (!OwnedCircles[i].IsValid())
			{
				OwnedCircles.RemoveAt(i);
			}
		}
	}

	if (OwnedCircles.Num() == 0 && bIsLocalPlayer)
	{
		if (ABlackholioPlayerController* PlayerController = Cast<ABlackholioPlayerController>(UGameplayStatics::GetPlayerController(GetWorld(), 0)))
		{
			PlayerController->ShowDeathScreen();
		}
	}
}

void APlayerPawn::Split()
{
	AGameManager::Instance->Conn->Reducers->PlayerSplit();
}

void APlayerPawn::Suicide()
{
	AGameManager::Instance->Conn->Reducers->Suicide();
}

int32 APlayerPawn::TotalMass() const
{
	int32 Total = 0;
	for (int32 Index = 0; Index < OwnedCircles.Num(); ++Index)
	{
		const TWeakObjectPtr<ACircle>& Weak = OwnedCircles[Index];
		if (!Weak.IsValid()) continue;

		const ACircle* Circle = Weak.Get();
		const int32 Id = Circle->EntityId;

		const FEntityType Entity = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(Id);
		Total += Entity.Mass;
	}
	return Total;
}

FVector APlayerPawn::CenterOfMass() const
{
	if (OwnedCircles.Num() == 0)
	{
		return FVector::ZeroVector;
	}

	FVector WeightedPosition = FVector::ZeroVector; // Σ (pos * mass)
	double  TotalMass        = 0.0;                 // Σ mass

	const int32 Count = OwnedCircles.Num();

	for (int32 Index = 0; Index < Count; ++Index)
	{
		const TWeakObjectPtr<ACircle>& Weak = OwnedCircles[Index];
		if (!Weak.IsValid()) continue;

		const ACircle* Circle = Weak.Get();
		const int32 Id = Circle->EntityId;

		const FEntityType Entity = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(Id);
		const double Mass = Entity.Mass;

		const FVector Loc = Circle->GetActorLocation();

		if (Mass <= 0.0) continue;

		WeightedPosition += (Loc * Mass);
		TotalMass += Mass;
	}

	const FVector ActorLoc = GetActorLocation();

	FVector Result = FVector::ZeroVector;
	if (TotalMass > 0.0)
	{
		const FVector CalculatedCenter = WeightedPosition / TotalMass;
		// Keep Z at the player's Z, per your original intent
		Result = FVector(CalculatedCenter.X, ActorLoc.Y, CalculatedCenter.Z);
	}

	return Result;
}

void APlayerPawn::Destroyed()
{
	Super::Destroyed();
	for (TWeakObjectPtr<ACircle>& CirclePtr : OwnedCircles)
	{
		if (ACircle* Circle = CirclePtr.Get())
		{
			Circle->Destroy();
		}
	}
	OwnedCircles.Empty();
}

void APlayerPawn::Tick(float DeltaTime)
{
	Super::Tick(DeltaTime);

	if (!bIsLocalPlayer || OwnedCircles.Num() == 0)
		return;

	const FVector ArenaCenter(0.f, 1.f, 0.f);
	FVector Target = ArenaCenter;
	if (AGameManager::Instance->IsConnected())
	{
		const FVector CoM = CenterOfMass();
		if (!CoM.ContainsNaN())
		{
			Target = { CoM.X, 1.f, CoM.Z };
		}
	}
	const FVector NewLoc = FMath::VInterpTo(GetActorLocation(), Target, DeltaTime, 120.f);
	SetActorLocation(NewLoc);


	const float FOVDeg = 90.f; // vertical FOV
	const float HalfAngleRad = FMath::DegreesToRadians(FOVDeg * 0.5f);
	const float TanHalf = FMath::Tan(HalfAngleRad); // = 1.0 at 90°

	const float sizeUnity =
		BaseSize
	  + FMath::Min(MaxMassBonus, TotalMass() / MassToSizeDivisor)
	  + FMath::Min(FMath::Max(OwnedCircles.Num() - 1, 0), 1) * SplitBonus;

	// If you want ~15000 cm at BaseSize (e.g., 50), use scale = 3.f:
	const float Scale = 3.f;

	// Distance that matches Unity size at 90° FOV:
	const float targetArmCm = (Scale * sizeUnity * 100.f) / TanHalf; // TanHalf==1 at 90°

	SpringArm->TargetArmLength = FMath::FInterpTo(
		SpringArm->TargetArmLength, targetArmCm, DeltaTime, /*ZoomSpeed*/2.f);
}

