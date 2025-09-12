# Notes for maintainers

The directory `sdk-unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings` is generated from [the SpacetimeDB client-api-messages](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/client-api-messages).
This is not automated.
Whenever the `client-api-messages` crate changes, you'll have to manually re-generate the definitions. 
See that crate's DEVELOP.md for how to do this.

**⚠️ IMPORTANT:** The following files/folders needs to be deleted everytime we re-generate:  
- `crates/sdk-unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Private/ModuleBindings`
- `crates/sdk-unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/ReducerBase.g.h`
- `crates/sdk-unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/SpacetimeDBClient.g.h`

# How to use AdditionalPluginDirectories

When integrating the SDK, you will need to place the plugin in the Engine or Project Plugins folder. Alternatively, you can specify additional directories where Unreal Engine should look for plugins. This is particularly useful when your plugin is located outside the standard `Plugins` folder of your project or engine installation, as in this case.

To use `AdditionalPluginDirectories`, add the key to your `.uproject` file, pointing to the exact location of your plugin's root directory.

## Example

Here's an example of how to include `AdditionalPluginDirectories` in your `.uproject` file:

```json
{
	"FileVersion": 3,
	"EngineAssociation": "5.6",
	"Category": "Tutorial",
	"Description": "Unreal Engine tutorial project",
	"Modules": [
		{
			"Name": "QuickstartChat",
			"Type": "Runtime",
			"LoadingPhase": "Default"
		}
	],
	"_Note": "Make sure to point to the SpacetimeDbSdk root directory, exact location, no relative paths.",
	"AdditionalPluginDirectories": [
		"C:/Github/clockworklabs/SpacetimeDB/crates/sdk-unreal"
	]
}
```



