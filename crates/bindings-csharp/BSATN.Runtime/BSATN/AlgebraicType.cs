namespace SpacetimeDB.BSATN;

public interface ITypeRegistrar
{
    AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> type);
}

[SpacetimeDB.Type]
public partial struct AggregateElement
{
    public string? Name;

    public AlgebraicType AlgebraicType;

    public AggregateElement(string name, AlgebraicType algebraicType)
    {
        Name = name;
        AlgebraicType = algebraicType;
    }
}

[SpacetimeDB.Type]
public partial record AlgebraicType
    : SpacetimeDB.TaggedEnum<(
        int Ref,
        AggregateElement[] Sum,
        AggregateElement[] Product,
        AlgebraicType Array,
        Unit String,
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
        Unit I256,
        Unit U256,
        Unit F32,
        Unit F64
    )>
{
    public static readonly AlgebraicType Unit = new Product([]);

    // Special AlgebraicType that can be recognised by the SpacetimeDB `generate` CLI as an Option<T>.
    internal static AlgebraicType MakeOption(AlgebraicType someType) =>
        new Sum([new("some", someType), new("none", Unit)]);
}
