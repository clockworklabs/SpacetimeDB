#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"
#include "Tests/TestHandler.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewPkQueryBuilderDirectSourcesTest,
    "SpacetimeDB.TestViewPkClient.ViewPkQueryBuilderDirectSourcesTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewPkQueryBuilderSemijoinTest,
    "SpacetimeDB.TestViewPkClient.ViewPkQueryBuilderSemijoinTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewPkSubscribeAllTablesTest,
    "SpacetimeDB.TestViewPkClient.ViewPkSubscribeAllTablesTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewPkRuntimeUpdatePairingTest,
    "SpacetimeDB.TestViewPkClient.ViewPkRuntimeUpdatePairingTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)
