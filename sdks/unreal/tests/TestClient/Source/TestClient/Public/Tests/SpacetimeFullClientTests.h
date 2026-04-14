#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"

class FTestCounter;

/**
 * Latent command that waits for a test counter to finish or time out.
 */
DEFINE_LATENT_AUTOMATION_COMMAND_FOUR_PARAMETER(FWaitForTestCounter, FAutomationTestBase&, Test, FString, TestName, TSharedPtr<FTestCounter>, Counter, double, StartTime);


/** Tests inserting primitive types by calling reducers and verifying the results.  */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertPrimitiveTest, "SpacetimeDB.TestClient.InsertPrimitiveTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests cancelling a subscription before it is applied. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FSubscribeAndCancelTest, "SpacetimeDB.TestClient.SubscribeAndCancelTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests unsubscribing after a subscription has been applied. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FSubscribeAndUnsubscribeTest, "SpacetimeDB.TestClient.SubscribeAndUnsubscribeTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests that subscription errors are reported to callbacks. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FSubscriptionErrorSmokeTest, "SpacetimeDB.TestClient.SubscriptionErrorSmokeTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests deleting primitive rows. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FDeletePrimitiveTest, "SpacetimeDB.TestClient.DeletePrimitiveTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests updating primitive rows with primary keys. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FUpdatePrimitiveTest, "SpacetimeDB.TestClient.UpdatePrimitiveTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests inserting identity type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertOneIdentityTest, "SpacetimeDB.TestClient.InsertIdentityTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting caller ConnectionId type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertCallerIdentityTest, "SpacetimeDB.TestClient.InsertCallerIdentityTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests deleting identity rows. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FDeleteUniqueIdentityTest, "SpacetimeDB.TestClient.DeleteIdentityTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests updating unique identity rows. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FUpdatePkIdentityTest, "SpacetimeDB.TestClient.UpdateIdentityTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests inserting one ConnectionId type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertOneConnectionIdTest, "SpacetimeDB.TestClient.InsertConnectionIdTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting caller ConnectionId type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertCallerConnectionIdTest, "SpacetimeDB.TestClient.InsertCallerConnectionIdTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests deleting ConnectionId rows. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FDeletePkConnectionIdTest, "SpacetimeDB.TestClient.DeleteConnectionIdTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests updating unique identity rows. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FUpdatePkConnectionIdTest, "SpacetimeDB.TestClient.UpdateConnectionIdTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting unique ConnectionId type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertUniqueConnectionIdTest, "SpacetimeDB.TestClient.InsertUniqueConnectionIdTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests inserting timestamp type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertOneTimestampTest, "SpacetimeDB.TestClient.InsertTimestampTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting timestamp type by calling reducer and verifying the result. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertCallTimestampTest, "SpacetimeDB.TestClient.InsertCallTimestampTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests on reducer. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FOnReducerTest, "SpacetimeDB.TestClient.OnReducerTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests on fail reducer. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FOnFailReducerTest, "SpacetimeDB.TestClient.FailReducerTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests inserting vector types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertVecTest, "SpacetimeDB.TestClient.InsertVecTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting some optional types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertOptionSomeTest, "SpacetimeDB.TestClient.InsertOptionSomeTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting none optional types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertOptionNoneTest, "SpacetimeDB.TestClient.InsertOptionNoneTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** Tests inserting Result Ok types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertResultOkTest, "SpacetimeDB.TestClient.InsertResultOkTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting Result Err types. */
//IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertResultErrTest, "SpacetimeDB.TestClient.InsertResultErrTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

/** Tests inserting struct types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertStructTest, "SpacetimeDB.TestClient.InsertStructTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting simple enum types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertSimpleEnumTest, "SpacetimeDB.TestClient.InsertSimpleEnumTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests inserting enum with payload types. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertEnumWithPayloadTest, "SpacetimeDB.TestClient.InsertEnumWithPayloadTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests deleting large table. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertDeleteLargeTableTest, "SpacetimeDB.TestClient.InsertDeleteLargeTableTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests inserting primitives and getting back string to compare to. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertPrimitivesAsStringTest, "SpacetimeDB.TestClient.InsertPrimitivesAsStringsTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests authentication. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReauth1Test, "SpacetimeDB.TestClient.ReauthPart1Test", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests reauthenticate using old credentials. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReauth2Test, "SpacetimeDB.TestClient.ReauthPart2Test", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests should file logic. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FShouldFailTest, "SpacetimeDB.TestClient.ShouldFailTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests subscribe caller always notified. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FCallerAlwaysNotifiedTest, "SpacetimeDB.TestClient.CallerAlwaysNotifiedTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests subscribe all select star. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FSubscribeAllSelectStarTest, "SpacetimeDB.TestClient.SubscribeAllSelectStarTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests row deduplication behavior. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FRowDeduplicationTest, "SpacetimeDB.TestClient.RowDeduplicationTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests row deduplication with join between pk_u32 and unique_u32. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FRowDeduplicationJoinRAndSTest, "SpacetimeDB.TestClient.RowDeduplicationJoinRAndSTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests row deduplication with r join s and r join t queries. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FRowDeduplicationRJoinSandRJoinTTest, "SpacetimeDB.TestClient.RowDeduplicationRJoinSAndRJoinTTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests lhs join update behavior. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FLhsJoinUpdateTest, "SpacetimeDB.TestClient.LhsJoinUpdateTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests lhs join update with disjoint queries. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FLhsJoinUpdateDisjointQueriesTest, "SpacetimeDB.TestClient.LhsJoinUpdateDisjointQueriesTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests bag semantics for joins within a single query. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FIntraQueryBagSemanticsForJoinTest, "SpacetimeDB.TestClient.IntraQueryBagSemanticsForJoinTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests different compression algorithms for multiple clients. @Note: Only one compresstion GZip compresstion is implemented, add this test in the future if more compresstion is added. */
//IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTwoDifferentCompressionAlgosTest, "SpacetimeDB.TestClient.TwoDifferentCompressionAlgosTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests parameterized subscriptions. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FParameterizedSubscriptionTest, "SpacetimeDB.TestClient.ParameterizedSubscriptionTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests RLS controlled subscription visibility. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FRlsSubscriptionTest, "SpacetimeDB.TestClient.RlsSubscriptionTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests pk simple enum updates. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FPkSimpleEnumTest, "SpacetimeDB.TestClient.PkSimpleEnumTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
/** Tests indexed simple enum updates. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FIndexedSimpleEnumTest, "SpacetimeDB.TestClient.IndexedSimpleEnumTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)


/** Tests overlapping subscriptions. */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FOverlappingSubscriptionsTest, "SpacetimeDB.TestClient.OverlappingSubscriptionsTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertCallUuidV4Test, "SpacetimeDB.TestClient.InsertCallUuidV4Test", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FInsertCallUuidV7Test, "SpacetimeDB.TestClient.InsertCallUuidV7Test", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
