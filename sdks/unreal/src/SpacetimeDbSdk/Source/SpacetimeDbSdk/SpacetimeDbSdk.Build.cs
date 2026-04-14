using UnrealBuildTool;
using System.IO;

public class SpacetimeDbSdk : ModuleRules
{
	public SpacetimeDbSdk(ReadOnlyTargetRules Target) : base(Target)
	{
		// Set the module type to be a standard module
		PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

		// Set the module to use C++20 standard
		CppStandard = CppStandardVersion.Cpp20;


		// Enable exceptions for this module
		bEnableExceptions = true;

		PublicDependencyModuleNames.AddRange(
			new string[]
			{
				"Core",
				"WebSockets" // Required for WebSocket functionality
			}
			);
			
		
		PrivateDependencyModuleNames.AddRange(
			new string[]
			{
				"CoreUObject",
				"Engine",
				"JsonUtilities", // Required for JSON serialization/deserialization
				"Json" // Required for JSON handling
			}
			);
		
		
		DynamicallyLoadedModuleNames.AddRange(
			new string[]
			{
			}
			);
	}
}
