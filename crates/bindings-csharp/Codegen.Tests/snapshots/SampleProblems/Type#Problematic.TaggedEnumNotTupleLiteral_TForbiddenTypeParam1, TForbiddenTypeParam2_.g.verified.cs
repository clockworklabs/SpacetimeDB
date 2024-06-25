//HintName: Problematic.TaggedEnumNotTupleLiteral_TForbiddenTypeParam1, TForbiddenTypeParam2_.g.cs
#nullable enable

namespace Problematic
{
    partial record TaggedEnumNotTupleLiteral<TForbiddenTypeParam1, TForbiddenTypeParam2>
    {
        private TaggedEnumNotTupleLiteral() { }

        internal enum @enum : byte
        {
            Item1
        }

        public sealed record Item1(int Item1_) : TaggedEnumNotTupleLiteral;

        public readonly partial struct BSATN
            : SpacetimeDB.BSATN.IReadWrite<TaggedEnumNotTupleLiteral>
        {
            internal static readonly SpacetimeDB.BSATN.Enum<@enum> __enumTag = new();
            internal static readonly SpacetimeDB.BSATN.I32 Item1 = new();

            public TaggedEnumNotTupleLiteral Read(System.IO.BinaryReader reader) =>
                __enumTag.Read(reader) switch
                {
                    @enum.Item1 => new Item1(Item1.Read(reader)),
                    _
                        => throw new System.InvalidOperationException(
                            "Invalid tag value, this state should be unreachable."
                        )
                };

            public void Write(System.IO.BinaryWriter writer, TaggedEnumNotTupleLiteral value)
            {
                switch (value)
                {
                    case Item1(var inner):
                        __enumTag.Write(writer, @enum.Item1);
                        Item1.Write(writer, inner);
                        break;
                }
            }

            public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
                SpacetimeDB.BSATN.ITypeRegistrar registrar
            ) =>
                registrar.RegisterType<TaggedEnumNotTupleLiteral>(
                    typeRef => new SpacetimeDB.BSATN.AlgebraicType.Sum(
                        new SpacetimeDB.BSATN.AggregateElement[]
                        {
                            new(nameof(Item1), Item1.GetAlgebraicType(registrar))
                        }
                    )
                );
        }
    } // TaggedEnumNotTupleLiteral<TForbiddenTypeParam1, TForbiddenTypeParam2>
} // namespace
