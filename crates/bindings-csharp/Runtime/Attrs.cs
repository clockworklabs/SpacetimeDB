namespace SpacetimeDB;

using System;
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
public sealed class TableAttribute : Attribute
{
    public bool Public { get; init; }
}

[AttributeUsage(AttributeTargets.Field, Inherited = false, AllowMultiple = false)]
public sealed class ColumnAttribute(ColumnAttrs type) : Attribute
{
    public ColumnAttrs Type => type;
}
