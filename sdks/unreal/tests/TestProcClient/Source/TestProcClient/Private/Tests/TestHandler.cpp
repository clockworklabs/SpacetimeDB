#include "Tests/TestHandler.h"
#include "Tests/TestCounter.h"
#include "Tests/CommonTestFunctions.h"

void UProcedureHandler::OnReturnEnumA(const FProcedureEvent& Event, const FReturnEnumType& Result, bool bSuccess)
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
void UProcedureHandler::OnReturnEnumB(const FProcedureEvent& Event, const FReturnEnumType& Result, bool bSuccess)
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
void UProcedureHandler::OnReturnPrimitive(const FProcedureEvent& Event, const uint32 Result, bool bSuccess)
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
void UProcedureHandler::OnReturnStruct(const FProcedureEvent& Event, const FReturnStructType& Result, bool bSuccess)
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
void UProcedureHandler::OnWillPanic(const FProcedureEvent& Event, const FSpacetimeDBUnit& Result, bool bSuccess)
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
