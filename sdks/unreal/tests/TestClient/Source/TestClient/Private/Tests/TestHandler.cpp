#include "Tests/TestHandler.h"

#include "ModuleBindings/Types/OneUuidType.g.h"
#include "Tests/TestCounter.h"
#include "Tests/CommonTestFunctions.h"

/* Implementation for every primitive ---------------------------------- */
#define DEFINE_UFUNC(Suffix, Expected, RowStructType) \
void UInsertPrimitiveHandler::OnInsertOne##Suffix(const FEventContext& Context, const RowStructType& Value) \
{ \
	static const FString Name(TEXT("InsertOne" #Suffix)); \
	RowStructType ExpectedValue = RowStructType(Expected); \
	(Value == ExpectedValue) ? Counter->MarkSuccess(Name) : Counter->MarkFailure(Name, TEXT("Unexpected value")); \
}

FOREACH_PRIMITIVE(DEFINE_UFUNC)
#undef DEFINE_UFUNC

/* DeletePrimitive handler functions ------------------------------------ */
#define DEFINE_DELETE_UNIQUE(Suffix, Field, Literal, Expected, RowStructType) \
void UDeletePrimitiveHandler::OnInsertUnique##Suffix(const FEventContext& Context, const RowStructType& Value) \
{ \
	static const FString Name(TEXT("InsertUnique" #Suffix)); \
	RowStructType ExpectedValue; \
	ExpectedValue.Field = Literal; \
	ExpectedValue.Data = Expected; \
	if (Value == ExpectedValue) { \
		Counter->MarkSuccess(Name); \
	} else { \
		Counter->MarkFailure(Name, TEXT("Unexpected value")); \
	} \
	Context.Reducers->DeleteUnique##Suffix(Value.Field); \
} \
 \
void UDeletePrimitiveHandler::OnDeleteUnique##Suffix(const FEventContext& Context, const RowStructType& Value) \
{ \
	static const FString Name(TEXT("DeleteUnique" #Suffix)); \
	RowStructType ExpectedValue; \
	ExpectedValue.Field = Literal; \
	ExpectedValue.Data = Expected; \
	(Value == ExpectedValue) ? Counter->MarkSuccess(Name) : Counter->MarkFailure(Name, TEXT("Unexpected value")); \
}

FOREACH_UNIQUE_PRIMITIVE(DEFINE_DELETE_UNIQUE)
#undef DEFINE_DELETE_UNIQUE

/* UpdatePrimitive handler functions ------------------------------------ */
#define DEFINE_UPDATE_PK(Suffix, Field, Literal, Expected, Updated, RowStructType) \
void UUpdatePrimitiveHandler::OnInsertPk##Suffix(const FEventContext& Context, const RowStructType& Value) \
{ \
	static const FString Name(TEXT("InsertPk" #Suffix)); \
	RowStructType ExpectedValue; \
	ExpectedValue.Field = Literal; \
	ExpectedValue.Data = Expected; \
	if (Value == ExpectedValue) { \
		Counter->MarkSuccess(Name); \
	} else { \
		Counter->MarkFailure(Name, TEXT("Unexpected value")); \
	} \
	Context.Reducers->UpdatePk##Suffix(Value.Field, Updated); \
} \
 \
void UUpdatePrimitiveHandler::OnUpdatePk##Suffix(const FEventContext& Context, const RowStructType& OldValue, const RowStructType& NewValue) \
{ \
	static const FString Name(TEXT("UpdatePk" #Suffix)); \
	RowStructType ExpectedOld; ExpectedOld.Field = Literal; ExpectedOld.Data = Expected; \
	RowStructType ExpectedNew; ExpectedNew.Field = Literal; ExpectedNew.Data = Updated; \
	if (OldValue == ExpectedOld && NewValue == ExpectedNew) { \
		Counter->MarkSuccess(Name); \
	} else { \
		Counter->MarkFailure(Name, TEXT("Unexpected value")); \
	} \
	Context.Reducers->DeletePk##Suffix(NewValue.Field); \
} \
 \
void UUpdatePrimitiveHandler::OnDeletePk##Suffix(const FEventContext& Context, const RowStructType& Value) \
{ \
	static const FString Name(TEXT("DeletePk" #Suffix)); \
	RowStructType ExpectedValue; ExpectedValue.Field = Literal; ExpectedValue.Data = Updated; \
	(Value == ExpectedValue) ? Counter->MarkSuccess(Name) : Counter->MarkFailure(Name, TEXT("Unexpected value")); \
}

FOREACH_PK_PRIMITIVE(DEFINE_UPDATE_PK)
#undef DEFINE_UPDATE_PK

void UIdentityActionsHandler::SetExpectedValue(const FSpacetimeDBIdentity& Expected, int32 InsertData, int32 UpdateData)
{
	ExpectedValue = Expected;
	ExpectedInsertData = InsertData;
	ExpectedUpdateData = UpdateData;
}

void UConnectionIdActionsHandler::SetExpectedvalue(const FSpacetimeDBConnectionId& Expected, const int32& Data)
{
	ExpectedValue = Expected;
	ExpectedData = Data;
}

void UIdentityActionsHandler::OnInsertOneIdentity(const FEventContext& Context, const FOneIdentityType& Value)
{
	static const FString Name(TEXT("InsertIdentity"));

	FOneIdentityType Expectedstruct = FOneIdentityType(ExpectedValue);
	if (Value == Expectedstruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UIdentityActionsHandler::OnInsertUniqueIdentity(const FEventContext& Context, const FUniqueIdentityType& Value)
{
	static const FString Name(TEXT("UniqueIdentity_Insert"));

	FUniqueIdentityType Expectedstruct = FUniqueIdentityType(ExpectedValue);
	if (Value == Expectedstruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Reducers->DeleteUniqueIdentity(ExpectedValue);
}

void UIdentityActionsHandler::OnInsertCallerIdentity(const FEventContext& Context, const FOneIdentityType& Value)
{
	static const FString Name(TEXT("InsertCallerIdentity"));

	FSpacetimeDBIdentity Identity;
	if (Context.TryGetIdentity(Identity))
	{
		if (Value.I == Identity)
		{
			Counter->MarkSuccess(Name);
		}
		else
		{
			Counter->MarkFailure(Name, TEXT("Unexpected value"));
		}
	}
	else {
		Counter->MarkFailure(Name, TEXT("Identity not found"));
	}
}

void UIdentityActionsHandler::OnInsertPkIdentity(const FEventContext& Context, const FPkIdentityType& Value)
{
	static const FString Name(TEXT("PkIdentity_Insert"));

	FPkIdentityType ExpectedStruct = FPkIdentityType(ExpectedValue, ExpectedInsertData);
	if (Value == ExpectedStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Reducers->UpdatePkIdentity(ExpectedValue, ExpectedUpdateData);
	Context.Db->PkIdentity->OnInsert.RemoveDynamic(this, &UIdentityActionsHandler::OnInsertPkIdentity);

}

void UIdentityActionsHandler::OnUpdatePkIdentity(const FEventContext& Context, const FPkIdentityType& OldValue, const FPkIdentityType& NewValue)
{
	static const FString Name(TEXT("PkIdentity_Update"));

	FPkIdentityType ExpectedNewStruct = FPkIdentityType(ExpectedValue, ExpectedUpdateData);
	FPkIdentityType ExpectedOldStruct = FPkIdentityType(ExpectedValue, ExpectedInsertData);
	if (OldValue == ExpectedOldStruct &&
		NewValue == ExpectedNewStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Reducers->DeletePkIdentity(ExpectedValue);
	Context.Db->PkIdentity->OnUpdate.RemoveDynamic(this, &UIdentityActionsHandler::OnUpdatePkIdentity);
}

void UIdentityActionsHandler::OnDeletePkIdentity(const FEventContext& Context, const FPkIdentityType& Value)
{
	static const FString Name(TEXT("PkIdentity_Delete"));
	FPkIdentityType ExpectedStruct = FPkIdentityType(ExpectedValue, ExpectedUpdateData);
	if (Value == ExpectedStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Db->PkIdentity->OnDelete.RemoveDynamic(this, &UIdentityActionsHandler::OnDeletePkIdentity);
}

void UIdentityActionsHandler::OnDeleteUniqueIdentity(const FEventContext& Context, const FUniqueIdentityType& Value)
{
	static const FString Name(TEXT("UniqueIdentity_Delete"));

	if (Value.I == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UConnectionIdActionsHandler::OnInsertOneConnectionId(const FEventContext& Context, const FOneConnectionIdType& Value)
{
	static const FString Name(TEXT("InsertConnectionId"));

	if (Value.A == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UConnectionIdActionsHandler::OnInsertPkConnectionId(const FEventContext& Context, const FPkConnectionIdType& Value)
{
	static const FString Name(TEXT("PkConnectionId_Insert"));

	if (Value.A == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	ExpectedData = 2;
	Context.Reducers->UpdatePkConnectionId(ExpectedValue, 2);
}

void UConnectionIdActionsHandler::OnInsertUniqueConnectionId(const FEventContext& Context, const FUniqueConnectionIdType& Value)
{
	static const FString Name(TEXT("InsertUniqueConnectionId"));

	if (Value.Data == ExpectedData)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Reducers->UpdateUniqueConnectionId(Value.A, Value.Data);
}

void UConnectionIdActionsHandler::OnInsertCallerConnectionId(const FEventContext& Context, const FOneConnectionIdType& Value)
{
	static const FString Name(TEXT("InsertCallerConnectionId"));

	if (Value.A == Context.GetConnectionId())
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UConnectionIdActionsHandler::OnDeletePkConnectionId(const FEventContext& Context, const FPkConnectionIdType& Value)
{
	static const FString Name(TEXT("PkConnectionId_Delete"));

	if (Value.A == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UConnectionIdActionsHandler::OnUpdatePkConnectionId(const FEventContext& Context, const FPkConnectionIdType& OldValue, const FPkConnectionIdType& NewValue)
{
	static const FString Name(TEXT("PkConnectionId_Update"));

	if (NewValue.Data == ExpectedData && NewValue.A == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
	Context.Reducers->DeletePkConnectionId(ExpectedValue);
}

void UConnectionIdActionsHandler::OnUpdateUniqueConnectionId(const FEventContext& Context, const FUniqueConnectionIdType& OldValue, const FUniqueConnectionIdType& NewValue)
{
	static const FString Name(TEXT("UpdateUniqueConnectionId"));

	if (NewValue.Data == ExpectedData && NewValue.A == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}

	SetExpectedvalue(NewValue.A, 3);

	// Call the reducer to insert the identity first.
	Context.Reducers->UpdateUniqueConnectionId(NewValue.A, 3);
}

void UTimestampActionsHandler::SetExpectedvalue(const FSpacetimeDBTimestamp& Expected)
{
	ExpectedValue = Expected;
}

void UOnReducerActionsHandler::SetExpectedvalue(const uint8& Expected)
{
	ExpectedValue = Expected;
}

void UOnReducerActionsHandler::SetExpectedKeyAndValue(const uint8& Key, int32 SuccessValue, int32 FailValue)
{
	ExpectedKey = Key;
	ExpectedValue = SuccessValue;
	ExpectedFailValue = FailValue;
	bShouldSucceed = true;
}

void UVectorDataActionsHandler::OnInsertVecU8(const FEventContext& Context, const FVecU8Type& Value)
{
	static const FString Name(TEXT("InsertVecU8"));

	if (ExpectedVecU8 == Value) 
	{
		Counter->MarkSuccess(Name);
	}
	else 
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecU16(const FEventContext& Context, const FVecU16Type& Value)
{
	static const FString Name(TEXT("InsertVecU16"));

	if (ExpectedVecU16 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecU32(const FEventContext& Context, const FVecU32Type& Value)
{
	static const FString Name(TEXT("InsertVecU32"));

	if (ExpectedVecU32 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecU64(const FEventContext& Context, const FVecU64Type& Value)
{
	static const FString Name(TEXT("InsertVecU64"));

	if (ExpectedVecU64 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecU128(const FEventContext& Context, const FVecU128Type& Value)
{
	static const FString Name(TEXT("InsertVecU128"));

	if (ExpectedVecU128 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecU256(const FEventContext& Context, const FVecU256Type& Value)
{
	static const FString Name(TEXT("InsertVecU256"));

	if (ExpectedVecU256 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI8(const FEventContext& Context, const FVecI8Type& Value)
{
	static const FString Name(TEXT("InsertVecI8"));

	if (ExpectedVecI8 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI16(const FEventContext& Context, const FVecI16Type& Value)
{
	static const FString Name(TEXT("InsertVecI16"));

	if (ExpectedVecI16 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI32(const FEventContext& Context, const FVecI32Type& Value)
{
	static const FString Name(TEXT("InsertVecI32"));

	if (ExpectedVecI32 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI64(const FEventContext& Context, const FVecI64Type& Value)
{
	static const FString Name(TEXT("InsertVecI64"));

	if (ExpectedVecI64 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI128(const FEventContext& Context, const FVecI128Type& Value)
{
	static const FString Name(TEXT("InsertVecI128"));

	if (ExpectedVecI128 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecI256(const FEventContext& Context, const FVecI256Type& Value)
{
	static const FString Name(TEXT("InsertVecI256"));

	if (ExpectedVecI256 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecBool(const FEventContext& Context, const FVecBoolType& Value)
{
	static const FString Name(TEXT("InsertVecBool"));

	if (ExpectedVecBool == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecF32(const FEventContext& Context, const FVecF32Type& Value)
{
	static const FString Name(TEXT("InsertVecF32"));

	if (ExpectedVecF32 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecF64(const FEventContext& Context, const FVecF64Type& Value)
{
	static const FString Name(TEXT("InsertVecF64"));

	if (ExpectedVecF64 == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecString(const FEventContext& Context, const FVecStringType& Value)
{
	static const FString Name(TEXT("InsertVecString"));

	if (ExpectedVecString.S == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecIdentity(const FEventContext& Context, const FVecIdentityType& Value)
{
	static const FString Name(TEXT("InsertVecIdentity"));

	if (ExpectedVecIdentity.I == Value.I)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecConnectionId(const FEventContext& Context, const FVecConnectionIdType& Value)
{
	static const FString Name(TEXT("InsertVecConnectionId"));

	if (ExpectedVecConnectionId.A == Value.A)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UVectorDataActionsHandler::OnInsertVecTimestamp(const FEventContext& Context, const FVecTimestampType& Value)
{
	static const FString Name(TEXT("InsertVecTimestamp"));

	if (ExpectedVecTimestamp.T == Value.T)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOnReducerActionsHandler::OnInsertOneU8(const FReducerEventContext& Context, uint8 Value)
{
	static const FString Name(TEXT("OnReducer"));

	// Check 1: Validate the inserted value.
	if (Value != ExpectedValue)
	{
		// Log an error and abort the test.
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}

	// Check 2: Validate identity with caller identity
	FSpacetimeDBIdentity Identity;
	if (Context.TryGetIdentity(Identity)) {
		if (Identity != Context.Event.CallerIdentity)
		{
			Counter->MarkFailure(Name, TEXT("Caller_identity is not equal to my own identity"));
		}
	}
	else {
		Counter->MarkFailure(Name, TEXT("No identity found"));
	}

	// Check 3: Validate connection_id with caller_connection_id
	if (Context.GetConnectionId() != Context.Event.CallerConnectionId)
	{
		Counter->MarkFailure(Name, TEXT("Caller_connection_id is not equal to my own connection_id"));
	}

	// Check 4: Validate status of reducer call
	if (!Context.Event.Status.IsCommitted())
	{
		Counter->MarkFailure(Name, TEXT("Unexpected status."));
	}

	// Check 5: Validate row count in the table
	if (Context.Db->OneU8->Count() != 1)
	{
		Counter->MarkFailure(Name, TEXT("There is more than one row in the table"));
	}

	// All checks passed. The "unwrap" was successful.
	// Mark the test as complete.
	Counter->MarkSuccess(Name);
}

// Within your UOnReducerActionsHandler class
void UOnReducerActionsHandler::OnInsertPkU8(const FReducerEventContext& Context, uint8 Key, int32 Value)
{
	if (bShouldSucceed)
	{
		static const FString Name(TEXT("Reducer-Callback-Success"));

		if (Key != ExpectedKey || Value != ExpectedValue)
		{
			Counter->MarkFailure(Name, TEXT("Unexpected reducer argument"));
		}

		FSpacetimeDBIdentity Identity;
		if (Context.TryGetIdentity(Identity))
		{
			if (Identity != Context.Event.CallerIdentity)
			{
				Counter->MarkFailure(Name, TEXT("Caller_identity is not equal to my own identity"));
			}
		}
		else
		{
			Counter->MarkFailure(Name, TEXT("No identity found"));
		}

		if (Context.GetConnectionId() != Context.Event.CallerConnectionId)
		{
			Counter->MarkFailure(Name, TEXT("Caller_connection_id is not equal to my own connection_id"));
		}

		if (!Context.Event.Status.IsCommitted())
		{
			Counter->MarkFailure(Name, TEXT("Unexpected status."));
		}

		if (Context.Db->PkU8->Count() != 1)
		{
			Counter->MarkFailure(Name, TEXT("Expected one row in the table"));
		}
		else
		{
			FPkU8Type Row = Context.Db->PkU8->Iter()[0];
			if (Row.N != ExpectedKey || Row.Data != ExpectedValue)
			{
				Counter->MarkFailure(Name, TEXT("Unexpected row value"));
			}
		}

		bShouldSucceed = false;
		Context.Reducers->InsertPkU8(ExpectedKey, ExpectedFailValue);
		Counter->MarkSuccess(Name);
	}
	else
	{
		static const FString Name(TEXT("Reducer-Callback-Fail"));

		if (Key != ExpectedKey || Value != ExpectedFailValue)
		{
			Counter->MarkFailure(Name, TEXT("Unexpected reducer argument"));
		}

		FSpacetimeDBIdentity Identity;
		if (Context.TryGetIdentity(Identity))
		{
			if (Identity != Context.Event.CallerIdentity)
			{
				Counter->MarkFailure(Name, TEXT("Caller_identity is not equal to my own identity"));
			}
		}
		else
		{
			Counter->MarkFailure(Name, TEXT("No identity found"));
		}

		if (Context.GetConnectionId() != Context.Event.CallerConnectionId)
		{
			Counter->MarkFailure(Name, TEXT("Caller_connection_id is not equal to my own connection_id"));
		}

		if (!Context.Event.Status.IsFailed())
		{
			Counter->MarkFailure(Name, TEXT("Unexpected status."));
		}

		if (Context.Db->PkU8->Count() != 1)
		{
			Counter->MarkFailure(Name, TEXT("Expected one row in the table"));
		}
		else
		{
			FPkU8Type Row = Context.Db->PkU8->Iter()[0];
			if (Row.N != ExpectedKey || Row.Data != ExpectedValue)
			{
				Counter->MarkFailure(Name, TEXT("Unexpected row value"));
			}
		}

		Counter->MarkSuccess(Name);
	}
}

void UTimestampActionsHandler::OnInsertOneTimestamp(const FEventContext& Context, const FOneTimestampType& Value)
{
	static const FString Name(TEXT("InsertTimestamp"));

	if (Value.T == ExpectedValue)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UTimestampActionsHandler::OnInsertCallTimestamp(const FReducerEventContext& Context)
{
	static const FString Name(TEXT("InsertCallTimestamp"));

	Counter->MarkSuccess(Name);
}

void URowDeduplicationHandler::OnInsertPkU32(const FEventContext& Context, const FPkU32Type& Value)
{
	if (Value.N == 24)
	{
		static const FString Name(TEXT("ins_24"));
		if (bInserted24)
		{
			Counter->MarkFailure(Name, TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInserted24 = true;
		Counter->MarkSuccess(Name);
		Context.Reducers->DeletePkU32(Value.N);
	}
	else if (Value.N == 42)
	{
		static const FString Name(TEXT("ins_42"));
		if (bInserted42)
		{
			Counter->MarkFailure(Name, TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInserted42 = true;
		Counter->MarkSuccess(Name);
		Context.Reducers->UpdatePkU32(Value.N, 0xfeeb);
	}
	else
	{
		Counter->MarkFailure(TEXT("unexpected_insert"), TEXT("unexpected key"));
		Counter->Abort();
	}
}

void URowDeduplicationHandler::OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value)
{
	static const FString Name(TEXT("del_24"));
	if (Value.N != 24 || bDeleted24)
	{
		Counter->MarkFailure(Name, TEXT("unexpected delete"));
		Counter->Abort();
		return;
	}
	bDeleted24 = true;
	Counter->MarkSuccess(Name);
}

void URowDeduplicationHandler::OnUpdatePkU32(const FEventContext& Context, const FPkU32Type& OldValue, const FPkU32Type& NewValue)
{
	static const FString Name(TEXT("upd_42"));
	if (bUpdated42)
	{
		Counter->MarkFailure(Name, TEXT("duplicate update"));
		Counter->Abort();
		return;
	}
	if (OldValue.N == 42 && NewValue.N == 42 && OldValue.Data == 0xbeef && NewValue.Data == 0xfeeb)
	{
		bUpdated42 = true;
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("unexpected value"));
		Counter->Abort();
	}
}

void URowDeduplicationJoinHandler::OnInsertPkU32(const FEventContext& Context, const FPkU32Type& Value)
{
	static const FString Name(TEXT("pk_u32_on_insert"));
	const uint32 KEY = 42;
	const int32 D1 = 50;
	const int32 DU = 0xbeef;
	const int32 D2 = 100;
	if (bPkInsert)
	{
		Counter->MarkFailure(Name, TEXT("duplicate insert"));
		Counter->Abort();
		return;
	}
	if (Value.N == KEY && Value.Data == D1)
	{
		bPkInsert = true;
		Counter->MarkSuccess(Name);
		Context.Reducers->InsertUniqueU32UpdatePkU32(KEY, DU, D2);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("unexpected value"));
		Counter->Abort();
	}
}

void URowDeduplicationJoinHandler::OnUpdatePkU32(const FEventContext& Context, const FPkU32Type& OldValue, const FPkU32Type& NewValue)
{
	static const FString Name(TEXT("pk_u32_on_update"));
	const uint32 KEY = 42;
	const int32 D1 = 50;
	const int32 D2 = 100;
	if (bPkUpdate)
	{
		Counter->MarkFailure(Name, TEXT("duplicate update"));
		Counter->Abort();
		return;
	}
	if (OldValue.N == KEY && NewValue.N == KEY && OldValue.Data == D1 && NewValue.Data == D2)
	{
		bPkUpdate = true;
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("unexpected value"));
		Counter->Abort();
	}
}

void URowDeduplicationJoinHandler::OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value)
{
	Counter->MarkFailure(TEXT("pk_u32_on_delete"), TEXT("unexpected delete"));
	Counter->Abort();
}

void URowDeduplicationJoinHandler::OnInsertUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value)
{
	static const FString Name(TEXT("unique_u32_on_insert"));
	const uint32 KEY = 42;
	const int32 DU = 0xbeef;
	if (bUniqueInsert)
	{
		Counter->MarkFailure(Name, TEXT("duplicate insert"));
		Counter->Abort();
		return;
	}
	if (Value.N == KEY && Value.Data == DU)
	{
		bUniqueInsert = true;
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("unexpected value"));
		Counter->Abort();
	}
}

void URowDeduplicationJoinHandler::OnDeleteUniqueU32(const FEventContext& Context, const FUniqueU32Type& Value)
{
	Counter->MarkFailure(TEXT("unique_u32_on_delete"), TEXT("unexpected delete"));
	Counter->Abort();
}

























/* PkSimpleEnum handler functions -------------------------------------- */
void UPkSimpleEnumHandler::OnInsertPkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& Value) 
{
	if (Value.Data == Data1 && Value.A == A) 
	{
		Counter->MarkSuccess(TEXT("InsertPkSimpleEnum"));
		Context.Reducers->UpdatePkSimpleEnum(A, Data2);
	}
	else {
		Counter->MarkFailure(TEXT("InsertPkSimpleEnum"), TEXT("Unexpected value"));
	}
}
void UPkSimpleEnumHandler::OnUpdatePkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& OldValue,
	const FPkSimpleEnumType& NewValue) 
{
	if (OldValue.Data == Data1 && NewValue.Data == Data2 && OldValue.A == A && NewValue.A == A) 
	{
		Counter->MarkSuccess("UpdatePkPkSimpleEnum");
	}
	else {
		Counter->MarkFailure("UpdatePkPkSimpleEnum", TEXT("Unexpected value"));
	}
}
void UPkSimpleEnumHandler::OnDeletePkSimpleEnum(const FEventContext& Context, const FPkSimpleEnumType& Value) 
{
	Counter->MarkFailure(TEXT("InsertPkSimpleEnum"), TEXT("OnDeletePkSimpleEnum should not be reached"));
	Counter->MarkFailure(TEXT("UpdatePkPkSimpleEnum"), TEXT("OnDeletePkSimpleEnum should not be reached"));
}

/* IndexedSimpleEnum handler functions --------------------------------- */
void UIndexedSimpleEnumHandler::OnInsertIndexedSimpleEnum(const FEventContext& Context, const FIndexedSimpleEnumType& Value)
{
	static const FString Name(TEXT("IndexedSimpleEnum"));
	if (Value.N == A1) 
	{
		Context.Reducers->UpdateIndexedSimpleEnum(A1, A2);
	}
	else if (Value.N == A2) 
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}


/* OverlappingSubscriptions handler functions -------------------------- */
void UOverlappingSubscriptionsHandler::OnInsertPkU8Reducer(const FReducerEventContext& Context, uint8 N, int32 Data)
{
	Counter->MarkSuccess(TEXT("OverlappingSubscriptions_insert_reducer_done"));

	TArray<FString> Queries;
	Queries.Add(TEXT("select * from pk_u8 where n < 100"));
	Queries.Add(TEXT("select * from pk_u8 where n > 0"));
	SubscribeTheseThen( Connection, Queries, [this](FSubscriptionEventContext Ctx) 
	{
		if (Ctx.Db->PkU8->Count() == 1) 
		{
			Counter->MarkSuccess(TEXT("OverlappingSubscriptions_subscribe_with_row_present"));
		}
		else 
		{
			Counter->MarkFailure(TEXT("OverlappingSubscriptions_subscribe_with_row_present"), TEXT("Expected one row"));
		}
		Ctx.Reducers->UpdatePkU8(1, 1);
		Counter->MarkSuccess(TEXT("OverlappingSubscriptions_call_update_reducer"));
	});
}

void UOverlappingSubscriptionsHandler::OnUpdatePkU8(const FEventContext& Context, const FPkU8Type& OldValue, const FPkU8Type& NewValue)
{
	static const FString Name(TEXT("OverlappingSubscriptions_update_row"));
	if (OldValue.N == 1 && OldValue.Data == 0 && NewValue.N == 1 && NewValue.Data == 1 && Context.Db->PkU8->Count() == 1) 
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionI32(const FEventContext& Context, const FOptionI32Type& Value)
{
	static const FString Name(TEXT("InsertOptionI32"));

	if (ExpectedI32Type == Value.N)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionString(const FEventContext& Context, const FOptionStringType& Value)
{
	static const FString Name(TEXT("InsertOptionString"));

	if (ExpectedStringType == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionIdentity(const FEventContext& Context, const FOptionIdentityType& Value)
{
	static const FString Name(TEXT("InsertOptionIdentity"));

	if (ExpectedIdentityType == Value.I)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionSimpleEnum(const FEventContext& Context, const FOptionSimpleEnumType& Value)
{
	static const FString Name(TEXT("InsertOptionSimpleEnum"));

	if (ExpectedEnumType == Value.E)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionPrimitiveStruct(const FEventContext& Context, const FOptionEveryPrimitiveStructType& Value)
{
	static const FString Name(TEXT("InsertOptionEveryPrimitiveStruct"));

	if (ExpectedEveryPrimitiveStructType == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UOptionActionsHandler::OnInsertOptionVecOptionI32(const FEventContext& Context, const FOptionVecOptionI32Type& Value)
{
	static const FString Name(TEXT("InsertOptionVecOptionI32"));

	if (ExpectedVecOptionI32Type == Value.V)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultI32String(const FEventContext& Context, const FResultI32StringType& Value)
{
	static const FString Name(TEXT("InsertResultI32String"));
	if (ExpectedResultI32StringType == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultStringI32(const FEventContext& Context, const FResultStringI32Type& Value)
{
	static const FString Name(TEXT("InsertResultStringI32"));
	if (ExpectedResultStringI32Type == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultIdentityString(const FEventContext& Context, const FResultIdentityStringType& Value)
{
	static const FString Name(TEXT("InsertResultIdentityString"));
	if (ExpectedResultIdentityStringType == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultSimpleEnumI32(const FEventContext& Context, const FResultSimpleEnumI32Type& Value)
{
	static const FString Name(TEXT("InsertResultSimpleEnumI32"));
	if (ExpectedResultSimpleEnumI32Type == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultEveryPrimitiveStructString(const FEventContext& Context, const FResultEveryPrimitiveStructStringType& Value)
{
	static const FString Name(TEXT("InsertResultEveryPrimitiveStructString"));
	if (ExpectedResultEveryPrimitiveStructStringType == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UResultActionsHandler::OnInsertResultVecI32String(const FEventContext& Context, const FResultVecI32StringType& Value)
{
	static const FString Name(TEXT("InsertResultVecI32String"));
	if (ExpectedResultVecI32StringType == Value.R)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertOneUnitStruct(const FEventContext& Context, const FOneUnitStructType& Value)
{
	static const FString Name(TEXT("InsertOneUnitStruct"));

	if (Value.S == FUnitStructType()) {
		Counter->MarkSuccess(Name);
	}
	else 
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertOneByteStruct(const FEventContext& Context, const FOneByteStructType& Value)
{
	static const FString Name(TEXT("InsertOneByteStruct"));

	if (ExpectedByteStruct == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertOneEveryPrimitiveStruct(const FEventContext& Context, const FOneEveryPrimitiveStructType& Value)
{
	static const FString Name(TEXT("InsertOneEveryPrimitiveStruct"));

	if (Value.S == ExpectedEveryPrimitiveStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertOneEveryVecStruct(const FEventContext& Context, const FOneEveryVecStructType& Value)
{
	static const FString Name(TEXT("InsertOneEveryVecStruct"));

	if (Value.S == ExpectedEveryVecStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertVecUnitStruct(const FEventContext& Context, const FVecUnitStructType& Value)
{
	static const FString Name(TEXT("InsertVecUnitStruct"));

	if (Value.S == TArray<FUnitStructType>()) 
	{
		Counter->MarkSuccess(Name);
	}
	else 
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertVecByteStruct(const FEventContext& Context, const FVecByteStructType& Value)
{
	static const FString Name(TEXT("InsertVecByteStruct"));

	if (ExpectedVecByteStruct == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertVecEveryPrimitiveStruct(const FEventContext& Context, const FVecEveryPrimitiveStructType& Value)
{
	static const FString Name(TEXT("InsertVecEveryPrimitiveStruct"));

	if (ExpectedVecPrimitiveStruct == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UStructActionsHandler::OnInsertVecEveryVecStruct(const FEventContext& Context, const FVecEveryVecStructType& Value)
{
	static const FString Name(TEXT("InsertVecEveryVecStruct"));

	if (ExpectedVecEveryVecStruct == Value.S)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UEnumActionsHandler::OnInsertOneSimpleEnum(const FEventContext& Context, const FOneSimpleEnumType& Value)
{
	static const FString Name(TEXT("InsertOneSimpleEnum"));

	if (ExpectedSimpleEnum == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UEnumActionsHandler::OnInsertVecSimpleEnum(const FEventContext& Context, const FVecSimpleEnumType& Value)
{
	static const FString Name(TEXT("InsertVecSimpleEnum"));

	if (ExpectedVecEnum == Value)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UEnumActionsHandler::OnInsertOneEnumWithPayload(const FEventContext& Context, const FOneEnumWithPayloadType& Value)
{
	static const FString Name(TEXT("InsertOneEnumWithPayload"));

	if (FEnumWithPayloadType::U8(0) == Value.E)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UEnumActionsHandler::OnInsertVecEnumWithPayload(const FEventContext& Context, const FVecEnumWithPayloadType& Value)
{
	static const FString Name(TEXT("InsertVecEnumWithPayload"));

	if (ExpectedVecEnumWithPayload.E == Value.E)
	{
		Counter->MarkSuccess(Name);
	}
	else {
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

static void LogArraysSideBySide(const FString& Name, const TArray<FString>& Value, const TArray<FString>& Expected)
{
	int32 Count = FMath::Max(Value.Num(), Expected.Num());
	for (int32 i = 0; i < Count; ++i)
	{
		const FString& ValValue = Value.IsValidIndex(i) ? Value[i] : TEXT("<missing>");
		const FString& ValExpected = Expected.IsValidIndex(i) ? Expected[i] : TEXT("<missing>");
		UE_LOG(LogTemp, Log, TEXT("[%s] Index %d: Value = %s | Expected = %s"),
			*Name, i, *ValValue, *ValExpected);
	}
}

void UInsertPrimitiveHandler::OnInsertPrimitivesAsString(const FEventContext& Context, const FVecStringType& Value)
{
	static const FString Name(TEXT("InsertPrimitivesAsString"));

	// Detailed side-by-side log
	LogArraysSideBySide(Name, Value.S, ExpectedStrings);

	FVecStringType ExpectedStruct = FVecStringType(ExpectedStrings);
	if (Value == ExpectedStruct)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Unexpected value"));
	}
}

void UTestHandler::OnNoOpSucceeds(const FReducerEventContext& Context)
{
	static const FString Name(TEXT("NoOpSucceeds"));

	if (Context.Event.Status.IsCommitted())
	{
		if (Context.Event.Reducer.IsNoOpSucceeds())
		{
			Counter->MarkSuccess(Name);
		}
		else
		{
			Counter->MarkFailure(Name, "Wrong Reducer should be NoOpSucceeds");
		}
	}
	else
	{
		Counter->MarkFailure(Name, "Not committed");
	}
}

void UTestHandler::OnConnectionDone(UDbConnection* Connection)
{
	static const FString Name(TEXT("OnConnect"));
	InitialConnectionId = Connection->GetConnectionId();
	
	Counter->MarkSuccess(Name);
}

void UTestHandler::OnReConnectionDone(UDbConnection* Connection)
{
	static const FString Name(TEXT("OnReconnect"));

	const FSpacetimeDBConnectionId NewId = Connection->GetConnectionId();
	if (InitialConnectionId == NewId)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(
			Name,
			TEXT("Connection ID changed. Stored: ") + InitialConnectionId.ToHex() +
			TEXT(" New: ") + NewId.ToHex());
	}
}

void URLSSubscriptionHandler::OnInsertUser(const FEventContext& Context, const FUsersType& UserType)
{
	static const FString Name(TEXT("RLSSubscription"));

	if (UserType == ExpectedUserType)
	{
		if (MainCounter && MainCounter->Counter.IsValid())
		{
			MainCounter->Counter->MarkSuccess(ExpectedUserType.Name);
		}
	}
	else
	{
		if (MainCounter && MainCounter->Counter.IsValid())
		{
			//One instance wont be equal so we do not mark failure here.
			//MainCounter->Counter->MarkFailure(Name, TEXT("UserName or Identity is not equal!"));
		}
	}
}

void UParameterizedSubscriptionHandler::OnInsertPkIdentity(const FEventContext& Context, const FPkIdentityType& Identity)
{
	const FString TestName = FString::Printf(TEXT("insert_%d"), ExpectedOldData);
	FPkIdentityType ExpectedStruct = FPkIdentityType( ExpectedIdentity, ExpectedOldData );
	if (ExpectedStruct == Identity)
	{
		Counters->Counter->MarkSuccess(TestName);
	}
	else
	{
		Counters->Counter->MarkFailure(TestName, TEXT("Unexpected identity or data"));
	}
}

void UParameterizedSubscriptionHandler::OnUpdatePkIdentity(const FEventContext& Context, const FPkIdentityType& OldIdentity, const FPkIdentityType& NewIdentity)
{

	const FString TestName = FString::Printf(TEXT("update_%d"), ExpectedNewData);
	FPkIdentityType ExpectedOldStruct = FPkIdentityType(ExpectedIdentity, ExpectedOldData);
	FPkIdentityType ExpectedNewStruct = FPkIdentityType(ExpectedIdentity, ExpectedNewData);
	if (ExpectedOldStruct == OldIdentity
		&& ExpectedNewStruct == NewIdentity)
	{
		Counters->Counter->MarkSuccess(TestName);
	}
	else
	{
		Counters->Counter->MarkFailure(TestName, TEXT("Unexpected identity or data"));
	}
}

void UBagSemanticsTestHandler::OnDeletePkU32(const FEventContext& Context, const FPkU32Type& Value)
{
	static const FString Name(TEXT("pk_u32_on_delete"));

	if (Context.Db->BtreeU32->Count() == 0)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, TEXT("Bag semantics not implemented correctly"));
	}
}

/* LhsJoinUpdate handler functions ------------------------------------- */
void ULhsJoinUpdateHandler::OnInsertPkU32(const FReducerEventContext& Context, uint32 N, int32 Data)
{
	static const uint32 KEY1 = 1;
	static const uint32 KEY2 = 2;
	static const int32 DATA0 = 0;

	if (N == KEY1 && Data == DATA0)
	{
		if (bInsert1)
		{
			Counter->MarkFailure(TEXT("on_insert_1"), TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInsert1 = true;
		Counter->MarkSuccess(TEXT("on_insert_1"));
	}
	else if (N == KEY2 && Data == DATA0)
	{
		if (bInsert2)
		{
			Counter->MarkFailure(TEXT("on_insert_2"), TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInsert2 = true;
		Counter->MarkSuccess(TEXT("on_insert_2"));
	}
	else
	{
		Counter->MarkFailure(TEXT("unexpected_insert"), TEXT("unexpected value"));
		Counter->Abort();
	}

	if (!bUpdateRequested && bInsert1 && bInsert2)
	{
		bUpdateRequested = true;
		Context.Reducers->UpdatePkU32(2, 1);
	}
}

void ULhsJoinUpdateHandler::OnUpdatePkU32(const FReducerEventContext& Context, uint32 N, int32 Data)
{
	static const uint32 KEY2 = 2;
	if (!bUpdate1)
	{
		if (N == KEY2 && Data == 1)
		{
			bUpdate1 = true;
			Counter->MarkSuccess(TEXT("on_update_1"));
			Context.Reducers->UpdatePkU32(KEY2, 0);
		}
		else
		{
			Counter->MarkFailure(TEXT("on_update_1"), TEXT("unexpected value"));
			Counter->Abort();
		}
		return;
	}

	if (!bUpdate2)
	{
		if (N == KEY2 && Data == 0)
		{
			bUpdate2 = true;
			Counter->MarkSuccess(TEXT("on_update_2"));
		}
		else
		{
			Counter->MarkFailure(TEXT("on_update_2"), TEXT("unexpected value"));
			Counter->Abort();
		}
		return;
	}

	Counter->MarkFailure(TEXT("on_update_unexpected"), TEXT("duplicate update"));
	Counter->Abort();
}

void ULhsJoinUpdateDisjointQueriesHandler::OnInsertPkU32Reducer(const FReducerEventContext& Context, uint32 N, int32 Data)
{
	if (N == 1 && Data == 0)
	{
		static const FString Name(TEXT("on_insert_1"));
		if (bInserted1)
		{
			Counter->MarkFailure(Name, TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInserted1 = true;
		Counter->MarkSuccess(Name);
	}
	else if (N == 2 && Data == 0)
	{
		static const FString Name(TEXT("on_insert_2"));
		if (bInserted2)
		{
			Counter->MarkFailure(Name, TEXT("duplicate insert"));
			Counter->Abort();
			return;
		}
		bInserted2 = true;
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(TEXT("unexpected_insert"), TEXT("unexpected value"));
		Counter->Abort();
	}

	if (!bUpdateRequested && bInserted1 && bInserted2)
	{
		bUpdateRequested = true;
		Context.Reducers->UpdatePkU32(2, 1);
	}
}

void ULhsJoinUpdateDisjointQueriesHandler::OnUpdatePkU32Reducer(const FReducerEventContext& Context, uint32 N, int32 Data)
{
	if (N == 2 && Data == 1)
	{
		static const FString Name(TEXT("on_update_1"));
		if (bUpdated1)
		{
			Counter->MarkFailure(Name, TEXT("duplicate update"));
			Counter->Abort();
			return;
		}
		bUpdated1 = true;
		Counter->MarkSuccess(Name);
		Context.Reducers->UpdatePkU32(N, 0);
	}
	else if (N == 2 && Data == 0)
	{
		static const FString Name(TEXT("on_update_2"));
		if (bUpdated2)
		{
			Counter->MarkFailure(Name, TEXT("duplicate update"));
			Counter->Abort();
			return;
		}
		bUpdated2 = true;
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(TEXT("unexpected_update"), TEXT("unexpected value"));
		Counter->Abort();
	}
}

void ULargeTableActionHandler::OnInsertLargeTable(const FEventContext& Context, const FLargeTableType& InsertedRow)
{
	static const FString Name(TEXT("InsertLargeTable"));

	if (InsertedRow == ExpectedLargeTable)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, "Unexpected Value!");
	}
	Context.Reducers->DeleteLargeTable(
		ExpectedLargeTable.A,
		ExpectedLargeTable.B,
		ExpectedLargeTable.C,
		ExpectedLargeTable.D,
		ExpectedLargeTable.E,
		ExpectedLargeTable.F,
		ExpectedLargeTable.G,
		ExpectedLargeTable.H,
		ExpectedLargeTable.I,
		ExpectedLargeTable.J,
		ExpectedLargeTable.K,
		ExpectedLargeTable.L,
		ExpectedLargeTable.M,
		ExpectedLargeTable.N,
		ExpectedLargeTable.O,
		ExpectedLargeTable.P,
		ExpectedLargeTable.Q,
		ExpectedLargeTable.R,
		ExpectedLargeTable.S,
		ExpectedLargeTable.T,
		ExpectedLargeTable.U,
		ExpectedLargeTable.V
	);
}

void ULargeTableActionHandler::OnDeleteLargeTable(const FEventContext& Context, const FLargeTableType& DeletedRow)
{
	static const FString Name(TEXT("DeleteLargeTable"));

	if (DeletedRow == ExpectedLargeTable)
	{
		Counter->MarkSuccess(Name);
	}
	else
	{
		Counter->MarkFailure(Name, "Unexpected Value!");
	}
}
void UUuidActionsHandler::OnInsertCallUuidV4(const FEventContext& Context, const FOneUuidType& Value)
{
	if (Value.U.IsValid() && Value.U != FSpacetimeDBUuid::Nil())
	{
		Counter->MarkSuccess(TEXT("InsertCallUuidV4"));
	}
	else
	{
		FString ErrorMessage = FString::Printf(TEXT("Invalid UUID value: %s"), *Value.U.ToString());
		Counter->MarkFailure(TEXT("InsertCallUuidV4"), ErrorMessage);
	}
}

void UUuidActionsHandler::OnInsertCallUuidV7(const FEventContext& Context, const FOneUuidType& Value)
{
	if (Value.U.IsValid() && Value.U != FSpacetimeDBUuid::Nil())
	{
		Counter->MarkSuccess(TEXT("InsertCallUuidV7"));
	}
	else
	{
		FString ErrorMessage = FString::Printf(TEXT("Invalid UUID value: %s"), *Value.U.ToString());
		Counter->MarkFailure(TEXT("InsertCallUuidV7"), ErrorMessage);
	}
}
