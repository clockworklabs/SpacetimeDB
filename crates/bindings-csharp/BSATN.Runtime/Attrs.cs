namespace SpacetimeDB;

using System;

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TypeAttribute : Attribute { }

public interface TaggedEnum<Variants>
    where Variants : struct { }
