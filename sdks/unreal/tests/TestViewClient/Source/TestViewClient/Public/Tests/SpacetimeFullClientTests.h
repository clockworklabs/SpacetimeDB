#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewQueryBuilderDirectSourcesTest,
    "SpacetimeDB.TestViewClient.ViewQueryBuilderDirectSourcesTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewSubscribeAllTablesTest,
    "SpacetimeDB.TestViewClient.ViewSubscribeAllTablesTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewBlueprintQueryBuilderFlowTest,
    "SpacetimeDB.TestViewClient.ViewBlueprintQueryBuilderFlowTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)

IMPLEMENT_SIMPLE_AUTOMATION_TEST(
    FViewBlueprintQueryBuilderRuntimeTest,
    "SpacetimeDB.TestViewClient.ViewBlueprintQueryBuilderRuntimeTest",
    EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter
)
