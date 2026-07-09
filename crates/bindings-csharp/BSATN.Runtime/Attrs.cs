namespace SpacetimeDB;

using System.IO;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TypeAttribute : Attribute { }

// This could be an interface, but using `record` forces C# to check that it can
// only be applied on types that are records themselves.
public abstract record TaggedEnum<Variants> : IStructuralWrite
    where Variants : struct, ITuple
{
    public abstract void WriteFields(BinaryWriter writer);
}
