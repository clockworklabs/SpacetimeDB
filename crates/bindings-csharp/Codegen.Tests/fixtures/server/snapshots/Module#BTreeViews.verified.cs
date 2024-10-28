﻿//HintName: BTreeViews.cs
// <auto-generated />
#nullable enable

partial struct BTreeViews : SpacetimeDB.Internal.ITable<BTreeViews>
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        Id = BSATN.Id.Read(reader);
        X = BSATN.X.Read(reader);
        Y = BSATN.Y.Read(reader);
        Faction = BSATN.Faction.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.Id.Write(writer, Id);
        BSATN.X.Write(writer, X);
        BSATN.Y.Write(writer, Y);
        BSATN.Faction.Write(writer, Faction);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<BTreeViews>
    {
        internal static readonly SpacetimeDB.Identity.BSATN Id = new();
        internal static readonly SpacetimeDB.BSATN.U32 X = new();
        internal static readonly SpacetimeDB.BSATN.U32 Y = new();
        internal static readonly SpacetimeDB.BSATN.String Faction = new();

        public BTreeViews Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<BTreeViews>(reader);

        public void Write(System.IO.BinaryWriter writer, BTreeViews value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<BTreeViews>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[]
                {
                    new(nameof(Id), Id.GetAlgebraicType(registrar)),
                    new(nameof(X), X.GetAlgebraicType(registrar)),
                    new(nameof(Y), Y.GetAlgebraicType(registrar)),
                    new(nameof(Faction), Faction.GetAlgebraicType(registrar))
                }
            ));
    }

    static IEnumerable<SpacetimeDB.Internal.TableDesc> SpacetimeDB.Internal.ITable<BTreeViews>.MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        [
            new(
                new(
                    TableName: nameof(SpacetimeDB.Local.BTreeViews),
                    Columns:
                    [
                        new(nameof(Id), BSATN.Id.GetAlgebraicType(registrar)),
                        new(nameof(X), BSATN.X.GetAlgebraicType(registrar)),
                        new(nameof(Y), BSATN.Y.GetAlgebraicType(registrar)),
                        new(nameof(Faction), BSATN.Faction.GetAlgebraicType(registrar))
                    ],
                    Indexes:
                    [
                        new(
                            "bt_BTreeViews_Location",
                            false,
                            SpacetimeDB.Internal.IndexType.BTree,
                            [1, 2]
                        ),
                        new(
                            "bt_BTreeViews_Faction",
                            false,
                            SpacetimeDB.Internal.IndexType.BTree,
                            [3]
                        ),
                        new(
                            "idx_BTreeViews_BTreeViews_Id_unique",
                            true,
                            SpacetimeDB.Internal.IndexType.BTree,
                            [0]
                        )
                    ],
                    Constraints:
                    [
                        new("BTreeViews_Id", (byte)SpacetimeDB.Internal.ColumnAttrs.PrimaryKey, [0])
                    ],
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
} // BTreeViews
