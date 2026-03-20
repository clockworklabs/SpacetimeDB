/**
 * BSATN round-trip test-suite (Simple Automation Test)
 */

#include "Tests/SpacetimeDBBSATNTestOrg.h"

#include "Types/LargeIntegers.h"
#include "Types/Builtins.h"
#include "ModuleBindings/Types/BsatnRowListType.g.h"
#include "ModuleBindings/Types/CallProcedureType.g.h"
#include "ModuleBindings/Types/CallReducerType.g.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/EventTableRowsType.g.h"
#include "ModuleBindings/Types/InitialConnectionType.g.h"
#include "ModuleBindings/Types/OneOffQueryType.g.h"
#include "ModuleBindings/Types/OneOffQueryResultType.g.h"
#include "ModuleBindings/Types/PersistentTableRowsType.g.h"
#include "ModuleBindings/Types/ProcedureResultType.g.h"
#include "ModuleBindings/Types/ProcedureStatusType.g.h"
#include "ModuleBindings/Types/QueryRowsType.g.h"
#include "ModuleBindings/Types/QuerySetIdType.g.h"
#include "ModuleBindings/Types/QuerySetUpdateType.g.h"
#include "ModuleBindings/Types/ReducerOkType.g.h"
#include "ModuleBindings/Types/ReducerOutcomeType.g.h"
#include "ModuleBindings/Types/ReducerResultType.g.h"
#include "ModuleBindings/Results/SpacetimeDbSdkResultQueryRowsString.g.h"
#include "ModuleBindings/Types/RowSizeHintType.g.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/SingleTableRowsType.g.h"
#include "ModuleBindings/Types/SubscribeAppliedType.g.h"
#include "ModuleBindings/Types/SubscribeType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "ModuleBindings/Types/TableUpdateType.g.h"
#include "ModuleBindings/Types/TableUpdateRowsType.g.h"
#include "ModuleBindings/Types/TransactionUpdateType.g.h"
#include "ModuleBindings/Types/UnsubscribeAppliedType.g.h"
#include "ModuleBindings/Types/UnsubscribeFlagsType.g.h"
#include "ModuleBindings/Types/UnsubscribeType.g.h"
#include "ModuleBindings/Optionals/SpacetimeDbSdkOptionalQueryRows.g.h"


// ──────────────────────────────────────────────────────────────────────────────
// Simple Automation Test entry-point
// ──────────────────────────────────────────────────────────────────────────────
IMPLEMENT_SIMPLE_AUTOMATION_TEST(
	FBSATNSerializationTest,
	"SpacetimeDB.Serialization.RoundTrip",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)

	bool FBSATNSerializationTest::RunTest(const FString& /*Parameters*/)
{
	// Primitive types
	LOG_Category("Primitive types");
	TEST_ROUNDTRIP(bool, true, "bool true");
	TEST_ROUNDTRIP(bool, false, "bool false");
	TEST_ROUNDTRIP(uint8, 255, "uint8 max");
	TEST_ROUNDTRIP(uint16, 65535, "uint16 max");
	TEST_ROUNDTRIP(uint32, 4294967295u, "uint32 max");
	TEST_ROUNDTRIP(uint64, 18446744073709551615ull, "uint64 max");
	TEST_ROUNDTRIP(int8, -128, "int8 min");
	TEST_ROUNDTRIP(int16, -32768, "int16 min");
	TEST_ROUNDTRIP(int32, -2147483648, "int32 min");
	TEST_ROUNDTRIP(int64, INT64_MIN, "int64 min");
	TEST_ROUNDTRIP(float, 3.14159f, "float π");
	TEST_ROUNDTRIP(double, 2.718281828459045, "double e");

	// Strings & names
	LOG_Category("Strings & names");
	TEST_ROUNDTRIP(FString, FString(""), "FString empty");
	TEST_ROUNDTRIP(FString, FString("Hello, World!"), "FString ascii");
	TEST_ROUNDTRIP(FString, FString("Hello, 世界! 🚀"), "FString unicode");
	TEST_ROUNDTRIP(FString, FString("Line1\nLine2\tTab"), "FString special");
	TEST_ROUNDTRIP(FName, FName("PlayerController"), "FName normal");
	TEST_ROUNDTRIP(FName, FName(""), "FName empty");

	// Large Integers
	LOG_Category("Large Integers");
	TEST_ROUNDTRIP(FSpacetimeDBUInt128, FSpacetimeDBUInt128(0, 0), "u128 zero");
	TEST_ROUNDTRIP(FSpacetimeDBUInt128, FSpacetimeDBUInt128(MAX_uint64, MAX_uint64), "u128 max");
	TEST_ROUNDTRIP(FSpacetimeDBUInt128, FSpacetimeDBUInt128(1234567890, 9876543210), "u128 value");
	TEST_ROUNDTRIP(FSpacetimeDBInt128, FSpacetimeDBInt128(0, 0), "i128 zero");
	TEST_ROUNDTRIP(FSpacetimeDBInt128, FSpacetimeDBInt128(static_cast<uint64>(INT64_MAX), MAX_uint64), "i128 max positive");
	TEST_ROUNDTRIP(FSpacetimeDBInt128, FSpacetimeDBInt128(MAX_uint64, MAX_uint64), "i128 -1");
	TEST_ROUNDTRIP(FSpacetimeDBInt128, FSpacetimeDBInt128(1ULL << 63, 0), "i128 min");
	TEST_ROUNDTRIP(FSpacetimeDBUInt256, FSpacetimeDBUInt256(), "u256 zero");
	TEST_ROUNDTRIP(FSpacetimeDBUInt256, FSpacetimeDBUInt256(FSpacetimeDBUInt128(MAX_uint64, MAX_uint64), FSpacetimeDBUInt128(MAX_uint64, MAX_uint64)), "u256 max");
	TEST_ROUNDTRIP(FSpacetimeDBUInt256, FSpacetimeDBUInt256(FSpacetimeDBUInt128(1, 2), FSpacetimeDBUInt128(3, 4)), "u256 value");
	TEST_ROUNDTRIP(FSpacetimeDBInt256, FSpacetimeDBInt256(), "i256 zero");
	const FSpacetimeDBUInt128 MaxInt256Upper(static_cast<uint64>(INT64_MAX), MAX_uint64);
	const FSpacetimeDBUInt128 MaxInt256Lower(MAX_uint64, MAX_uint64);
	TEST_ROUNDTRIP(FSpacetimeDBInt256, FSpacetimeDBInt256(MaxInt256Upper, MaxInt256Lower), "i256 max positive");
	const FSpacetimeDBUInt128 MinInt256Upper(1ULL << 63, 0);
	const FSpacetimeDBUInt128 MinInt256Lower(0, 0);
	TEST_ROUNDTRIP(FSpacetimeDBInt256, FSpacetimeDBInt256(MinInt256Upper, MinInt256Lower), "i256 min");

	// Spacetime Special types
	LOG_Category("Spacetime Special types");
	TEST_ROUNDTRIP(FSpacetimeDBIdentity, FSpacetimeDBIdentity(FSpacetimeDBUInt256(FSpacetimeDBUInt128(4, 3), FSpacetimeDBUInt128(2, 1))), "Identity");
	TEST_ROUNDTRIP(FSpacetimeDBConnectionId, FSpacetimeDBConnectionId(FSpacetimeDBUInt128(1234567890, 9876543210)), "ConnectionId");
	TEST_ROUNDTRIP(FSpacetimeDBTimestamp, FSpacetimeDBTimestamp(0), "Timestamp zero");
	FSpacetimeDBTimestamp DBTimestamp = FSpacetimeDBTimestamp::FromFDateTime(FDateTime(2025, 6, 23, 15, 2, 24));
	TEST_ROUNDTRIP(FSpacetimeDBTimestamp, DBTimestamp, "Timestamp from FDateTime");
	TEST_ROUNDTRIP(FSpacetimeDBTimeDuration, FSpacetimeDBTimeDuration(0), "TimeDuration zero");
	FSpacetimeDBTimeDuration TimeDuration = FSpacetimeDBTimeDuration(123456789LL);
	TEST_ROUNDTRIP(FSpacetimeDBTimeDuration, TimeDuration, "TimeDuration with microseconds");
	FSpacetimeDBScheduleAt ScheduleAtTimestamp = FSpacetimeDBScheduleAt::Time(DBTimestamp);
	TEST_ROUNDTRIP(FSpacetimeDBScheduleAt, ScheduleAtTimestamp, "ScheduleAt as Timestamp");
	FSpacetimeDBScheduleAt ScheduleAtTimeDuration = FSpacetimeDBScheduleAt::Interval(TimeDuration);
	TEST_ROUNDTRIP(FSpacetimeDBScheduleAt, ScheduleAtTimeDuration, "ScheduleAt as TimeDuration");

	// Containers & optionals
	LOG_Category("Containers & optionals");
	TEST_ROUNDTRIP(TArray<int32>, TArray<int32>{}, "Empty int array");
	TEST_ROUNDTRIP(TArray<int32>, (TArray<int32>{1, 2, 3, 4, 5}), "Int array");
	TEST_ROUNDTRIP(TArray<FString>, (TArray<FString>{"One", "Two", "Three"}), "String array");
	TEST_ROUNDTRIP(FSpacetimeDbSdkOptionalUInt32, FSpacetimeDbSdkOptionalUInt32(100), "Custom Optional<UInt32>");
	TEST_ROUNDTRIP(FSpacetimeDbSdkOptionalUInt32, FSpacetimeDbSdkOptionalUInt32(), "Empty Custom Optional<UInt32>");


	// IDs & time @Note: Not really needed, Guid will not be used and we will be using spacial types for time
	LOG_Category("IDs & time");
	TEST_ROUNDTRIP(FDateTime, FDateTime(), "FDateTime zero");
	TEST_ROUNDTRIP(FDateTime, FDateTime::FromUnixTimestamp(1700000000), "FDateTime");
	TEST_ROUNDTRIP(FTimespan, FTimespan(), "FTimespan zero");
	TEST_ROUNDTRIP(FTimespan, FTimespan::FromMicroseconds(123456789), "FTimespan");

	// Complex struct
	LOG_Category("Complex struct");
	FPlayerData Player;
	Player.PlayerName = "TestPlayer123";
	Player.Level = 42;
	Player.Inventory = { "Sword","Shield","Potion" };
	TEST_ROUNDTRIP(FPlayerData, Player, "FPlayerData");
	FNpc Npc;
	Npc.Type = "SadGoblin";
	TEST_ROUNDTRIP(FNpc, Npc, "FNpc");

	// Edge cases
	LOG_Category("Edge cases");
	{
		TArray<uint32> Large;
		Large.Reserve(1000);
		for (uint32 i = 0; i < 1000; ++i)
		{
			Large.Add(i);
		}
		TEST_ROUNDTRIP(TArray<uint32>, Large, "Large array");

		FString Long;
		for (int32 i = 0; i < 100; ++i)
		{
			Long.Append("Hello World! ");
		}
		TEST_ROUNDTRIP(FString, Long, "Long string");
	}

	//Enum
	LOG_Category("Enum");
	ESpaceTimeDBTestEnum1 TestEnum1 = ESpaceTimeDBTestEnum1::First;
	TEST_ROUNDTRIP(ESpaceTimeDBTestEnum1, TestEnum1, "Enum ESpaceTimeDBTestEnum1");
	ECharacterTypeTag TestEnum2 = ECharacterTypeTag::PlayerData;
	TEST_ROUNDTRIP(ECharacterTypeTag, TestEnum2, "Enum ECharacterTypeTag");

	//Tagged Enum
	LOG_Category("Tagged Enum");
	FCharacterType OrgPlayerChar = FCharacterType::PlayerData(Player);
	TEST_ROUNDTRIP(FCharacterType, OrgPlayerChar, "FCharacterType::Player Tagged Enum");
	FCharacterType OrgNpcChar = FCharacterType::Npc(Npc);
	TEST_ROUNDTRIP(FCharacterType, OrgNpcChar, "FCharacterType::Npc Tagged Enum");
	FCharacterThing ChartactarThingOrg;
	ChartactarThingOrg.Active = true;
	ChartactarThingOrg.Type = OrgNpcChar;
	TEST_ROUNDTRIP(FCharacterThing, ChartactarThingOrg, "FCharacterThing struct with Tagged Enum");

	// Client API (WS v2)
	LOG_Category("Client API WS v2");

	FQuerySetIdType QuerySetId;
	QuerySetId.Id = 100;
	TEST_ROUNDTRIP(FQuerySetIdType, QuerySetId, "FQuerySetIdType");

	FRowSizeHintType FixedSizeHint = FRowSizeHintType::FixedSize(static_cast<uint16>(128));
	TEST_ROUNDTRIP(FRowSizeHintType, FixedSizeHint, "FRowSizeHintType::FixedSize Variant");
	TArray<uint64> RowOffsetsArray;
	FRowSizeHintType RowOffsetsHint = FRowSizeHintType::RowOffsets(RowOffsetsArray);
	TEST_ROUNDTRIP(FRowSizeHintType, RowOffsetsHint, "FRowSizeHintType::RowOffsets Variant");

	FBsatnRowListType BsatnRowsFixed;
	BsatnRowsFixed.SizeHint = FixedSizeHint;
	BsatnRowsFixed.RowsData.Init(0xAB, 10);
	TEST_ROUNDTRIP(FBsatnRowListType, BsatnRowsFixed, "FBsatnRowListType fixed");

	FBsatnRowListType BsatnRowsOffsets;
	BsatnRowsOffsets.SizeHint = RowOffsetsHint;
	BsatnRowsOffsets.RowsData.Init(0xCD, 12);
	TEST_ROUNDTRIP(FBsatnRowListType, BsatnRowsOffsets, "FBsatnRowListType offsets");

	FSingleTableRowsType SingleTableRows;
	SingleTableRows.Table = "PlayerStats";
	SingleTableRows.Rows = BsatnRowsFixed;
	TEST_ROUNDTRIP(FSingleTableRowsType, SingleTableRows, "FSingleTableRowsType");

	FQueryRowsType QueryRows;
	QueryRows.Tables.Add(SingleTableRows);
	TEST_ROUNDTRIP(FQueryRowsType, QueryRows, "FQueryRowsType");

	FCallReducerType CallReducer;
	CallReducer.RequestId = 200;
	CallReducer.Flags = 0;
	CallReducer.Reducer = "MyGameReducer";
	CallReducer.Args.Init(0xDE, 20);
	TEST_ROUNDTRIP(FCallReducerType, CallReducer, "FCallReducerType");

	FCallProcedureType CallProcedure;
	CallProcedure.RequestId = 201;
	CallProcedure.Flags = 0;
	CallProcedure.Procedure = "MyGameProcedure";
	CallProcedure.Args.Init(0xEF, 10);
	TEST_ROUNDTRIP(FCallProcedureType, CallProcedure, "FCallProcedureType");

	FSubscribeType Subscribe;
	Subscribe.RequestId = 300;
	Subscribe.QuerySetId = QuerySetId;
	Subscribe.QueryStrings.Add("SELECT * FROM users WHERE status = 'online'");
	Subscribe.QueryStrings.Add("SELECT item_name FROM inventory WHERE owner_id = 32");
	TEST_ROUNDTRIP(FSubscribeType, Subscribe, "FSubscribeType");

	FOneOffQueryType OneOffQuery;
	OneOffQuery.RequestId = 301;
	OneOffQuery.QueryString = "SELECT * FROM game_settings";
	TEST_ROUNDTRIP(FOneOffQueryType, OneOffQuery, "FOneOffQueryType");

	FUnsubscribeType Unsubscribe;
	Unsubscribe.RequestId = 600;
	Unsubscribe.QuerySetId = QuerySetId;
	Unsubscribe.Flags = EUnsubscribeFlagsType::SendDroppedRows;
	TEST_ROUNDTRIP(FUnsubscribeType, Unsubscribe, "FUnsubscribeType");

	FClientMessageType ClientMessageCallReducer = FClientMessageType::CallReducer(CallReducer);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageCallReducer, "FClientMessageType::CallReducer Variant");
	FClientMessageType ClientMessageCallProcedure = FClientMessageType::CallProcedure(CallProcedure);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageCallProcedure, "FClientMessageType::CallProcedure Variant");
	FClientMessageType ClientMessageSubscribe = FClientMessageType::Subscribe(Subscribe);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageSubscribe, "FClientMessageType::Subscribe Variant");
	FClientMessageType ClientMessageOneOffQuery = FClientMessageType::OneOffQuery(OneOffQuery);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageOneOffQuery, "FClientMessageType::OneOffQuery Variant");
	FClientMessageType ClientMessageUnsubscribe = FClientMessageType::Unsubscribe(Unsubscribe);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageUnsubscribe, "FClientMessageType::Unsubscribe Variant");

	FPersistentTableRowsType PersistentRows;
	PersistentRows.Inserts = BsatnRowsFixed;
	PersistentRows.Deletes = BsatnRowsOffsets;
	TEST_ROUNDTRIP(FPersistentTableRowsType, PersistentRows, "FPersistentTableRowsType");

	FEventTableRowsType EventRows;
	EventRows.Events = BsatnRowsFixed;
	TEST_ROUNDTRIP(FEventTableRowsType, EventRows, "FEventTableRowsType");

	FTableUpdateRowsType PersistentTableUpdateRows = FTableUpdateRowsType::PersistentTable(PersistentRows);
	TEST_ROUNDTRIP(FTableUpdateRowsType, PersistentTableUpdateRows, "FTableUpdateRowsType::PersistentTable");
	FTableUpdateRowsType EventTableUpdateRows = FTableUpdateRowsType::EventTable(EventRows);
	TEST_ROUNDTRIP(FTableUpdateRowsType, EventTableUpdateRows, "FTableUpdateRowsType::EventTable");

	FTableUpdateType TableUpdate;
	TableUpdate.TableName = "PlayerStats";
	TableUpdate.Rows.Add(PersistentTableUpdateRows);
	TableUpdate.Rows.Add(EventTableUpdateRows);
	TEST_ROUNDTRIP(FTableUpdateType, TableUpdate, "FTableUpdateType");

	FQuerySetUpdateType QuerySetUpdate;
	QuerySetUpdate.QuerySetId = QuerySetId;
	QuerySetUpdate.Tables.Add(TableUpdate);
	TEST_ROUNDTRIP(FQuerySetUpdateType, QuerySetUpdate, "FQuerySetUpdateType");

	FTransactionUpdateType TransactionUpdate;
	TransactionUpdate.QuerySets.Add(QuerySetUpdate);
	TEST_ROUNDTRIP(FTransactionUpdateType, TransactionUpdate, "FTransactionUpdateType");

	FSubscribeAppliedType SubscribeApplied;
	SubscribeApplied.RequestId = 12345;
	SubscribeApplied.QuerySetId = QuerySetId;
	SubscribeApplied.Rows = QueryRows;
	TEST_ROUNDTRIP(FSubscribeAppliedType, SubscribeApplied, "FSubscribeAppliedType");

	FUnsubscribeAppliedType UnsubscribeApplied;
	UnsubscribeApplied.RequestId = 3000;
	UnsubscribeApplied.QuerySetId = QuerySetId;
	UnsubscribeApplied.Rows = FSpacetimeDbSdkOptionalQueryRows(QueryRows);
	TEST_ROUNDTRIP(FUnsubscribeAppliedType, UnsubscribeApplied, "FUnsubscribeAppliedType");

	FSubscriptionErrorType SubscriptionError;
	SubscriptionError.RequestId = FSpacetimeDbSdkOptionalUInt32(1001);
	SubscriptionError.QuerySetId = QuerySetId;
	SubscriptionError.Error = "SQL syntax error in subscription query.";
	TEST_ROUNDTRIP(FSubscriptionErrorType, SubscriptionError, "FSubscriptionErrorType");

	FInitialConnectionType InitialConnection;
	InitialConnection.Identity = FSpacetimeDBIdentity(FSpacetimeDBUInt256(FSpacetimeDBUInt128(10, 9), FSpacetimeDBUInt128(8, 7)));
	InitialConnection.Token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
	InitialConnection.ConnectionId = FSpacetimeDBConnectionId(FSpacetimeDBUInt128(12345, 67890));
	TEST_ROUNDTRIP(FInitialConnectionType, InitialConnection, "FInitialConnectionType");

	FSpacetimeDbSdkResultQueryRowsString OneOffResult = FSpacetimeDbSdkResultQueryRowsString::Ok(QueryRows);
	TEST_ROUNDTRIP(FSpacetimeDbSdkResultQueryRowsString, OneOffResult, "FSpacetimeDbSdkResultQueryRowsString::Ok");

	FOneOffQueryResultType OneOffQueryResult;
	OneOffQueryResult.RequestId = 901;
	OneOffQueryResult.Result = OneOffResult;
	TEST_ROUNDTRIP(FOneOffQueryResultType, OneOffQueryResult, "FOneOffQueryResultType");

	FReducerOkType ReducerOk;
	ReducerOk.RetValue.Init(0xAA, 8);
	ReducerOk.TransactionUpdate = TransactionUpdate;
	TEST_ROUNDTRIP(FReducerOkType, ReducerOk, "FReducerOkType");

	FReducerOutcomeType ReducerOutcomeOk = FReducerOutcomeType::Ok(ReducerOk);
	TEST_ROUNDTRIP(FReducerOutcomeType, ReducerOutcomeOk, "FReducerOutcomeType::Ok");
	TArray<uint8> ReducerErrBytes;
	ReducerErrBytes.Add(0x11);
	ReducerErrBytes.Add(0x22);
	FReducerOutcomeType ReducerOutcomeErr = FReducerOutcomeType::Err(ReducerErrBytes);
	TEST_ROUNDTRIP(FReducerOutcomeType, ReducerOutcomeErr, "FReducerOutcomeType::Err");
	FReducerOutcomeType ReducerOutcomeInternal = FReducerOutcomeType::InternalError("Reducer crashed");
	TEST_ROUNDTRIP(FReducerOutcomeType, ReducerOutcomeInternal, "FReducerOutcomeType::InternalError");

	FReducerResultType ReducerResult;
	ReducerResult.RequestId = 777;
	ReducerResult.Timestamp = FSpacetimeDBTimestamp::FromFDateTime(FDateTime(2025, 6, 25, 9, 33, 0));
	ReducerResult.Result = ReducerOutcomeOk;
	TEST_ROUNDTRIP(FReducerResultType, ReducerResult, "FReducerResultType");

	FProcedureStatusType ProcedureStatusReturned = FProcedureStatusType::Returned(TArray<uint8>{0x10, 0x20});
	TEST_ROUNDTRIP(FProcedureStatusType, ProcedureStatusReturned, "FProcedureStatusType::Returned");
	FProcedureStatusType ProcedureStatusInternal = FProcedureStatusType::InternalError("Procedure crashed");
	TEST_ROUNDTRIP(FProcedureStatusType, ProcedureStatusInternal, "FProcedureStatusType::InternalError");

	FProcedureResultType ProcedureResult;
	ProcedureResult.Status = ProcedureStatusReturned;
	ProcedureResult.Timestamp = FSpacetimeDBTimestamp::FromFDateTime(FDateTime(2025, 6, 25, 9, 35, 0));
	ProcedureResult.TotalHostExecutionDuration = FSpacetimeDBTimeDuration(75000);
	ProcedureResult.RequestId = 888;
	TEST_ROUNDTRIP(FProcedureResultType, ProcedureResult, "FProcedureResultType");

	FServerMessageType MessageInitialConnection = FServerMessageType::InitialConnection(InitialConnection);
	TEST_ROUNDTRIP(FServerMessageType, MessageInitialConnection, "FServerMessageType::InitialConnection Variant");
	FServerMessageType MessageTransactionUpdate = FServerMessageType::TransactionUpdate(TransactionUpdate);
	TEST_ROUNDTRIP(FServerMessageType, MessageTransactionUpdate, "FServerMessageType::TransactionUpdate Variant");
	FServerMessageType MessageOneOffQueryResult = FServerMessageType::OneOffQueryResult(OneOffQueryResult);
	TEST_ROUNDTRIP(FServerMessageType, MessageOneOffQueryResult, "FServerMessageType::OneOffQueryResult Variant");
	FServerMessageType MessageSubscribeApplied = FServerMessageType::SubscribeApplied(SubscribeApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageSubscribeApplied, "FServerMessageType::SubscribeApplied Variant");
	FServerMessageType MessageUnsubscribeApplied = FServerMessageType::UnsubscribeApplied(UnsubscribeApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageUnsubscribeApplied, "FServerMessageType::UnsubscribeApplied Variant");
	FServerMessageType MessageSubscriptionError = FServerMessageType::SubscriptionError(SubscriptionError);
	TEST_ROUNDTRIP(FServerMessageType, MessageSubscriptionError, "FServerMessageType::SubscriptionError Variant");
	FServerMessageType MessageReducerResult = FServerMessageType::ReducerResult(ReducerResult);
	TEST_ROUNDTRIP(FServerMessageType, MessageReducerResult, "FServerMessageType::ReducerResult Variant");
	FServerMessageType MessageProcedureResult = FServerMessageType::ProcedureResult(ProcedureResult);
	TEST_ROUNDTRIP(FServerMessageType, MessageProcedureResult, "FServerMessageType::ProcedureResult Variant");

	return true;
}
