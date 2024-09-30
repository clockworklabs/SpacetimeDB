﻿//HintName: TestUniqueNotEquatable.cs
// <auto-generated />
#nullable enable

partial struct TestUniqueNotEquatable : SpacetimeDB.Internal.ITable<TestUniqueNotEquatable>
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        UniqueField = BSATN.UniqueField.Read(reader);
        PrimaryKeyField = BSATN.PrimaryKeyField.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.UniqueField.Write(writer, UniqueField);
        BSATN.PrimaryKeyField.Write(writer, PrimaryKeyField);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<TestUniqueNotEquatable>
    {
        internal static readonly SpacetimeDB.BSATN.ValueOption<
            int,
            SpacetimeDB.BSATN.I32
        > UniqueField = new();
        internal static readonly SpacetimeDB.BSATN.Enum<TestEnumWithExplicitValues> PrimaryKeyField =
            new();

        public TestUniqueNotEquatable Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<TestUniqueNotEquatable>(reader);

        public void Write(System.IO.BinaryWriter writer, TestUniqueNotEquatable value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<TestUniqueNotEquatable>(
                _ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                    new SpacetimeDB.BSATN.AggregateElement[]
                    {
                        new(nameof(UniqueField), UniqueField.GetAlgebraicType(registrar)),
                        new(nameof(PrimaryKeyField), PrimaryKeyField.GetAlgebraicType(registrar))
                    }
                )
            );
    }

    static IEnumerable<SpacetimeDB.Internal.TableDesc> SpacetimeDB.Internal.ITable<TestUniqueNotEquatable>.MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        [
            new(
                new(
                    TableName: nameof(SpacetimeDB.Local.TestUniqueNotEquatable),
                    Columns:
                    [
                        new(nameof(UniqueField), BSATN.UniqueField.GetAlgebraicType(registrar)),
                        new(
                            nameof(PrimaryKeyField),
                            BSATN.PrimaryKeyField.GetAlgebraicType(registrar)
                        )
                    ],
                    Indexes: [],
                    Constraints: [],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "private",
                    Scheduled: null
                ),
                (uint)
                    (
                        (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
                    ).Ref_
            ),
        ];
} // TestUniqueNotEquatable
