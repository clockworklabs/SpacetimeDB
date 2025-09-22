> ⚠️ **Internal Project** ⚠️
>
> This project is intended for internal use only. It is **not** stable and may change without notice.

# SpacetimeDB.Runtime

This project contains the runtime bindings for SpacetimeDB WebAssembly modules. See the [C# module library reference](https://spacetimedb.com/docs/modules/c-sharp) for stable, user-facing documentation.

SpacetimeDB modules are compiled to WebAssembly modules that expose a specific interface; see the [module ABI reference](https://spacetimedb.com/docs/webassembly-abi).

The runtime bindings are currently implementing via `Wasi.Sdk` package, which is a .NET implementation of the [WASI](https://wasi.dev/) standard. This is likely to change in the future.

While not really documented, it allows to build raw WebAssembly modules with custom bindings as well, which is what we're using here. The process is somewhat complicated, but here are the steps:

- `bindings.c` declares raw C bindings to the SpacetimeDB FFI _imports_ and marks them with attributes like `__attribute__((import_module("spacetime"), import_name("_insert")))` that make them WebAssembly imports. (unfortunately, function name duplication is currently unavoidable)
- `bindings.c` implements a bunch of Mono-compatible wrappers that convert between Mono types and raw types expected by the SpacetimeDB FFI and invoke corresponding raw bindings.
- `Runtime.cs` declares corresponding functions with compatible signatures for Mono-compatible wrappers to attach to. It marks them all with `[MethodImpl(MethodImplOptions.InternalCall)]`.
- `bindings.c` attaches all those Mono-compatible wrappers to their C# declarations in a `mono_stdb_attach_bindings` function.
- `bindings.c` adds FFI-compatible _exports_ that search for a method by assembly name, namespace, class name and a method name in the Mono runtime and invoke it. Those exports are marked with attributes like `__attribute__((export_name("__call_reducer__")))` so that they're exported from Wasm by the linker.
- Finally, `bindings.c` implements no-op shims for all the WASI APIs so that they're linked internally and not attempted to be imported from the runtime itself.

The result is a WebAssembly module FFI-compatible with SpacetimeDB and with no WASI imports, which is what we need.

## Regenerating RawModuleDef
To regenenerate the `Autogen` folder, run:

```sh
cargo run -p spacetimedb-codegen --example regen-csharp-moduledef
```

This folder contains the type definitions used to serialize the `RawModuleDef` that is returned by `__describe_module__`.