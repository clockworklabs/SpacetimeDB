using System;

namespace SpacetimeDB.BSATN
{
    public interface ITypeRegistrar
    {
        AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> type);
    }

    [SpacetimeDB.Type]
    public partial struct AggregateElement
    {
        public string? Name;
        public AlgebraicType AlgebraicType;

        public AggregateElement(string? name, AlgebraicType algebraicType)
        {
            Name = name;
            AlgebraicType = algebraicType;
        }
    }

    [SpacetimeDB.Type]
    public partial struct MapElement
    {
        public AlgebraicType Key;
        public AlgebraicType Value;

        public MapElement(AlgebraicType key, AlgebraicType value)
        {
            Key = key;
            Value = value;
        }
    }

    [SpacetimeDB.Type]
    public partial record BuiltinType
        : SpacetimeDB.TaggedEnum<(
            Unit Bool,
            Unit I8,
            Unit U8,
            Unit I16,
            Unit U16,
            Unit I32,
            Unit U32,
            Unit I64,
            Unit U64,
            Unit I128,
            Unit U128,
            Unit F32,
            Unit F64,
            Unit String,
            AlgebraicType Array,
            MapElement Map
        )> { }

    [SpacetimeDB.Type]
    public partial record AlgebraicType
        : SpacetimeDB.TaggedEnum<(
            AggregateElement[] Sum,
            AggregateElement[] Product,
            BuiltinType Builtin,
            int Ref
        )>
    {
        public static implicit operator AlgebraicType(BuiltinType builtin)
        {
            return new Builtin(builtin);
        }

        public static readonly AlgebraicType Unit = new Product(Array.Empty<AggregateElement>());

        // Special AlgebraicType that can be recognised by the SpacetimeDB `generate` CLI as an Option<T>.
        internal static AlgebraicType MakeOption(AlgebraicType someType) =>
            new Sum(new AggregateElement[] { new("some", someType), new("none", Unit) });
    }
}
