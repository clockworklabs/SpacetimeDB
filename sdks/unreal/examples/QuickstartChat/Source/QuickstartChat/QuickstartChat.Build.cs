// Fill out your copyright notice in the Description page of Project Settings.

using System.IO;
using UnrealBuildTool;

public class QuickstartChat : ModuleRules
{
	public QuickstartChat(ReadOnlyTargetRules Target) : base(Target)
	{
        // This module is used to demonstrate the SpacetimeDb SDK in Unreal Engine.
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

        PublicDependencyModuleNames.AddRange(new string[] 
        {
            "Core",
            "CoreUObject",
            "Engine",
            "InputCore",
            "SpacetimeDbSdk"  // Ensure the SpacetimeDb SDK is included as a dependency
        });

		PrivateDependencyModuleNames.AddRange(new string[] {  });
    }
}
