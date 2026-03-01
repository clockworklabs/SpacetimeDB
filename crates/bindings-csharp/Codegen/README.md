> ⚠️ **Internal Project** ⚠️
>
> This project is intended for internal use only. It is **not** stable and may change without notice.

See the [C# module library reference](https://spacetimedb.com/docs/modules/c-sharp) for stable, user-facing documentation.

## Internal documentation

This project contains Roslyn [incremental source generators](https://github.com/dotnet/roslyn/blob/main/docs/features/incremental-generators.md) that augment tables and reducers with static methods for self-describing and registration.

SpacetimeDB modules are compiled to WebAssembly modules that expose a specific interface; see the [module ABI reference](https://spacetimedb.com/docs/webassembly-abi). This interface is implemented in the generated `FFI` class; see [`../Codegen.Tests/fixtures/server/snapshots/Module#FFI.verified.cs`](../Codegen.Tests/fixtures/server/snapshots/Module#FFI.verified.cs) for an example of what this generated code looks like.

The source generators are implemented via several attributes usable in module code:

- `[SpacetimeDB.Table]` - generates code to register this table in the `FFI` upon startup so that they can be enumerated by the `__describe_module__` FFI API. It implies `[SpacetimeDB.Type]`, so you must not specify both attributes on the same struct.

- `[SpacetimeDB.Reducer]` - generates code to register a static function as a SpacetimeDB reducer in the `FFI` upon startup and creates a wrapper that will parse SATS binary blob into individual arguments and invoke the underlying function for the `__call_reducer__` FFI API.
