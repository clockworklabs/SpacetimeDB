#pragma once

#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"

class FTestCounter;

/**
 * Latent command that waits for a test counter to finish or time out.
 */
DEFINE_LATENT_AUTOMATION_COMMAND_FOUR_PARAMETER(FWaitForTestCounter, FAutomationTestBase&, Test, FString, TestName, TSharedPtr<FTestCounter>, Counter, double, StartTime);


/** Tests for calling simple procedures.  */
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FProcedureTest, "SpacetimeDB.TestProcClient.ProcedureTest", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
