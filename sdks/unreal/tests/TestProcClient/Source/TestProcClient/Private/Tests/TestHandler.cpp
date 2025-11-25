#include "Tests/TestHandler.h"

#include "ModuleBindings/Tables/MyTableTable.g.h"
#include "Tests/TestCounter.h"
#include "Tests/CommonTestFunctions.h"

void UProcedureHandler::OnReturnEnumA(const FProcedureEventContext& Context, const FReturnEnumType& Result, bool bSuccess)
{
	static const FString Name(TEXT("ReturnEnumA"));
	if (bSuccess && Result.IsA() && Result.GetAsA() == 42)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected result"));
	}
}
void UProcedureHandler::OnReturnEnumB(const FProcedureEventContext& Context, const FReturnEnumType& Result, bool bSuccess)
{
	static const FString Name(TEXT("ReturnEnumB"));
	if (bSuccess && Result.IsB() && Result.GetAsB() == TEXT("Hello, SpacetimeDB!"))
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected result"));
	}
}
void UProcedureHandler::OnReturnPrimitive(const FProcedureEventContext& Context, const uint32 Result, bool bSuccess)
{
	static const FString Name(TEXT("ReturnPrimitive"));
	if (bSuccess && Result == 42+27)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected result"));
	}
}
void UProcedureHandler::OnReturnStruct(const FProcedureEventContext& Context, const FReturnStructType& Result, bool bSuccess)
{
	static const FString Name(TEXT("ReturnStruct"));
	if (bSuccess && Result.A == 42 && Result.B == TEXT("Hello, SpacetimeDB!"))
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected result"));
	}
}
void UProcedureHandler::OnWillPanic(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result, bool bSuccess)
{
	static const FString Name(TEXT("WillPanic"));
	if (!bSuccess)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Expected failure did not occur"));
	}
}

void UProcedureHandler::OnInsertWithTxCommitMyTable(const FEventContext& Event, const FMyTableType& NewRow)
{
	static const FString Name(TEXT("InsertWithTxCommitCallback"));
	if (NewRow == ExpectedMyTableRow)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Data did not match"));
	}
}

void UProcedureHandler::OnReturnInsertTxCommit(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result,
	bool bSuccess)
{
	static const FString Name(TEXT("InsertWithTxCommitValues"));
	
	FMyTableType Row = Context.Db->MyTable->Iter()[0];
	if (Row == ExpectedMyTableRow)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Data did not match"));
	}
}

void UProcedureHandler::OnInsertWithTxRollbackMyTable(const FEventContext& Event, const FMyTableType& NewRow)
{
    // This should never be called - if it is, fail immediately
    UE_LOG(LogTemp, Error, TEXT("CRITICAL FAILURE: Row was inserted despite transaction rollback"));
    Counter->Abort();
}

void UProcedureHandler::OnReturnInsertTxRollback(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result,
	bool bSuccess)
{
	static const FString Name(TEXT("InsertWithTxRollbackValues"));
	if (Context.Db->MyTable->Count() == 0)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Received data but shouldn't have"));
	}
}
