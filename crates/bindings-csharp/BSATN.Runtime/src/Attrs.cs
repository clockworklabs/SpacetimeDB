using System;

namespace SpacetimeDB
{
    [AttributeUsage(
        AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
        Inherited = false,
        AllowMultiple = false
    )]
    public sealed class TypeAttribute : Attribute { }

    // This could be an interface, but using `record` forces C# to check that it can
    // only be applied on types that are records themselves.
    public record TaggedEnum<Variants>
        where Variants : struct { }
}
