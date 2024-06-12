// Note: this file is also used from Codegen.csproj.

using System;
using System.Runtime.CompilerServices;

namespace SpacetimeDB
{
    [SpacetimeDB.Type]
    public readonly partial struct Unit { }

    [AttributeUsage(
        AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Enum,
        Inherited = false,
        AllowMultiple = false
    )]
    public sealed class TypeAttribute : Attribute { }

    // This could be an interface, but using `record` forces C# to check that it can
    // only be applied on types that are records themselves.
    public abstract record TaggedEnum<Variants>
        where Variants : struct
        // We want to statically check that the type is a tuple, but ITuple is only available in .NET Standard 2.1.
        //
        // Roslyn codegen - which also references this file - is limited to .NET Standard 2.0, but
        // in those we don't care about this constraint much anyway, so just ifdef it out.
        #if NETSTANDARD2_1_OR_GREATER
        , ITuple
        #endif
        { }
}
