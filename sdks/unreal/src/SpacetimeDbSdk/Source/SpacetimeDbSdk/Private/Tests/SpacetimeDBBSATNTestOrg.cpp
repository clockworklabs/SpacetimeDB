/**
 * BSATN round-trip test-suite (Simple Automation Test)
 */

#include "Tests/SpacetimeDBBSATNTestOrg.h"

#include "Types/LargeIntegers.h"
#include "Types/Builtins.h"
#include "ModuleBindings/Types/BsatnRowListType.g.h"
#include "ModuleBindings/Types/CallReducerType.g.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/CompressableQueryUpdateType.g.h"
#include "ModuleBindings/Types/DatabaseUpdateType.g.h"
#include "ModuleBindings/Types/EnergyQuantaType.g.h"
#include "ModuleBindings/Types/IdentityTokenType.g.h"
#include "ModuleBindings/Types/InitialSubscriptionType.g.h"
#include "ModuleBindings/Types/OneOffQueryResponseType.g.h"
#include "ModuleBindings/Types/OneOffQueryType.g.h"
#include "ModuleBindings/Types/OneOffTableType.g.h"
#include "ModuleBindings/Types/QueryIdType.g.h"
#include "ModuleBindings/Types/QueryUpdateType.g.h"
#include "ModuleBindings/Types/ReducerCallInfoType.g.h"
#include "ModuleBindings/Types/RowSizeHintType.g.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/SubscribeAppliedType.g.h"
#include "ModuleBindings/Types/SubscribeMultiAppliedType.g.h"
#include "ModuleBindings/Types/SubscribeMultiType.g.h"
#include "ModuleBindings/Types/SubscribeRowsType.g.h"
#include "ModuleBindings/Types/SubscribeSingleType.g.h"
#include "ModuleBindings/Types/SubscribeType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "ModuleBindings/Types/TableUpdateType.g.h"
#include "ModuleBindings/Types/TransactionUpdateLightType.g.h"
#include "ModuleBindings/Types/TransactionUpdateType.g.h"
#include "ModuleBindings/Types/UnsubscribeAppliedType.g.h"
#include "ModuleBindings/Types/UnsubscribeMultiAppliedType.g.h"
#include "ModuleBindings/Types/UnsubscribeMultiType.g.h"
#include "ModuleBindings/Types/UnsubscribeType.g.h"
#include "ModuleBindings/Types/UpdateStatusType.g.h"


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

	//Cliant API
	LOG_Category("Cliant API");
	//QueryIdType
	FQueryIdType QueryId;
	QueryId.Id = 100;
	TEST_ROUNDTRIP(FQueryIdType, QueryId, "FQueryId");

	// SubscribeMultiType
	FSubscribeMultiType SubscribeMulti;
	SubscribeMulti.QueryStrings.Add("SELECT * FROM players");
	SubscribeMulti.QueryStrings.Add("SELECT * FROM guilds WHERE region = 'EU'");
	SubscribeMulti.RequestId = 500;
	SubscribeMulti.QueryId = QueryId;
	TEST_ROUNDTRIP(FSubscribeMultiType, SubscribeMulti, "FSubscribeMultiType");

	// RowSizeHintType
	FRowSizeHintType FixedSizeHint = FRowSizeHintType::FixedSize(static_cast<uint16>(128));
	TEST_ROUNDTRIP(FRowSizeHintType, FixedSizeHint, "FRowSizeHintType::FixedSize Variant");
	TArray<uint64> RowOffsetsArray; // keep empty like before (or add offsets if you want)
	FRowSizeHintType RowOffsetsHint = FRowSizeHintType::RowOffsets(RowOffsetsArray);
	TEST_ROUNDTRIP(FRowSizeHintType, RowOffsetsHint, "FRowSizeHintType::RowOffsets Variant");

	// BsatnRowListType
	FBsatnRowListType BsatnRowList;
	BsatnRowList.SizeHint = FixedSizeHint;
	BsatnRowList.RowsData.Init(0xAB, 10);
	TEST_ROUNDTRIP(FBsatnRowListType, BsatnRowList, "FBsatnRowListType with FixedSize hint");

	// CallReducerType
	FCallReducerType CallReducer;
	CallReducer.Reducer = "MyGameReducer";
	CallReducer.Args.Init(0xDE, 20);
	CallReducer.RequestId = 200;
	CallReducer.Flags = 0; 
	TEST_ROUNDTRIP(FCallReducerType, CallReducer, "FCallReducerType");

	// SubscribeType
	FSubscribeType Subscribe;
	Subscribe.QueryStrings.Add("SELECT * FROM users WHERE status = 'online'");
	Subscribe.QueryStrings.Add("SELECT item_name FROM inventory WHERE owner_id = 32");
	Subscribe.RequestId = 300;
	TEST_ROUNDTRIP(FSubscribeType, Subscribe, "FSubscribeType");

	// OneOffQueryType
	FOneOffQueryType OneOffQuery;
	OneOffQuery.MessageId.Init(0xCC, 16);
	OneOffQuery.QueryString = "SELECT * FROM game_settings";
	TEST_ROUNDTRIP(FOneOffQueryType, OneOffQuery, "FOneOffQueryType");

	// SubscribeSingleType
	FSubscribeSingleType SubscribeSingle;
	SubscribeSingle.Query = "SELECT * FROM player_data WHERE player_id = 33";
	SubscribeSingle.RequestId = 400;
	SubscribeSingle.QueryId = QueryId;
	TEST_ROUNDTRIP(FSubscribeSingleType, SubscribeSingle, "FSubscribeSingleType");

	// UnsubscribeType
	FUnsubscribeType Unsubscribe;
	Unsubscribe.RequestId = 600;
	Unsubscribe.QueryId = QueryId;
	TEST_ROUNDTRIP(FUnsubscribeType, Unsubscribe, "FUnsubscribeType");

	// UnsubscribeMultiType
	FUnsubscribeMultiType UnsubscribeMulti;
	UnsubscribeMulti.RequestId = 700;
	UnsubscribeMulti.QueryId = QueryId;
	TEST_ROUNDTRIP(FUnsubscribeMultiType, UnsubscribeMulti, "FUnsubscribeMultiType");

	// CallReducer variant
	FClientMessageType ClientMessageCallReducer = FClientMessageType::CallReducer(CallReducer);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageCallReducer, "FClientMessageType::CallReducer Variant");
	FClientMessageType ClientMessageSubscribe = FClientMessageType::Subscribe(Subscribe);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageSubscribe, "FClientMessageType::Subscribe Variant");
	FClientMessageType ClientMessageOneOffQuery = FClientMessageType::OneOffQuery(OneOffQuery);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageOneOffQuery, "FClientMessageType::OneOffQuery Variant");
	FClientMessageType ClientMessageSubscribeSingle = FClientMessageType::SubscribeSingle(SubscribeSingle);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageSubscribeSingle, "FClientMessageType::SubscribeSingle Variant");
	FClientMessageType ClientMessageSubscribeMulti = FClientMessageType::SubscribeMulti(SubscribeMulti);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageSubscribeMulti, "FClientMessageType::SubscribeMulti Variant");
	FClientMessageType ClientMessageUnsubscribe = FClientMessageType::Unsubscribe(Unsubscribe);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageUnsubscribe, "FClientMessageType::Unsubscribe Variant");
	FClientMessageType ClientMessageUnsubscribeMulti = FClientMessageType::UnsubscribeMulti(UnsubscribeMulti);
	TEST_ROUNDTRIP(FClientMessageType, ClientMessageUnsubscribeMulti, "FClientMessageType::UnsubscribeMulti Variant");


	// BsatnRowListType
	FBsatnRowListType BsatnRowList1;
	BsatnRowList1.SizeHint = FixedSizeHint;
	BsatnRowList1.RowsData.Init(0xAB, 10);
	TEST_ROUNDTRIP(FBsatnRowListType, BsatnRowList1, "FBsatnRowListType with FixedSize hint");
	FBsatnRowListType BsatnRowList2;
	BsatnRowList2.SizeHint = RowOffsetsHint;
	BsatnRowList2.RowsData.Init(0xAB, 10);
	TEST_ROUNDTRIP(FBsatnRowListType, BsatnRowList2, "FBsatnRowListType with RowOffsets hint");

	// QueryUpdateType
	FQueryUpdateType QueryUpdate;
	QueryUpdate.Deletes = BsatnRowList1;
	QueryUpdate.Inserts = BsatnRowList2;
	TEST_ROUNDTRIP(FQueryUpdateType, QueryUpdate, "FQueryUpdateType");

	// CompressableQueryUpdateType
	FCompressableQueryUpdateType UncompressedUpdate =FCompressableQueryUpdateType::Uncompressed(QueryUpdate);
	TEST_ROUNDTRIP(FCompressableQueryUpdateType, UncompressedUpdate, "FCompressableQueryUpdateType::Uncompressed Variant");
	TArray<uint8> BrotliData;
	BrotliData.Add(0x11);
	BrotliData.Add(0x22);
	FCompressableQueryUpdateType BrotliUpdate =FCompressableQueryUpdateType::Brotli(BrotliData);
	TEST_ROUNDTRIP(FCompressableQueryUpdateType, BrotliUpdate, "FCompressableQueryUpdateType::Brotli Variant");
	TArray<uint8> GzipData;
	GzipData.Add(0xA1);
	GzipData.Add(0xB2);
	FCompressableQueryUpdateType GzipUpdate = FCompressableQueryUpdateType::Gzip(GzipData);
	TEST_ROUNDTRIP(FCompressableQueryUpdateType, GzipUpdate, "FCompressableQueryUpdateType::Gzip Variant");

	// TableUpdateType
	FTableUpdateType TableUpdate;
	TableUpdate.TableId = 1;
	TableUpdate.TableName = "PlayerStats";
	TableUpdate.NumRows = 100;
	TableUpdate.Updates.Add(UncompressedUpdate);
	TableUpdate.Updates.Add(BrotliUpdate);
	TableUpdate.Updates.Add(GzipUpdate);
	TEST_ROUNDTRIP(FTableUpdateType, TableUpdate, "FTableUpdateType");

	// DatabaseUpdateType
	FDatabaseUpdateType DatabaseUpdate;
	DatabaseUpdate.Tables.Add(TableUpdate);
	TEST_ROUNDTRIP(FDatabaseUpdateType, DatabaseUpdate, "FDatabaseUpdateType");

	// EnergyQuantaType
	FEnergyQuantaType EnergyQuanta;
	EnergyQuanta.Quanta = FSpacetimeDBUInt128(1000, 500);
	TEST_ROUNDTRIP(FEnergyQuantaType, EnergyQuanta, "FEnergyQuantaType");

	// IdentityTokenType
	FIdentityTokenType IdentityToken;
	IdentityToken.Identity = FSpacetimeDBIdentity(FSpacetimeDBUInt256(FSpacetimeDBUInt128(10, 9), FSpacetimeDBUInt128(8, 7)));
	IdentityToken.Token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
	IdentityToken.ConnectionId = FSpacetimeDBConnectionId(FSpacetimeDBUInt128(12345, 67890));
	TEST_ROUNDTRIP(FIdentityTokenType, IdentityToken, "FIdentityTokenType");

	// InitialSubscriptionType
	FInitialSubscriptionType InitialSubscription;
	InitialSubscription.DatabaseUpdate = DatabaseUpdate;
	InitialSubscription.RequestId = 101;
	InitialSubscription.TotalHostExecutionDuration = FSpacetimeDBTimeDuration(500000); 
	TEST_ROUNDTRIP(FInitialSubscriptionType, InitialSubscription, "FInitialSubscriptionType");


	// OneOffTableType
	FOneOffTableType OneOffTable;
	OneOffTable.TableName = "GameScores";
	OneOffTable.Rows = BsatnRowList1;
	TEST_ROUNDTRIP(FOneOffTableType, OneOffTable, "FOneOffTableType");


	// OneOffQueryResponseType
	FOneOffQueryResponseType OneOffQueryResponse;
	OneOffQueryResponse.MessageId.Init(0xDD, 16);
	FSpacetimeDbSdkOptionalString SdkOptionalStringError;
	SdkOptionalStringError.bHasValue = true;
	SdkOptionalStringError.Value = "Error text";
	OneOffQueryResponse.Tables.Add(OneOffTable);
	OneOffQueryResponse.TotalHostExecutionDuration = FSpacetimeDBTimeDuration(123456);
	TEST_ROUNDTRIP(FOneOffQueryResponseType, OneOffQueryResponse, "FOneOffQueryResponseType");

	// ReducerCallInfoType
	FReducerCallInfoType ReducerCallInfo;
	ReducerCallInfo.ReducerName = "UpdatePlayerScore";
	ReducerCallInfo.ReducerId = 123;
	ReducerCallInfo.Args.Init(0xAB, 10);
	ReducerCallInfo.RequestId = 789;
	TEST_ROUNDTRIP(FReducerCallInfoType, ReducerCallInfo, "FReducerCallInfoType");

	// UpdateStatusType
	FUpdateStatusType StatusCommitted = FUpdateStatusType::Committed(DatabaseUpdate);
	TEST_ROUNDTRIP(FUpdateStatusType, StatusCommitted, "FUpdateStatusType::Committed Variant");
	FString FailedMsg = TEXT("Reducer execution failed due to invalid input.");
	FUpdateStatusType StatusFailed = FUpdateStatusType::Failed(FailedMsg);
	TEST_ROUNDTRIP(FUpdateStatusType, StatusFailed, "FUpdateStatusType::Failed Variant");
	FSpacetimeDBUnit UnitValue{};
	FUpdateStatusType StatusOutOfEnergy = FUpdateStatusType::OutOfEnergy(UnitValue);
	TEST_ROUNDTRIP(FUpdateStatusType, StatusOutOfEnergy, "FUpdateStatusType::OutOfEnergy Variant");

	// TransactionUpdateType
	FTransactionUpdateType TransactionUpdate;
	TransactionUpdate.Status = StatusCommitted;
	TransactionUpdate.Timestamp = FSpacetimeDBTimestamp::FromFDateTime(FDateTime(2025, 6, 25, 9, 33, 0));
	TransactionUpdate.CallerIdentity = FSpacetimeDBIdentity(FSpacetimeDBUInt256(FSpacetimeDBUInt128(1, 2), FSpacetimeDBUInt128(3, 4)));
	TransactionUpdate.CallerConnectionId = FSpacetimeDBConnectionId(FSpacetimeDBUInt128(98765, 43210));
	TransactionUpdate.ReducerCall = ReducerCallInfo;
	TransactionUpdate.EnergyQuantaUsed = EnergyQuanta;
	TransactionUpdate.TotalHostExecutionDuration = FSpacetimeDBTimeDuration(75000);
	TEST_ROUNDTRIP(FTransactionUpdateType, TransactionUpdate, "FTransactionUpdateType");

	// SubscribeRowsType
	FSubscribeRowsType SubscribeRows;
	SubscribeRows.TableId = 10;
	SubscribeRows.TableName = "ConfigData";
	SubscribeRows.TableRows = TableUpdate;
	TEST_ROUNDTRIP(FSubscribeRowsType, SubscribeRows, "FSubscribeRowsType");

	// SubscribeAppliedType
	FSubscribeAppliedType SubscribeApplied;
	SubscribeApplied.RequestId = 12345;
	SubscribeApplied.TotalHostExecutionDurationMicros = 250000;
	SubscribeApplied.QueryId = QueryId;
	SubscribeApplied.Rows = SubscribeRows;
	TEST_ROUNDTRIP(FSubscribeAppliedType, SubscribeApplied, "FSubscribeAppliedType");

	// SubscribeMultiAppliedType
	FSubscribeMultiAppliedType SubscribeMultiApplied;
	SubscribeMultiApplied.RequestId = 54321;
	SubscribeMultiApplied.TotalHostExecutionDurationMicros = 300000; 
	SubscribeMultiApplied.QueryId = QueryId;
	SubscribeMultiApplied.Update = DatabaseUpdate;
	TEST_ROUNDTRIP(FSubscribeMultiAppliedType, SubscribeMultiApplied, "FSubscribeMultiAppliedType");

	// SubscriptionErrorType
	FSubscriptionErrorType SubscriptionError;
	SubscriptionError.TotalHostExecutionDurationMicros = 50000; 
	SubscriptionError.RequestId = FSpacetimeDbSdkOptionalUInt32(1001);
	SubscriptionError.QueryId = FSpacetimeDbSdkOptionalUInt32(201);
	SubscriptionError.TableId = FSpacetimeDbSdkOptionalUInt32(301);
	SubscriptionError.Error = "SQL syntax error in subscription query.";
	TEST_ROUNDTRIP(FSubscriptionErrorType, SubscriptionError, "FSubscriptionErrorType");

	// TransactionUpdateLightType
	FTransactionUpdateLightType TransactionUpdateLight;
	TransactionUpdateLight.RequestId = 2000;
	TransactionUpdateLight.Update = DatabaseUpdate;
	TEST_ROUNDTRIP(FTransactionUpdateLightType, TransactionUpdateLight, "FTransactionUpdateLightType");


	// UnsubscribeAppliedType
	FUnsubscribeAppliedType UnsubscribeApplied;
	UnsubscribeApplied.RequestId = 3000;
	UnsubscribeApplied.TotalHostExecutionDurationMicros = 80000; 
	UnsubscribeApplied.QueryId = QueryId;
	UnsubscribeApplied.Rows = SubscribeRows;
	TEST_ROUNDTRIP(FUnsubscribeAppliedType, UnsubscribeApplied, "FUnsubscribeAppliedType");

	// UnsubscribeMultiAppliedType
	FUnsubscribeMultiAppliedType UnsubscribeMultiApplied;
	UnsubscribeMultiApplied.RequestId = 4000;
	UnsubscribeMultiApplied.TotalHostExecutionDurationMicros = 100000;
	UnsubscribeMultiApplied.QueryId = QueryId;
	UnsubscribeMultiApplied.Update = DatabaseUpdate;
	TEST_ROUNDTRIP(FUnsubscribeMultiAppliedType, UnsubscribeMultiApplied, "FUnsubscribeMultiAppliedType");


	// UServerMessageType
	FServerMessageType MessageInitialSubscription = FServerMessageType::InitialSubscription(InitialSubscription);
	TEST_ROUNDTRIP(FServerMessageType, MessageInitialSubscription, "FServerMessageType::InitialSubscription Variant");
	FServerMessageType MessageTransactionUpdate = FServerMessageType::TransactionUpdate(TransactionUpdate);
	TEST_ROUNDTRIP(FServerMessageType, MessageTransactionUpdate, "FServerMessageType::TransactionUpdate Variant");
	FServerMessageType MessageTransactionUpdateLight = FServerMessageType::TransactionUpdateLight(TransactionUpdateLight);
	TEST_ROUNDTRIP(FServerMessageType, MessageTransactionUpdateLight, "FServerMessageType::TransactionUpdateLight Variant");
	FServerMessageType MessageIdentityToken = FServerMessageType::IdentityToken(IdentityToken);
	TEST_ROUNDTRIP(FServerMessageType, MessageIdentityToken, "FServerMessageType::IdentityToken Variant");
	FServerMessageType MessageOneOffQueryResponse = FServerMessageType::OneOffQueryResponse(OneOffQueryResponse);
	TEST_ROUNDTRIP(FServerMessageType, MessageOneOffQueryResponse, "FServerMessageType::OneOffQueryResponse Variant");
	FServerMessageType MessageSubscribeApplied = FServerMessageType::SubscribeApplied(SubscribeApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageSubscribeApplied, "FServerMessageType::SubscribeApplied Variant");
	FServerMessageType MessageUnsubscribeApplied = FServerMessageType::UnsubscribeApplied(UnsubscribeApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageUnsubscribeApplied, "FServerMessageType::UnsubscribeApplied Variant");
	FServerMessageType MessageSubscriptionError = FServerMessageType::SubscriptionError(SubscriptionError);
	TEST_ROUNDTRIP(FServerMessageType, MessageSubscriptionError, "FServerMessageType::SubscriptionError Variant");
	FServerMessageType MessageSubscribeMultiApplied = FServerMessageType::SubscribeMultiApplied(SubscribeMultiApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageSubscribeMultiApplied, "FServerMessageType::SubscribeMultiApplied Variant");
	FServerMessageType MessageUnsubscribeMultiApplied = FServerMessageType::UnsubscribeMultiApplied(UnsubscribeMultiApplied);
	TEST_ROUNDTRIP(FServerMessageType, MessageUnsubscribeMultiApplied, "FServerMessageType::UnsubscribeMultiApplied Variant");

	return true;
}
