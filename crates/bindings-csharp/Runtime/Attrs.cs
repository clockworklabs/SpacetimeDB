namespace SpacetimeDB;

using System;
using System.Runtime.CompilerServices;
using SpacetimeDB.Module;

[AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
public sealed class ReducerAttribute(string? name = null) : Attribute
{
    public string? Name => name;
}

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TableAttribute : Attribute { }

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TypeAttribute : Attribute { }

// This could be an interface, but using `record` forces C# to check that it can
// only be applied on types that are records themselves.
public record TaggedEnum<Variants>
    where Variants : struct, ITuple { }

[AttributeUsage(AttributeTargets.Field, Inherited = false, AllowMultiple = false)]
public sealed class ColumnAttribute(ColumnAttrs type) : Attribute
{
    public ColumnAttrs Type => type;
}
