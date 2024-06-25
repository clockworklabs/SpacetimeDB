// Generated code produced compilation errors:
// (23,41): error CS1001: Identifier expected
// (23,62): error CS1525: Invalid expression term '.'
// (23,93): error CS1525: Invalid expression term ')'

//HintName: FFI.g.cs
#nullable enable

using static SpacetimeDB.RawBindings;
using SpacetimeDB.Module;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using static SpacetimeDB.Runtime;
using System.Diagnostics.CodeAnalysis;

using Buffer = SpacetimeDB.RawBindings.Buffer;

static class ModuleRegistration {
#if EXPERIMENTAL_WASM_AOT
    // In AOT mode we're building a library.
    // Main method won't be called automatically, so we need to export it as a preinit function.
    [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
#else
    // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
    [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(FFI))]
#endif
    public static void Main() {
        FFI.RegisterReducer<Problematic.<invalid-global-code>.ReducerWithNonVoidReturnType>();
        
    }

// Exports only work from the main assembly, so we need to generate forwarding methods.
#if EXPERIMENTAL_WASM_AOT
    [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
    public static Buffer __describe_module__() => FFI.__describe_module__();

    [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
    public static Buffer __call_reducer__(
        uint id,
        Buffer caller_identity,
        Buffer caller_address,
        ulong timestamp,
        Buffer args
    ) => FFI.__call_reducer__(id, caller_identity, caller_address, timestamp, args);
#endif
}