namespace SpacetimeDB.BSATN;

[Type]
public partial struct AggregateElement(string? name, AlgebraicType algebraicType)
{
    public string? Name = name;
    public AlgebraicType AlgebraicType = algebraicType;
}

[Type]
public partial struct MapElement(AlgebraicType key, AlgebraicType value)
{
    public AlgebraicType Key = key;
    public AlgebraicType Value = value;
}

[Type]
public partial record AlgebraicType : TaggedEnum<(
    int Ref,
    AggregateElement[] Sum,
    AggregateElement[] Product,
    AlgebraicType Array,
    MapElement Map,
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
    public static readonly Product Unit = new Product([]);

    // Special AlgebraicType that can be recognised by the SpacetimeDB `generate` CLI as an Option<T>.
    public static Sum MakeOption(AlgebraicType someType) =>
        new([new("some", someType), new("none", Unit)]);

    public static Sum MakeEnum<T>() where T : Enum =>
        new(Enum.GetNames(typeof(T))
            .Select(name => new AggregateElement(name, Unit))
            .ToArray());
}

public static class AlgebraicTypes
{
    public static readonly AlgebraicType String = new AlgebraicType.String(default);
    public static readonly AlgebraicType Bool = new AlgebraicType.Bool(default);
    public static readonly AlgebraicType I8 = new AlgebraicType.I8(default);
    public static readonly AlgebraicType U8 = new AlgebraicType.U8(default);
    public static readonly AlgebraicType I16 = new AlgebraicType.I16(default);
    public static readonly AlgebraicType U16 = new AlgebraicType.U16(default);
    public static readonly AlgebraicType I32 = new AlgebraicType.I32(default);
    public static readonly AlgebraicType U32 = new AlgebraicType.U32(default);
    public static readonly AlgebraicType I64 = new AlgebraicType.I64(default);
    public static readonly AlgebraicType U64 = new AlgebraicType.U64(default);
    public static readonly AlgebraicType I128 = new AlgebraicType.I128(default);
    public static readonly AlgebraicType U128 = new AlgebraicType.U128(default);
    public static readonly AlgebraicType I256 = new AlgebraicType.I256(default);
    public static readonly AlgebraicType U256 = new AlgebraicType.U256(default);
    public static readonly AlgebraicType F32 = new AlgebraicType.F32(default);
    public static readonly AlgebraicType F64 = new AlgebraicType.F64(default);

    public static readonly AlgebraicType I128Stdb = I128;
    public static readonly AlgebraicType U128Stdb = U128;

    public static readonly AlgebraicType U8Array = new AlgebraicType.Array(U8);

    public static readonly AlgebraicType Address = new AlgebraicType.Product([
        new("__address_bytes", U8Array)
    ]);

    public static readonly AlgebraicType Identity = new AlgebraicType.Product([
        new("__identity_bytes", U8Array)
    ]);
}
