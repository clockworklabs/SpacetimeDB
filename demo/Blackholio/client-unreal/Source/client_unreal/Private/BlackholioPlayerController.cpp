#include "BlackholioPlayerController.h"

#include "DbVector2.h"
#include "EnhancedInputComponent.h"
#include "EnhancedInputSubsystems.h"
#include "GameManager.h"
#include "PlayerPawn.h"
#include "Blueprint/UserWidget.h"
#include "Gameplay/LeaderboardWidget.h"
#include "Gameplay/RespawnWidget.h"
#include "Gameplay/UsernameChooserWidget.h"

ABlackholioPlayerController::ABlackholioPlayerController()
{
	bShowMouseCursor = true;
	bEnableClickEvents = true;
	bEnableMouseOverEvents = true;
	PrimaryActorTick.bCanEverTick = true;
}

void ABlackholioPlayerController::ShowDeathScreen()
{
	if (!IsLocalController() || !RespawnClass) return;

	if (!RespawnWidget)
	{
		RespawnWidget = CreateWidget<URespawnWidget>(this, RespawnClass);
		RespawnWidget->AddToViewport(100);
	}
	else if (!RespawnWidget->IsInViewport())
	{
		RespawnWidget->AddToViewport(100);
	}

	RespawnWidget->SetVisibility(ESlateVisibility::Visible);
	FInputModeUIOnly InputMode;
	InputMode.SetWidgetToFocus(RespawnWidget->TakeWidget());
	InputMode.SetLockMouseToViewportBehavior(EMouseLockMode::DoNotLock);
	SetInputMode(InputMode);
	bShowMouseCursor = true;
	//RespawnWidget->SetKeyboardFocus();
}

void ABlackholioPlayerController::BeginPlay()
{
	Super::BeginPlay();

	if (!LeaderboardWidget && LeaderboardClass)
	{
		LeaderboardWidget = CreateWidget<ULeaderboardWidget>(this, LeaderboardClass);
		LeaderboardWidget->AddToViewport(100);
		LeaderboardWidget->SetVisibility(ESlateVisibility::Visible);
	}
	SetInputMode(FInputModeGameOnly());
}

void ABlackholioPlayerController::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	if (!AGameManager::Instance || !AGameManager::Instance->IsConnected())
	{
		return;
	}

	const float Now = GetWorld() ? GetWorld()->GetTimeSeconds() : 0.f;
	if ((Now - LastMovementSendTimestamp) >= SendUpdatesFrequency)
	{
		LastMovementSendTimestamp = Now;
		FVector2D LatestDesiredDirection = ComputeDesiredDirection();
		if ((LatestDesiredDirection.X != 0 && LatestDesiredDirection.Y != 0))
		{
			const FVector2D ConvertDirection = FVector2D(LatestDesiredDirection.X, LatestDesiredDirection.Y*-1);
			AGameManager::Instance->Conn->Reducers->UpdatePlayerInput(ToDbVector(ConvertDirection));
		}
	}

	if (!AGameManager::Instance->bSubscriptionsApplied) return;	
	if (AGameManager::Instance->PlayerNameAtStart.IsEmpty() && !bShowedUsernameChooser)
	{
		bShowedUsernameChooser = true;
		if (IsLocalController() && UsernameChooserClass)
		{
			this->UsernameChooserWidget = CreateWidget<UUsernameChooserWidget>(this, UsernameChooserClass);
			if (this->UsernameChooserWidget)
			{
				this->UsernameChooserWidget->AddToViewport(100);
				SetInputMode(FInputModeUIOnly()); // should focus the textbox
				bShowMouseCursor = true;
			}
		}
		//this->UsernameChooserWidget->Hide();
	}
}

void ABlackholioPlayerController::OnPossess(APawn* InPawn)
{
	Super::OnPossess(InPawn);
	LocalPlayer = Cast<APlayerPawn>(InPawn);
	EnsureMappingContext();
}

void ABlackholioPlayerController::SetupInputComponent()
{
	Super::SetupInputComponent();
	if (UEnhancedInputComponent* EIC = Cast<UEnhancedInputComponent>(InputComponent))
	{
		if (SplitAction)
		{
			EIC->BindAction(SplitAction, ETriggerEvent::Triggered, this, &ABlackholioPlayerController::OnSplitTriggered);
		}
		if (SuicideAction)
		{
			EIC->BindAction(SuicideAction, ETriggerEvent::Triggered, this, &ABlackholioPlayerController::OnSuicideTriggered);
		}
		if (ToggleInputLockAction)
		{
			EIC->BindAction(ToggleInputLockAction, ETriggerEvent::Triggered, this, &ABlackholioPlayerController::OnToggleInputLockTriggered);
		}
	}
}

FVector2D ABlackholioPlayerController::ComputeDesiredDirection() const
{
	int32 SizeX = 0, SizeY = 0;
	GetViewportSize(SizeX, SizeY);
	if (SizeX <= 0 || SizeY <= 0)
	{
		return FVector2D::ZeroVector;
	}

	const FVector2D ViewportCenter(static_cast<float>(SizeX) * 0.5f, static_cast<float>(SizeY) * 0.5f);

	FVector2D MousePos = ViewportCenter;
	if (!LockInputPosition.IsSet())
	{
		float MouseX = 0.f, MouseY = 0.f;
		if (!GetMousePosition(MouseX, MouseY))
		{
			return FVector2D::ZeroVector;
		}
		MousePos = FVector2D(MouseX, MouseY);
	}
	else
	{
		MousePos = LockInputPosition.GetValue();
	}

	if (MousePos.X < 0.f || MousePos.X >= SizeX || MousePos.Y < 0.f || MousePos.Y >= SizeY)
	{
		return FVector2D::ZeroVector;
	}
	
	const float Denominator = FMath::Max(1.f, static_cast<float>(SizeY) / 3.f);
	const FVector2D DesiredDir = (MousePos - ViewportCenter) / Denominator;
	return DesiredDir;
}

void ABlackholioPlayerController::EnsureMappingContext() const
{
	if (!PlayerMappingContext) return;

	if (ULocalPlayer* LP = GetLocalPlayer())
	{
		if (UEnhancedInputLocalPlayerSubsystem* Subsystem = ULocalPlayer::GetSubsystem<UEnhancedInputLocalPlayerSubsystem>(LP))
		{
			Subsystem->AddMappingContext(PlayerMappingContext, 0);
		}
	}
}

void ABlackholioPlayerController::OnSplitTriggered(const FInputActionValue& Value)
{
	LocalPlayer->Split();
}

void ABlackholioPlayerController::OnSuicideTriggered(const FInputActionValue& Value)
{
	LocalPlayer->Suicide();
}

void ABlackholioPlayerController::OnToggleInputLockTriggered(const FInputActionValue& Value)
{
	if (LockInputPosition.IsSet())
	{
		LockInputPosition.Reset();
	}
	else
	{
		float X, Y;
		if (GetMousePosition(X, Y))
		{
			LockInputPosition = FVector2D(X, Y);
		}
	}
}
