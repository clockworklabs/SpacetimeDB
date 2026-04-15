#include "Tests/TestHandler.h"

void UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Insert(const FEventContext&, const FPlayerType& Value)
{
	if (Value.Identity != ExpectedIdentity)
	{
		Counter->MarkFailure(TEXT("players_at_level_0_insert"), FString::Printf(TEXT("Unexpected identity %s"), *Value.Identity.ToHex()));
		Counter->Abort();
		return;
	}

	Counter->MarkSuccess(TEXT("players_at_level_0_insert"));
}

void UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Update(const FEventContext&, const FPlayerType&, const FPlayerType&)
{
	Counter->MarkFailure(TEXT("players_at_level_0_insert"), TEXT("Unexpected update for players_at_level_0"));
	Counter->Abort();
}

void UViewBlueprintRuntimeHandler::OnPlayersAtLevel0Delete(const FEventContext&, const FPlayerType&)
{
	Counter->MarkFailure(TEXT("players_at_level_0_insert"), TEXT("Unexpected delete for players_at_level_0"));
	Counter->Abort();
}
