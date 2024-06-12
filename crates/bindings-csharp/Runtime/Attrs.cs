// Note: this file is also used from BSATN.Codegen.csproj.

using System;
using SpacetimeDB.Module;

namespace SpacetimeDB
{
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

    // TODO: flatten this into the top namespace with all other user-visible types.
    namespace Module
    {
        [System.Flags]
        public enum ColumnAttrs : byte
        {
            UnSet = 0b0000,
            Indexed = 0b0001,
            AutoInc = 0b0010,
            Unique = Indexed | 0b0100,
            Identity = Unique | AutoInc,
            PrimaryKey = Unique | 0b1000,
            PrimaryKeyAuto = PrimaryKey | AutoInc,
            PrimaryKeyIdentity = PrimaryKey | Identity,
        }
    }

    [AttributeUsage(AttributeTargets.Field, Inherited = false, AllowMultiple = false)]
    public sealed class ColumnAttribute(ColumnAttrs type) : Attribute
    {
        public ColumnAttrs Type => type;
    }
}
