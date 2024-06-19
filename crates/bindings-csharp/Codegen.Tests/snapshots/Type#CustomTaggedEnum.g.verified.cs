//HintName: CustomTaggedEnum.g.cs
#nullable enable

partial record CustomTaggedEnum
{
    private CustomTaggedEnum() { }

    internal enum @enum : byte
    {
        IntVariant,
        StringVariant
    }

    public sealed record IntVariant(int IntVariant_) : CustomTaggedEnum;

    public sealed record StringVariant(string StringVariant_) : CustomTaggedEnum;

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<CustomTaggedEnum>
    {
        internal static readonly SpacetimeDB.BSATN.Enum<@enum> __enumTag = new();
        internal static readonly SpacetimeDB.BSATN.I32 IntVariant = new();
        internal static readonly SpacetimeDB.BSATN.String StringVariant = new();

        public CustomTaggedEnum Read(System.IO.BinaryReader reader) =>
            __enumTag.Read(reader) switch
            {
                @enum.IntVariant => new IntVariant(IntVariant.Read(reader)),
                @enum.StringVariant => new StringVariant(StringVariant.Read(reader)),
                _
                    => throw new System.InvalidOperationException(
                        "Invalid tag value, this state should be unreachable."
                    )
            };

        public void Write(System.IO.BinaryWriter writer, CustomTaggedEnum value)
        {
            switch (value)
            {
                case IntVariant(var inner):
                    __enumTag.Write(writer, @enum.IntVariant);
                    IntVariant.Write(writer, inner);
                    break;
                case StringVariant(var inner):
                    __enumTag.Write(writer, @enum.StringVariant);
                    StringVariant.Write(writer, inner);
                    break;
            }
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<CustomTaggedEnum>(
                typeRef => new SpacetimeDB.BSATN.AlgebraicType.Sum(
                    new SpacetimeDB.BSATN.AggregateElement[]
                    {
                        new(nameof(IntVariant), IntVariant.GetAlgebraicType(registrar)),
                        new(nameof(StringVariant), StringVariant.GetAlgebraicType(registrar))
                    }
                )
            );
    }
} // CustomTaggedEnum
