#include "Gameplay/LeaderboardWidget.h"

#include "GameManager.h"
#include "PlayerPawn.h"
#include "Components/VerticalBox.h"
#include "Gameplay/LeaderboardRowWidget.h"

void ULeaderboardWidget::NativeConstruct()
{
	Super::NativeConstruct();
	BuildRowPool();

	if (UpdatePeriod > 0.f)
	{
		GetWorld()->GetTimerManager().SetTimer(
			UpdateTimer, this, &ULeaderboardWidget::UpdateLeaderboard, UpdatePeriod, true, 0.0f);
	}

	UpdateLeaderboard();
}

void ULeaderboardWidget::NativeDestruct()
{
	if (UWorld* World = GetWorld())
	{
		World->GetTimerManager().ClearTimer(UpdateTimer);
	}
	Super::NativeDestruct();
}

void ULeaderboardWidget::BuildRowPool()
{
	if (!Root || !RowClass) { return; }

	Rows.Reset();

	for (int32 i = 0; i < MaxRowCount; ++i)
	{
		ULeaderboardRowWidget* Row = CreateWidget<ULeaderboardRowWidget>(this, RowClass);
		if (!Row) { continue; }

		Root->AddChild(Row);
		Row->SetVisibility(ESlateVisibility::Collapsed);
		Rows.Add(Row);
	}
}

void ULeaderboardWidget::CollectPlayers(TArray<FLeaderboardEntry>& Out) const
{
	Out.Reset();

	const AGameManager* GM = AGameManager::Instance;
	if (!GM) return;

	TMap<int32, TWeakObjectPtr<APlayerPawn>> PlayerMap = GM->GetPlayerMap();
	if (PlayerMap.Num() == 0) return;

	// 2) Build entries: mass > 0 only
	for (const TPair<int32, TWeakObjectPtr<APlayerPawn>>& Pair : PlayerMap)
	{
		APlayerPawn* Pawn = Pair.Value.Get();
		if (!Pawn) continue;

		const int32 Mass = static_cast<int32>(Pawn->TotalMass());
		if (Mass == 0) continue;

		FLeaderboardEntry E;
		E.Username = Pawn->GetUsername();
		E.Mass = Mass;
		E.Pawn = Pawn;
		Out.Add(MoveTemp(E));
	}

	// 3) Sort by mass desc (stable by Username as a tiebreaker for consistent ordering)
	Out.Sort([](const FLeaderboardEntry& A, const FLeaderboardEntry& B)
	{
		if (A.Mass != B.Mass) return A.Mass > B.Mass;
		return A.Username < B.Username;
	});

	// 4) Keep top 10
	if (Out.Num() > 10)
	{
		Out.SetNum(10, EAllowShrinking::No);
	}

	// 5) Append local player if not already present and has Mass > 0
	APlayerController* PC = GetOwningPlayer();
	if (PC)
	{
		if (APlayerPawn* LocalPawn = Cast<APlayerPawn>(PC->GetPawn()))
		{
			const bool AlreadyIn = Out.ContainsByPredicate(
				[LocalPawn](const FLeaderboardEntry& E){ return E.Pawn.Get() == LocalPawn; });

			const int32 LocalMass = static_cast<int32>(LocalPawn->TotalMass());
			if (!AlreadyIn && LocalMass > 0)
			{
				FLeaderboardEntry Local;
				Local.Username = LocalPawn->GetUsername();
				Local.Mass = LocalMass;
				Local.Pawn = LocalPawn;
				Out.Add(MoveTemp(Local));
			}
		}
	}
}

void ULeaderboardWidget::UpdateLeaderboard()
{
	if (Rows.Num() == 0) return;

	TArray<FLeaderboardEntry> Players;
	CollectPlayers(Players);

	int32 i = 0;
	for (; i < Players.Num() && i < Rows.Num(); ++i)
	{
		if (ULeaderboardRowWidget* Row = Rows[i])
		{
			Row->SetData(Players[i].Username, Players[i].Mass);
			Row->SetVisibility(ESlateVisibility::SelfHitTestInvisible);
		}
	}

	// Hide the rest
	for (; i < Rows.Num(); ++i)
	{
		if (ULeaderboardRowWidget* Row = Rows[i])
		{
			Row->SetVisibility(ESlateVisibility::Collapsed);
		}
	}
}
