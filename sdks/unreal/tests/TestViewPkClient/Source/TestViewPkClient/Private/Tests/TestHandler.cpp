#include "Tests/TestHandler.h"

namespace
{
bool ValidateInsertedRow(UViewPkRuntimeHandler* Handler, const FString& StepName, const FViewPkPlayerType& Value)
{
	if (Value.Id != Handler->ExpectedId)
	{
		Handler->Counter->MarkFailure(StepName, FString::Printf(TEXT("Unexpected id %llu"), static_cast<unsigned long long>(Value.Id)));
		Handler->Counter->Abort();
		return false;
	}
	if (Value.Name != Handler->InitialName)
	{
		Handler->Counter->MarkFailure(StepName, FString::Printf(TEXT("Unexpected insert name %s"), *Value.Name));
		Handler->Counter->Abort();
		return false;
	}
	return true;
}

bool ValidateUpdatedRows(UViewPkRuntimeHandler* Handler, const FString& StepName, const FViewPkPlayerType& OldValue, const FViewPkPlayerType& NewValue)
{
	if (OldValue.Id != Handler->ExpectedId || NewValue.Id != Handler->ExpectedId)
	{
		Handler->Counter->MarkFailure(StepName, TEXT("Unexpected row id during update"));
		Handler->Counter->Abort();
		return false;
	}
	if (OldValue.Name != Handler->InitialName)
	{
		Handler->Counter->MarkFailure(StepName, FString::Printf(TEXT("Unexpected old name %s"), *OldValue.Name));
		Handler->Counter->Abort();
		return false;
	}
	if (NewValue.Name != Handler->UpdatedName)
	{
		Handler->Counter->MarkFailure(StepName, FString::Printf(TEXT("Unexpected new name %s"), *NewValue.Name));
		Handler->Counter->Abort();
		return false;
	}
	return true;
}
}

void UViewPkRuntimeHandler::OnAllViewPkPlayersInsert(const FEventContext&, const FViewPkPlayerType& Value)
{
	if (ValidateInsertedRow(this, TEXT("all_view_pk_players_insert"), Value))
	{
		Counter->MarkSuccess(TEXT("all_view_pk_players_insert"));
	}
}

void UViewPkRuntimeHandler::OnAllViewPkPlayersUpdate(const FEventContext&, const FViewPkPlayerType& OldValue, const FViewPkPlayerType& NewValue)
{
	if (ValidateUpdatedRows(this, TEXT("all_view_pk_players_update"), OldValue, NewValue))
	{
		Counter->MarkSuccess(TEXT("all_view_pk_players_update"));
	}
}

void UViewPkRuntimeHandler::OnAllViewPkPlayersDelete(const FEventContext&, const FViewPkPlayerType&)
{
	Counter->MarkFailure(TEXT("all_view_pk_players_update"), TEXT("Unexpected delete for all_view_pk_players"));
	Counter->Abort();
}

void UViewPkRuntimeHandler::OnSenderViewPkPlayersAInsert(const FEventContext&, const FViewPkPlayerType& Value)
{
	if (ValidateInsertedRow(this, TEXT("sender_view_pk_players_a_insert"), Value))
	{
		Counter->MarkSuccess(TEXT("sender_view_pk_players_a_insert"));
	}
}

void UViewPkRuntimeHandler::OnSenderViewPkPlayersAUpdate(const FEventContext&, const FViewPkPlayerType& OldValue, const FViewPkPlayerType& NewValue)
{
	if (ValidateUpdatedRows(this, TEXT("sender_view_pk_players_a_update"), OldValue, NewValue))
	{
		Counter->MarkSuccess(TEXT("sender_view_pk_players_a_update"));
	}
}

void UViewPkRuntimeHandler::OnSenderViewPkPlayersADelete(const FEventContext&, const FViewPkPlayerType&)
{
	Counter->MarkFailure(TEXT("sender_view_pk_players_a_update"), TEXT("Unexpected delete for sender_view_pk_players_a"));
	Counter->Abort();
}
