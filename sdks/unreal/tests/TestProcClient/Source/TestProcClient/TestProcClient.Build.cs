// Fill out your copyright notice in the Description page of Project Settings.

using UnrealBuildTool;

public class TestProcClient : ModuleRules
{
	public TestProcClient(ReadOnlyTargetRules Target) : base(Target)
	{
        // Set the module type to be a standard module
        PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

        // Set the module to use C++20 standard
        CppStandard = CppStandardVersion.Cpp20;


        // Enable exceptions for this module
        bEnableExceptions = true;


        PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "InputCore", "SpacetimeDbSdk" });

		PrivateDependencyModuleNames.AddRange(new string[] { "DeveloperSettings" });

	}
}
