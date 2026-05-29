namespace SpacetimeDB;

using System.Runtime.CompilerServices;

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TypeAttribute : Attribute { }

// Non-generic base record for sum types to avoid NativeAOT-LLVM vtable computation issues.
public abstract record TaggedEnum { }

// Generic version for backward compatibility; extends non-generic base to avoid vtable issues.
public abstract record TaggedEnum<Variants> : TaggedEnum
    where Variants : struct, ITuple { }
