# Notes for maintainers

The generated Unreal bindings under:

- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings`
- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Private/ModuleBindings`

come from SpacetimeDB codegen (`--lang unrealcpp`) and websocket schema definitions in `crates/client-api-messages`.

This is not automated; regenerate manually whenever websocket message schemas or Unreal codegen behavior changes.

## WS v2 websocket schema regeneration workflow

Run from repo root:

```powershell
# 1) Produce WS v2 schema JSON from canonical source
cargo run -p spacetimedb-client-api-messages --example get_ws_schema_v2 > crates/client-api-messages/ws_schema_v2.json

# 2) Regenerate Unreal bindings from WS v2 schema
cargo run -p spacetimedb-cli -- generate --lang unrealcpp `
  --module-def crates/client-api-messages/ws_schema_v2.json `
  --uproject-dir sdks/unreal/src/SpacetimeDbSdk `
  --unreal-module-name SpacetimeDbSdk `
  --yes
```

## Cleanup before regeneration

Delete these generated paths before rerunning generation when schema/model changes are significant:

- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Private/ModuleBindings`
- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/ReducerBase.g.h`
- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/SpacetimeDBClient.g.h`
- `sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Private/ModuleBindings/SpacetimeDBClient.g.cpp`

This avoids UnrealHeaderTool duplicate symbol/header conflicts with the `sdks/unreal/tests/TestClient` generated module bindings.

## Fast validation loop

For rapid iteration, run a single Unreal harness test instead of the full suite:

```powershell
cargo test -p sdk-unreal-test-harness --test test insert_primitive -- --nocapture
```

Prerequisite:

- `UE_ROOT_PATH` must point to the Unreal Engine install root (for example `C:/Program Files/Epic Games/UE_5.6`).

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



