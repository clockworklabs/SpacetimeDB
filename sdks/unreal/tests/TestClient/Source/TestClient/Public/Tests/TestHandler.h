#pragma once
#include "CoreMinimal.h"
#include "UObject/Object.h"

#include "Tests/PrimitiveHandlerList.def"

#include "Types/Builtins.h"
#include "Types/LargeIntegers.h"

#include "UmbreallaHeaderTypes.h"
#include "UmbreallaHeaderaTables.h"

#include "ModuleBindings/SpacetimeDBClient.g.h"

#include "TestHandler.generated.h"

class FTestCounter;

/** Receives table updates and validates their payloads. */
UCLASS()
class UTestHandler : public UObject
{
	GENERATED_BODY()
public:
	TSharedPtr<FTestCounter> Counter;

	UFUNCTION() void OnNoOpSucceeds(const FReducerEventContext& Context);
	UFUNCTION() void OnConnectionDone(UDbConnection* Connection);
	UFUNCTION() void OnReConnectionDone(UDbConnection* Connection);

	/** Stores the initial connection id so we can ensure a reconnect reuses it. */
	FSpacetimeDBConnectionId InitialConnectionId;
};

UCLASS()
class UInsertPrimitiveHandler : public UTestHandler
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


	UFUNCTION() void OnInsertOneU8(const FEventContext& Context, const FOneU8Type& Value);
	UFUNCTION() void OnInsertOneU16(const FEventContext& Context, const FOneU16Type& Value);
	UFUNCTION() void OnInsertOneU32(const FEventContext& Context, const FOneU32Type& Value);
	UFUNCTION() void OnInsertOneU64(const FEventContext& Context, const FOneU64Type& Value);
	UFUNCTION() void OnInsertOneU128(const FEventContext& Context, const FOneU128Type& Value);
	UFUNCTION() void OnInsertOneU256(const FEventContext& Context, const FOneU256Type& Value);
	UFUNCTION() void OnInsertOneI8(const FEventContext& Context, const FOneI8Type& Value);
	UFUNCTION() void OnInsertOneI16(const FEventContext& Context, const FOneI16Type& Value);
	UFUNCTION() void OnInsertOneI32(const FEventContext& Context, const FOneI32Type& Value);
	UFUNCTION() void OnInsertOneI64(const FEventContext& Context, const FOneI64Type& Value);
	UFUNCTION() void OnInsertOneI128(const FEventContext& Context, const FOneI128Type& Value);
	UFUNCTION() void OnInsertOneI256(const FEventContext& Context, const FOneI256Type& Value);
	UFUNCTION() void OnInsertOneBool(const FEventContext& Context, const FOneBoolType& Value);
	UFUNCTION() void OnInsertOneF32(const FEventContext& Context, const FOneF32Type& Value);
	UFUNCTION() void OnInsertOneF64(const FEventContext& Context, const FOneF64Type& Value);
	UFUNCTION() void OnInsertOneString(const FEventContext& Context, const FOneStringType& Value);

	UFUNCTION() void OnInsertPrimitivesAsString(const FEventContext& Context, const FVecStringType& Value);

	TArray<FString> ExpectedStrings;
};

/** Handler used for delete-primitive tests. */
UCLASS()
class UDeletePrimitiveHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void OnInsertUniqueU8(const FEventContext& Context, const FUniqueU8Type& Value);
	UFUNCTION() void OnDeleteUniqueU8(const FEventContext& Context, const FUniqueU8Type& Value);
	UFUNCTION() void OnInsertUniqueU16(const FEventContext& Context, const FUniqueU16Type& Value);
	UFUNCTION() void OnDeleteUniqueU16(const FEventContext& Context, const FUniqueU16Type& Value);
	UFUNCTION() void OnInsertUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value);
	UFUNCTION() void OnDeleteUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value);
	UFUNCTION() void OnInsertUniqueU64(const FEventContext& Context, const FUniqueU64Type& Value);
	UFUNCTION() void OnDeleteUniqueU64(const FEventContext& Context, const FUniqueU64Type& Value);
	UFUNCTION() void OnInsertUniqueU128(const FEventContext& Context, const FUniqueU128Type& Value);
	UFUNCTION() void OnDeleteUniqueU128(const FEventContext& Context, const FUniqueU128Type& Value);
	UFUNCTION() void OnInsertUniqueU256(const FEventContext& Context, const FUniqueU256Type& Value);
	UFUNCTION() void OnDeleteUniqueU256(const FEventContext& Context, const FUniqueU256Type& Value);
	UFUNCTION() void OnInsertUniqueI8(const FEventContext& Context, const FUniqueI8Type& Value);
	UFUNCTION() void OnDeleteUniqueI8(const FEventContext& Context, const FUniqueI8Type& Value);
	UFUNCTION() void OnInsertUniqueI16(const FEventContext& Context, const FUniqueI16Type& Value);
	UFUNCTION() void OnDeleteUniqueI16(const FEventContext& Context, const FUniqueI16Type& Value);
	UFUNCTION() void OnInsertUniqueI32(const FEventContext& Context, const FUniqueI32Type& Value);
	UFUNCTION() void OnDeleteUniqueI32(const FEventContext& Context, const FUniqueI32Type& Value);
	UFUNCTION() void OnInsertUniqueI64(const FEventContext& Context, const FUniqueI64Type& Value);
	UFUNCTION() void OnDeleteUniqueI64(const FEventContext& Context, const FUniqueI64Type& Value);
	UFUNCTION() void OnInsertUniqueI128(const FEventContext& Context, const FUniqueI128Type& Value);
	UFUNCTION() void OnDeleteUniqueI128(const FEventContext& Context, const FUniqueI128Type& Value);
	UFUNCTION() void OnInsertUniqueI256(const FEventContext& Context, const FUniqueI256Type& Value);
	UFUNCTION() void OnDeleteUniqueI256(const FEventContext& Context, const FUniqueI256Type& Value);
	UFUNCTION() void OnInsertUniqueBool(const FEventContext& Context, const FUniqueBoolType& Value);
	UFUNCTION() void OnDeleteUniqueBool(const FEventContext& Context, const FUniqueBoolType& Value);
	UFUNCTION() void OnInsertUniqueString(const FEventContext& Context, const FUniqueStringType& Value);
	UFUNCTION() void OnDeleteUniqueString(const FEventContext& Context, const FUniqueStringType& Value);
};

/** Handler used for update-primitive tests. */
UCLASS()
class UUpdatePrimitiveHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void OnInsertPkU8(const FEventContext& Context, const FPkU8Type& Value);
	UFUNCTION() void OnUpdatePkU8(const FEventContext& Context, const FPkU8Type& OldValue, const FPkU8Type& NewValue);
	UFUNCTION() void OnDeletePkU8(const FEventContext& Context, const FPkU8Type& Value);
	UFUNCTION() void OnInsertPkU16(const FEventContext& Context, const FPkU16Type& Value);
	UFUNCTION() void OnUpdatePkU16(const FEventContext& Context, const FPkU16Type& OldValue, const FPkU16Type& NewValue);
	UFUNCTION() void OnDeletePkU16(const FEventContext& Context, const FPkU16Type& Value);
	UFUNCTION() void OnInsertPkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnUpdatePkU32(const FEventContext& Context, const FPkU32Type& OldValue, const FPkU32Type& NewValue);
	UFUNCTION() void OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnInsertPkU64(const FEventContext& Context, const FPkU64Type& Value);
	UFUNCTION() void OnUpdatePkU64(const FEventContext& Context, const FPkU64Type& OldValue, const FPkU64Type& NewValue);
	UFUNCTION() void OnDeletePkU64(const FEventContext& Context, const FPkU64Type& Value);
	UFUNCTION() void OnInsertPkU128(const FEventContext& Context, const FPkU128Type& Value);
	UFUNCTION() void OnUpdatePkU128(const FEventContext& Context, const FPkU128Type& OldValue, const FPkU128Type& NewValue);
	UFUNCTION() void OnDeletePkU128(const FEventContext& Context, const FPkU128Type& Value);
	UFUNCTION() void OnInsertPkU256(const FEventContext& Context, const FPkU256Type& Value);
	UFUNCTION() void OnUpdatePkU256(const FEventContext& Context, const FPkU256Type& OldValue, const FPkU256Type& NewValue);
	UFUNCTION() void OnDeletePkU256(const FEventContext& Context, const FPkU256Type& Value);
	UFUNCTION() void OnInsertPkI8(const FEventContext& Context, const FPkI8Type& Value);
	UFUNCTION() void OnUpdatePkI8(const FEventContext& Context, const FPkI8Type& OldValue, const FPkI8Type& NewValue);
	UFUNCTION() void OnDeletePkI8(const FEventContext& Context, const FPkI8Type& Value);
	UFUNCTION() void OnInsertPkI16(const FEventContext& Context, const FPkI16Type& Value);
	UFUNCTION() void OnUpdatePkI16(const FEventContext& Context, const FPkI16Type& OldValue, const FPkI16Type& NewValue);
	UFUNCTION() void OnDeletePkI16(const FEventContext& Context, const FPkI16Type& Value);
	UFUNCTION() void OnInsertPkI32(const FEventContext& Context, const FPkI32Type& Value);
	UFUNCTION() void OnUpdatePkI32(const FEventContext& Context, const FPkI32Type& OldValue, const FPkI32Type& NewValue);
	UFUNCTION() void OnDeletePkI32(const FEventContext& Context, const FPkI32Type& Value);
	UFUNCTION() void OnInsertPkI64(const FEventContext& Context, const FPkI64Type& Value);
	UFUNCTION() void OnUpdatePkI64(const FEventContext& Context, const FPkI64Type& OldValue, const FPkI64Type& NewValue);
	UFUNCTION() void OnDeletePkI64(const FEventContext& Context, const FPkI64Type& Value);
	UFUNCTION() void OnInsertPkI128(const FEventContext& Context, const FPkI128Type& Value);
	UFUNCTION() void OnUpdatePkI128(const FEventContext& Context, const FPkI128Type& OldValue, const FPkI128Type& NewValue);
	UFUNCTION() void OnDeletePkI128(const FEventContext& Context, const FPkI128Type& Value);
	UFUNCTION() void OnInsertPkI256(const FEventContext& Context, const FPkI256Type& Value);
	UFUNCTION() void OnUpdatePkI256(const FEventContext& Context, const FPkI256Type& OldValue, const FPkI256Type& NewValue);
	UFUNCTION() void OnDeletePkI256(const FEventContext& Context, const FPkI256Type& Value);
	UFUNCTION() void OnInsertPkBool(const FEventContext& Context, const FPkBoolType& Value);
	UFUNCTION() void OnUpdatePkBool(const FEventContext& Context, const FPkBoolType& OldValue, const FPkBoolType& NewValue);
	UFUNCTION() void OnDeletePkBool(const FEventContext& Context, const FPkBoolType& Value);
	UFUNCTION() void OnInsertPkString(const FEventContext& Context, const FPkStringType& Value);
	UFUNCTION() void OnUpdatePkString(const FEventContext& Context, const FPkStringType& OldValue, const FPkStringType& NewValue);
	UFUNCTION() void OnDeletePkString(const FEventContext& Context, const FPkStringType& Value);
};

// Define a new test handler class for this specific test
UCLASS()
class UBagSemanticsTestHandler : public UTestHandler
{
	GENERATED_BODY()

public:
	// The handler for the "on delete" event from the database for the PkU32 table
	UFUNCTION()
	void OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value);
};

/** Handler used for lhs join update tests. */
UCLASS()
class ULhsJoinUpdateHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	bool bInsert1 = false;
	bool bInsert2 = false;
	bool bUpdateRequested = false;
	bool bUpdate1 = false;
	bool bUpdate2 = false;

	UFUNCTION() void OnInsertPkU32(const FReducerEventContext& Context, uint32 N, int32 Data);
	UFUNCTION() void OnUpdatePkU32(const FReducerEventContext& Context, uint32 N, int32 Data);
};

/** Handler used for lhs join update disjoint queries test. */
UCLASS()
class ULhsJoinUpdateDisjointQueriesHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	bool bInserted1 = false;
	bool bInserted2 = false;
	bool bUpdateRequested = false;
	bool bUpdated1 = false;
	bool bUpdated2 = false;

	UFUNCTION() void OnInsertPkU32Reducer(const FReducerEventContext& Context, uint32 N, int32 Data);
	UFUNCTION() void OnUpdatePkU32Reducer(const FReducerEventContext& Context, uint32 N, int32 Data);
};

// Define a new test handler class for this specific test
UCLASS()
class UParameterizedSubscriptionHandler : public UTestHandler
{
	GENERATED_BODY()

public:
	// The values we expect for this client
	FSpacetimeDBIdentity ExpectedIdentity;
	int32 ExpectedOldData;
	int32 ExpectedNewData;

	UTestHandler* Counters;

	// The handler for the "on insert" event from the database
	UFUNCTION()
	void OnInsertPkIdentity(const FEventContext& Context, const FPkIdentityType& Identity);

	// The handler for the "on update" event from the database
	UFUNCTION()
	void OnUpdatePkIdentity(const FEventContext& Context, const FPkIdentityType& OldIdentity, const FPkIdentityType& NewIdentity);
};

// Define a new test handler class for this specific test
UCLASS()
class URLSSubscriptionHandler : public UTestHandler
{
	GENERATED_BODY()

public:

	FUsersType ExpectedUserType;

	UTestHandler* MainCounter;

	// The handler for the "on insert" event from the database
	UFUNCTION()
	void OnInsertUser(const FEventContext& Context, const FUsersType& UserType);
};

/** Handler used for insert-identity test. */
UCLASS()
class UIdentityActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void SetExpectedValue(const FSpacetimeDBIdentity& Expected, int32 InsertData = 0, int32 UpdateData = 0);
	UFUNCTION() void OnInsertOneIdentity(const FEventContext& Context, const FOneIdentityType& Value);
	UFUNCTION() void OnInsertUniqueIdentity(const FEventContext& Context, const FUniqueIdentityType& Value);
	UFUNCTION() void OnInsertCallerIdentity(const FEventContext& Context, const FOneIdentityType& Value);
	UFUNCTION() void OnInsertPkIdentity(const FEventContext& Context, const FPkIdentityType& Value);
	UFUNCTION() void OnDeletePkIdentity(const FEventContext& Context, const FPkIdentityType& Value);
	UFUNCTION() void OnDeleteUniqueIdentity(const FEventContext& Context, const FUniqueIdentityType& Value);
	UFUNCTION() void OnUpdatePkIdentity(const FEventContext& Context, const FPkIdentityType& OldValue, const FPkIdentityType& NewValue);
private:
	FSpacetimeDBIdentity ExpectedValue;
	int32 ExpectedInsertData = 0;
	int32 ExpectedUpdateData = 0;
};

/** Handler used for insert-identity test. */
UCLASS()
class UConnectionIdActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void SetExpectedvalue(const FSpacetimeDBConnectionId& Expected, const int32& Data);
	UFUNCTION() void OnInsertOneConnectionId(const FEventContext& Context, const FOneConnectionIdType& Value);
	UFUNCTION() void OnInsertPkConnectionId(const FEventContext& Context, const FPkConnectionIdType& Value);
	UFUNCTION() void OnInsertUniqueConnectionId(const FEventContext& Context, const FUniqueConnectionIdType& Value);
	UFUNCTION() void OnInsertCallerConnectionId(const FEventContext& Context, const FOneConnectionIdType& Value);
	UFUNCTION() void OnDeletePkConnectionId(const FEventContext& Context, const FPkConnectionIdType& Value);
	UFUNCTION() void OnUpdatePkConnectionId(const FEventContext& Context, const FPkConnectionIdType& OldValue, const FPkConnectionIdType& NewValue);
	UFUNCTION() void OnUpdateUniqueConnectionId(const FEventContext& Context, const FUniqueConnectionIdType& OldValue, const FUniqueConnectionIdType& NewValue);
private:
	FSpacetimeDBConnectionId ExpectedValue;
	int32 ExpectedData;
};

/** Handler used for insert-identity test. */
UCLASS()
class UTimestampActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void SetExpectedvalue(const FSpacetimeDBTimestamp& Expected);
	UFUNCTION() void OnInsertOneTimestamp(const FEventContext& Context, const FOneTimestampType& Value);
	UFUNCTION() void OnInsertCallTimestamp(const FReducerEventContext& Context);
private:
	FSpacetimeDBTimestamp ExpectedValue;
};

/** Handler used for insert-identity test. */
UCLASS()
class UOnReducerActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	UFUNCTION() void SetExpectedvalue(const uint8& Expected);
	UFUNCTION() void SetExpectedKeyAndValue(const uint8& Key, int32 SuccessValue, int32 FailValue);
	UFUNCTION() void OnInsertOneU8(const FReducerEventContext& Context, uint8 Value);
	UFUNCTION() void OnInsertPkU8(const FReducerEventContext& Context, uint8 Key, int32 Value);
private:
	bool bShouldSucceed;
	uint8 ExpectedKey;
	int32 ExpectedValue;
	int32 ExpectedFailValue;
};

UCLASS()
class UVectorDataActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:

	UFUNCTION() void OnInsertVecU8(const FEventContext& Context, const FVecU8Type& Value);
	UFUNCTION() void OnInsertVecU16(const FEventContext& Context, const FVecU16Type& Value);
	UFUNCTION() void OnInsertVecU32(const FEventContext& Context, const FVecU32Type& Value);
	UFUNCTION() void OnInsertVecU64(const FEventContext& Context, const FVecU64Type& Value);
	UFUNCTION() void OnInsertVecU128(const FEventContext& Context, const FVecU128Type& Value);
	UFUNCTION() void OnInsertVecU256(const FEventContext& Context, const FVecU256Type& Value);

	UFUNCTION() void OnInsertVecI8(const FEventContext& Context, const FVecI8Type& Value);
	UFUNCTION() void OnInsertVecI16(const FEventContext& Context, const FVecI16Type& Value);
	UFUNCTION() void OnInsertVecI32(const FEventContext& Context, const FVecI32Type& Value);
	UFUNCTION() void OnInsertVecI64(const FEventContext& Context, const FVecI64Type& Value);
	UFUNCTION() void OnInsertVecI128(const FEventContext& Context, const FVecI128Type& Value);
	UFUNCTION() void OnInsertVecI256(const FEventContext& Context, const FVecI256Type& Value);

	UFUNCTION() void OnInsertVecBool(const FEventContext& Context, const FVecBoolType& Value);

	UFUNCTION() void OnInsertVecF32(const FEventContext& Context, const FVecF32Type& Value);
	UFUNCTION() void OnInsertVecF64(const FEventContext& Context, const FVecF64Type& Value);

	UFUNCTION() void OnInsertVecString(const FEventContext& Context, const FVecStringType& Value);

	UFUNCTION() void OnInsertVecIdentity(const FEventContext& Context, const FVecIdentityType& Value);
	UFUNCTION() void OnInsertVecConnectionId(const FEventContext& Context, const FVecConnectionIdType& Value);
	UFUNCTION() void OnInsertVecTimestamp(const FEventContext& Context, const FVecTimestampType& Value);

	FVecU8Type ExpectedVecU8;
	FVecU16Type ExpectedVecU16;
	FVecU32Type ExpectedVecU32;
	FVecU64Type ExpectedVecU64;
	FVecU128Type ExpectedVecU128;
	FVecU256Type ExpectedVecU256;

	FVecI8Type ExpectedVecI8;
	FVecI16Type ExpectedVecI16;
	FVecI32Type ExpectedVecI32;
	FVecI64Type ExpectedVecI64;
	FVecI128Type ExpectedVecI128;
	FVecI256Type ExpectedVecI256;

	FVecBoolType ExpectedVecBool;

	FVecF32Type ExpectedVecF32;
	FVecF64Type ExpectedVecF64;

	FVecStringType ExpectedVecString;

	FVecIdentityType ExpectedVecIdentity;
	FVecConnectionIdType ExpectedVecConnectionId;
	FVecTimestampType ExpectedVecTimestamp;
};

UCLASS()
class UOptionActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:

	UFUNCTION() void OnInsertOptionI32(const FEventContext& Context, const FOptionI32Type& Value);
	UFUNCTION() void OnInsertOptionString(const FEventContext& Context, const FOptionStringType& Value);
	UFUNCTION() void OnInsertOptionIdentity(const FEventContext& Context, const FOptionIdentityType& Value);
	UFUNCTION() void OnInsertOptionSimpleEnum(const FEventContext& Context, const FOptionSimpleEnumType& Value);
	UFUNCTION() void OnInsertOptionPrimitiveStruct(const FEventContext& Context, const FOptionEveryPrimitiveStructType& Value);
	UFUNCTION() void OnInsertOptionVecOptionI32(const FEventContext& Context, const FOptionVecOptionI32Type& Value);

	FTestClientOptionalInt32 ExpectedI32Type;
	FTestClientOptionalString ExpectedStringType;
	FTestClientOptionalIdentity ExpectedIdentityType;
	FTestClientOptionalSimpleEnum ExpectedEnumType;
	FTestClientOptionalEveryPrimitiveStruct ExpectedEveryPrimitiveStructType;
	FTestClientOptionalVecOptionalInt32 ExpectedVecOptionI32Type;
};

UCLASS()
class UStructActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:

	UFUNCTION() void OnInsertOneUnitStruct(const FEventContext& Context, const FOneUnitStructType& Value);
	UFUNCTION() void OnInsertOneByteStruct(const FEventContext& Context, const FOneByteStructType& Value);
	UFUNCTION() void OnInsertOneEveryPrimitiveStruct(const FEventContext& Context, const FOneEveryPrimitiveStructType& Value);
	UFUNCTION() void OnInsertOneEveryVecStruct(const FEventContext& Context, const FOneEveryVecStructType& Value);

	UFUNCTION() void OnInsertVecUnitStruct(const FEventContext& Context, const FVecUnitStructType& Value);
	UFUNCTION() void OnInsertVecByteStruct(const FEventContext& Context, const FVecByteStructType& Value);
	UFUNCTION() void OnInsertVecEveryPrimitiveStruct(const FEventContext& Context, const FVecEveryPrimitiveStructType& Value);
	UFUNCTION() void OnInsertVecEveryVecStruct(const FEventContext& Context, const FVecEveryVecStructType& Value);

	FByteStructType ExpectedByteStruct;
	FEveryPrimitiveStructType ExpectedEveryPrimitiveStruct;
	FEveryVecStructType ExpectedEveryVecStruct;
	TArray<FByteStructType> ExpectedVecByteStruct;
	TArray<FEveryPrimitiveStructType> ExpectedVecPrimitiveStruct;
	TArray<FEveryVecStructType> ExpectedVecEveryVecStruct;
};

UCLASS()
class UEnumActionsHandler : public UTestHandler
{
	GENERATED_BODY()
public:

	UFUNCTION() void OnInsertOneSimpleEnum(const FEventContext& Context, const FOneSimpleEnumType& Value);
	UFUNCTION() void OnInsertVecSimpleEnum(const FEventContext& Context, const FVecSimpleEnumType& Value);

	UFUNCTION() void OnInsertOneEnumWithPayload(const FEventContext& Context, const FOneEnumWithPayloadType& Value);
	UFUNCTION() void OnInsertVecEnumWithPayload(const FEventContext& Context, const FVecEnumWithPayloadType& Value);

	FOneSimpleEnumType ExpectedSimpleEnum;
	FVecSimpleEnumType ExpectedVecEnum;

	FVecEnumWithPayloadType ExpectedVecEnumWithPayload;
};

UCLASS()
class ULargeTableActionHandler : public UTestHandler
{
	GENERATED_BODY()

public:

	UFUNCTION() void OnInsertLargeTable(const FEventContext& Context, const FLargeTableType& InsertedRow);
	UFUNCTION() void OnDeleteLargeTable(const FEventContext& Context, const FLargeTableType& DeletedRow);

	FLargeTableType ExpectedLargeTable;
 };

/** Handler used for row deduplication tests. */
UCLASS()
class URowDeduplicationHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	bool bInserted24 = false;
	bool bInserted42 = false;
	bool bDeleted24 = false;
	bool bUpdated42 = false;

	UFUNCTION() void OnInsertPkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnUpdatePkU32(const FEventContext& Context, const FPkU32Type& OldValue, const FPkU32Type& NewValue);
};

/** Handler used for row deduplication join tests. */
UCLASS()
class URowDeduplicationJoinHandler : public UTestHandler
{
	GENERATED_BODY()
public:
	bool bPkInsert = false;
	bool bPkUpdate = false;
	bool bUniqueInsert = false;

	UFUNCTION() void OnInsertPkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnUpdatePkU32(const FEventContext& Context, const FPkU32Type& OldValue, const FPkU32Type& NewValue);
	UFUNCTION() void OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value);
	UFUNCTION() void OnInsertUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value);
	UFUNCTION() void OnDeleteUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value);
};











/** Handler used for pk-simple-enum test. */
UCLASS()
class UPkSimpleEnumHandler : public UTestHandler {
	GENERATED_BODY()
public:
	int32 Data1;
	int32 Data2;
	ESimpleEnumType A;

	UFUNCTION()
	void OnInsertPkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& Value);
	UFUNCTION()
	void OnUpdatePkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& OldValue, const FPkSimpleEnumType& NewValue);
	UFUNCTION()
	void OnDeletePkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& Value);
};

/** Handler used for indexed-simple-enum test. */
UCLASS()
class UIndexedSimpleEnumHandler : public UTestHandler {
	GENERATED_BODY()
public:
	ESimpleEnumType A1;
	ESimpleEnumType A2;

	UFUNCTION()
	void OnInsertIndexedSimpleEnum(const FEventContext& Context, const FIndexedSimpleEnumType& Value);
};

/** Handler used for overlapping-subscriptions test. */
UCLASS()
class UOverlappingSubscriptionsHandler : public UTestHandler {
	GENERATED_BODY()
public:
	UDbConnection* Connection;

	UFUNCTION()
	void OnInsertPkU8Reducer(const FReducerEventContext& Context, uint8 N, int32 Data);
	UFUNCTION()
	void OnUpdatePkU8(const FEventContext& Context, const FPkU8Type& OldValue, const FPkU8Type& NewValue);
};