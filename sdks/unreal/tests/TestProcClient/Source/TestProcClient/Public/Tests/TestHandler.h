#pragma once
#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "Types/Builtins.h"
#include "Types/LargeIntegers.h"

#include "UmbreallaHeaderTypes.h"
#include "UmbreallaHeaderProcedures.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "ModuleBindings/Types/MyTableType.g.h"

#include "TestHandler.generated.h"

class FTestCounter;

/** Receives table updates and validates their payloads. */
UCLASS()
class UTestHandler : public UObject
{
	GENERATED_BODY()
public:
	TSharedPtr<FTestCounter> Counter;

	/** Stores the initial connection id so we can ensure a reconnect reuses it. */
	FSpacetimeDBConnectionId InitialConnectionId;
};

UCLASS()
class UProcedureHandler : public UTestHandler
{
	GENERATED_BODY()
public:

	//@NOTE: Unreal’s UHT cannot see macros when generating reflection data, so the UFUNCTION()s via FOREACH_PRIMITIVE won't be registered or bindable via Macro :(
/* UFUNCTION declarations for every primitive ---------------------- */
//#define DECLARE_UFUNC(Suffix, Expected, RowStructType)                    \
//    UFUNCTION()                                                           \
//    void OnInsertOne##Suffix(const FEventContext& Context, const RowStructType& Value);
//    FOREACH_PRIMITIVE(DECLARE_UFUNC)
//#undef DECLARE_UFUNC


	UFUNCTION() void OnReturnEnumA(const FProcedureEventContext& Context, const FReturnEnumType& Result, bool bSuccess);
	UFUNCTION() void OnReturnEnumB(const FProcedureEventContext& Context, const FReturnEnumType& Result, bool bSuccess);
	UFUNCTION() void OnReturnPrimitive(const FProcedureEventContext& Context, const uint32 Result, bool bSuccess);
	UFUNCTION() void OnReturnStruct(const FProcedureEventContext& Context, const FReturnStructType& Result, bool bSuccess);
	UFUNCTION() void OnWillPanic(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result, bool bSuccess);


	UFUNCTION() void OnInsertWithTxCommitMyTable(const FEventContext& Event, const FMyTableType& NewRow);
	UFUNCTION() void OnReturnInsertTxCommit(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result, bool bSuccess);

	UFUNCTION() void OnInsertWithTxRollbackMyTable(const FEventContext& Event, const FMyTableType& NewRow);
	UFUNCTION() void OnReturnInsertTxRollback(const FProcedureEventContext& Context, const FSpacetimeDBUnit& Result, bool bSuccess);

	//FSpacetimeDBUnit
	TArray<FString> ExpectedStrings;
	FMyTableType ExpectedMyTableRow;
};
